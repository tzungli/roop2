use tauri::{AppHandle, Emitter, State};
use roo_dlna::{discover_dlna_devices, probe_device_ip, DlnaDevice, DlnaManager};

pub fn resolve_local_ip_for_target(target_host_or_ip: &str) -> String {
    let host = if let Ok(url) = reqwest::Url::parse(target_host_or_ip) {
        url.host_str().unwrap_or(target_host_or_ip).to_string()
    } else if let Some((h, _)) = target_host_or_ip.split_once(':') {
        h.to_string()
    } else {
        target_host_or_ip.to_string()
    };

    if !host.is_empty() && host != "127.0.0.1" && host != "localhost" {
        if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
            let target_sock = format!("{}:1900", host);
            if socket.connect(&target_sock).is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    let ip = addr.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() {
                        return ip.to_string();
                    }
                }
            }
        }
    }

    // Secondary fallback: probe towards a common reachable address
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip();
                if !ip.is_loopback() && !ip.is_unspecified() {
                    return ip.to_string();
                }
            }
        }
    }

    "127.0.0.1".to_string()
}

#[tauri::command]
pub async fn dlna_get_devices(manager: State<'_, DlnaManager>) -> Result<Vec<DlnaDevice>, String> {
    Ok(manager.get_devices())
}

#[tauri::command]
pub async fn dlna_scan(app: AppHandle, manager: State<'_, DlnaManager>) -> Result<Vec<DlnaDevice>, String> {
    let devices = discover_dlna_devices(3).await.map_err(|e| e.to_string())?;
    manager.set_devices(devices.clone());
    let _ = app.emit("dlna:devicesChanged", &devices);
    Ok(devices)
}

#[tauri::command]
pub async fn dlna_probe_ip(manager: State<'_, DlnaManager>, ip: String) -> Result<DlnaDevice, String> {
    let dev = probe_device_ip(&ip).await.map_err(|e| e.to_string())?;
    let mut list = manager.get_devices();
    if !list.iter().any(|d| d.id == dev.id) {
        list.push(dev.clone());
        manager.set_devices(list);
    }
    Ok(dev)
}

#[tauri::command]
pub async fn dlna_select_device(manager: State<'_, DlnaManager>, device_id: String) -> Result<bool, String> {
    Ok(manager.select_device(&device_id))
}

static TOKEN_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[tauri::command]
pub async fn dlna_play_track(
    app: AppHandle,
    manager: State<'_, DlnaManager>,
    device_id: String,
    track: serde_json::Value,
    start_sec: Option<f64>,
    dsp_options: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let title = track.get("title").and_then(|v| v.as_str()).unwrap_or("Track");
    let artist = track.get("artist").and_then(|v| v.as_str()).unwrap_or("Artist");
    let album = track.get("album").and_then(|v| v.as_str()).unwrap_or("Album");
    let file_path = track.get("filePath").or_else(|| track.get("path")).and_then(|v| v.as_str()).unwrap_or("");
    let raw_cover = track.get("cover").or_else(|| track.get("coverPath")).and_then(|v| v.as_str()).unwrap_or("");
    let start = start_sec.unwrap_or(0.0);
    let duration_sec = track.get("duration").and_then(|v| v.as_f64());

    let dev = manager.get_devices().into_iter().find(|d| d.id == device_id);
    let target_ip_or_host = dev.as_ref()
        .map(|d| if !d.ip.is_empty() { d.ip.clone() } else { d.location.clone() })
        .unwrap_or_default();

    let local_ip = resolve_local_ip_for_target(&target_ip_or_host);

    let stream_token = format!(
        "{:x}{:04x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        TOKEN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );

    // Register cover for token and form clean cover URL for DLNA renderer
    let cover_url = format!("http://{}:39840/cover/{}/cover.jpg", local_ip, stream_token);
    roo_streamer::register_cover_for_token(
        &stream_token,
        if raw_cover.is_empty() { None } else { Some(raw_cover.to_string()) },
        if file_path.is_empty() { None } else { Some(file_path.to_string()) },
    );

    let (target_rate, dsd_mode, dsd_order, filter_phase, peq_json, conv_json, ultrasonic_filter, dither) =
        if let Some(opts) = &dsp_options {
            let rate = opts.get("sampleRate").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let dsd_m = opts.get("dsdMode").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let dsd_o = opts.get("dsdOrder").and_then(|v| v.as_u64()).unwrap_or(5) as u8;
            let phase = opts.get("filterPhase").and_then(|v| v.as_str()).unwrap_or("linear").to_string();
            let peq = opts.get("peq").map(|v| v.to_string()).unwrap_or_default();
            let conv = opts.get("conv").map(|v| v.to_string()).unwrap_or_default();
            let ultra = opts.get("ultrasonicFilter").and_then(|v| v.as_str()).unwrap_or("30k").to_string();
            let dith = opts.get("dither").and_then(|v| v.as_str()).map(|s| s.to_string());
            (rate, dsd_m, dsd_o, phase, peq, conv, ultra, dith)
        } else {
            (0, String::new(), 5, "linear".to_string(), String::new(), String::new(), "30k".to_string(), None)
        };

    let is_native_dsf = file_path.to_lowercase().ends_with(".dsf");
    let is_dsd_upsample = !dsd_mode.is_empty();
    let is_dsd = is_dsd_upsample || is_native_dsf;

    // For DLNA network streaming, clamp DSD512 to DSD256 (11.2896 MHz).
    // Most UPnP/DLNA renderers cannot process 22.58 MHz streams over network HTTP and will stall.
    let effective_dsd_mode = if is_dsd_upsample {
        if dsd_mode.eq_ignore_ascii_case("dsd512") {
            "dsd256".to_string()
        } else {
            dsd_mode.clone()
        }
    } else {
        dsd_mode.clone()
    };

    let dsd_hz: Option<u32> = if is_dsd_upsample {
        match effective_dsd_mode.to_lowercase().as_str() {
            "dsd128" => Some(5644800),
            "dsd256" => Some(11289600),
            _ => Some(2822400),
        }
    } else if is_native_dsf {
        let track_rate = track.get("sampleRate").and_then(|v| v.as_u64()).unwrap_or(2822400) as u32;
        Some(if track_rate > 0 { track_rate } else { 2822400 })
    } else {
        None
    };

    let stream_filename = if is_dsd { "audio.dsf" } else { "audio.wav" };
    let stream_url = format!("http://{}:39840/stream/{}/{}", local_ip, stream_token, stream_filename);

    // Cancel any prior progressive stream sessions
    roo_streamer::cancel_all_progressive_streams();

    if is_native_dsf && !is_dsd_upsample && peq_json.is_empty() && conv_json.is_empty() {
        if let Err(e) = roo_streamer::register_direct_file_stream(
            &stream_token,
            std::path::Path::new(file_path),
            "audio/x-dsf",
        ) {
            tracing::error!("Failed to register direct DSF stream: {}", e);
            return Err(e.to_string());
        }
    } else {
        let opts = roo_streamer::DspStreamOptions {
            file_path: file_path.to_string(),
            start_sec: start,
            target_rate,
            dsd_mode: if effective_dsd_mode.is_empty() { None } else { Some(effective_dsd_mode.clone()) },
            dsd_order,
            filter_phase: filter_phase.clone(),
            peq_json: if peq_json.is_empty() { None } else { Some(peq_json.clone()) },
            conv_json: if conv_json.is_empty() { None } else { Some(conv_json.clone()) },
            ultrasonic_filter: Some(ultrasonic_filter.clone()),
            dop: Some(false), // Native DSD (DSF), NOT DoP!
            dither,
        };

        let token_clone = stream_token.clone();
        let opts_clone = opts.clone();
        let start_res = tokio::task::spawn_blocking(move || {
            roo_streamer::start_progressive_stream(&opts_clone, &token_clone)
        }).await.map_err(|e| e.to_string())?;

        if let Err(e) = start_res {
            tracing::error!("Failed to start progressive stream: {}", e);
            return Err(e.to_string());
        }
    }

    let app_clone = app.clone();
    let app_clone2 = app.clone();
    let res = manager.play(
        &device_id,
        &stream_url,
        title,
        artist,
        album,
        Some(&cover_url),
        dsd_hz,
        duration_sec,
        start,
        move |pos, dur, state| {
            let _ = app_clone.emit("dlna:progress", serde_json::json!({
                "currentSec": pos,
                "positionSec": pos,
                "durationSec": dur,
                "maxSec": pos,
                "relTime": format!("{:.1}", pos),
                "trackDuration": format!("{:.1}", dur),
                "state": state
            }));
        },
        move || {
            let _ = app_clone2.emit("dlna:trackEnded", ());
        },
    ).await;

    match res {
        Ok(_) => Ok(serde_json::json!({
            "success": true,
            "ok": true,
            "token": stream_token
        })),
        Err(e) => {
            tracing::error!("DLNA play failed: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "ok": false,
                "error": e.to_string(),
                "msg": e.to_string()
            }))
        }
    }
}

#[tauri::command]
pub async fn dlna_pause(manager: State<'_, DlnaManager>, device_id: String) -> Result<bool, String> {
    manager.pause(&device_id).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_resume(manager: State<'_, DlnaManager>, device_id: String) -> Result<bool, String> {
    manager.resume(&device_id).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_stop(manager: State<'_, DlnaManager>, device_id: String) -> Result<bool, String> {
    roo_streamer::cancel_all_progressive_streams();
    manager.stop(&device_id).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_seek(manager: State<'_, DlnaManager>, device_id: String, seconds: f64) -> Result<bool, String> {
    manager.seek(&device_id, seconds).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_set_volume(manager: State<'_, DlnaManager>, device_id: String, volume: u32) -> Result<bool, String> {
    manager.set_volume(&device_id, volume).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_fake_renderer_actions() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true }))
}
