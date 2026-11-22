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

    let dev = manager.get_devices().into_iter().find(|d| d.id == device_id);
    let target_ip_or_host = dev.as_ref()
        .map(|d| if !d.ip.is_empty() { d.ip.clone() } else { d.location.clone() })
        .unwrap_or_default();

    let local_ip = resolve_local_ip_for_target(&target_ip_or_host);

    let cover_url = if !raw_cover.is_empty() {
        if raw_cover.starts_with("http://") || raw_cover.starts_with("https://") {
            Some(format!("http://{}:39840/cover?url={}&size=360", local_ip, urlencoding::encode(raw_cover)))
        } else {
            let clean = raw_cover
                .trim_start_matches("media://")
                .trim_start_matches("file:///")
                .trim_start_matches("file://");
            Some(format!("http://{}:39840/cover?path={}&size=360", local_ip, urlencoding::encode(clean)))
        }
    } else if !file_path.is_empty() {
        Some(format!("http://{}:39840/cover?file_path={}&size=360", local_ip, urlencoding::encode(file_path)))
    } else {
        None
    };

    let (target_rate, dsd_mode, dsd_order, filter_phase, peq_json, conv_json, ultrasonic_filter) =
        if let Some(opts) = &dsp_options {
            let rate = opts.get("sampleRate").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let dsd_m = opts.get("dsdMode").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let dsd_o = opts.get("dsdOrder").and_then(|v| v.as_u64()).unwrap_or(5) as u8;
            let phase = opts.get("filterPhase").and_then(|v| v.as_str()).unwrap_or("linear").to_string();
            let peq = opts.get("peq").map(|v| v.to_string()).unwrap_or_default();
            let conv = opts.get("conv").map(|v| v.to_string()).unwrap_or_default();
            let ultra = opts.get("ultrasonicFilter").and_then(|v| v.as_str()).unwrap_or("30k").to_string();
            (rate, dsd_m, dsd_o, phase, peq, conv, ultra)
        } else {
            (0, String::new(), 5, "linear".to_string(), String::new(), String::new(), "30k".to_string())
        };

    let stream_url = format!(
        "http://{}:39840/sox-stream?file_path={}&start_sec={}&target_rate={}&dsd_mode={}&dsd_order={}&filter_phase={}&peq_json={}&conv_json={}&ultrasonic_filter={}&dop=true",
        local_ip,
        urlencoding::encode(file_path),
        start,
        target_rate,
        urlencoding::encode(&dsd_mode),
        dsd_order,
        urlencoding::encode(&filter_phase),
        urlencoding::encode(&peq_json),
        urlencoding::encode(&conv_json),
        urlencoding::encode(&ultrasonic_filter)
    );

    // If DSD upsampling or high-rate DSP is requested, prewarm stream into memory cache
    // so the DLNA renderer receives an immediate response upon connecting, avoiding socket timeout.
    if !dsd_mode.is_empty() || target_rate > 0 {
        let opts = roo_streamer::DspStreamOptions {
            file_path: file_path.to_string(),
            start_sec: start,
            target_rate,
            dsd_mode: if dsd_mode.is_empty() { None } else { Some(dsd_mode.clone()) },
            dsd_order,
            filter_phase: filter_phase.clone(),
            peq_json: if peq_json.is_empty() { None } else { Some(peq_json.clone()) },
            conv_json: if conv_json.is_empty() { None } else { Some(conv_json.clone()) },
            ultrasonic_filter: Some(ultrasonic_filter.clone()),
            dop: Some(true),
        };
        let _ = tokio::task::spawn_blocking(move || {
            roo_streamer::prewarm_stream(&opts)
        }).await;
    }

    let app_clone = app.clone();
    let app_clone2 = app.clone();
    let res = manager.play(
        &device_id,
        &stream_url,
        title,
        artist,
        album,
        cover_url.as_deref(),
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
            "token": format!("{}:{}:{}", file_path, start, target_rate)
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
