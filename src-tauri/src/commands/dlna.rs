use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use roo_dlna::{discover_dlna_devices, probe_device_ip, probe_known_devices, DlnaDevice, DlnaManager};
use roo_airplay::{discover_airplay_devices, AirPlayDevice, AirPlayManager};

fn airplay_to_unified_json(dev: &AirPlayDevice) -> serde_json::Value {
    serde_json::json!({
        "id": dev.id,
        "name": dev.name,
        "model": dev.model,
        "ip": dev.ip,
        "port": dev.port,
        "protocol": "airplay",
        "deviceCategory": "airplay",
        "isLocal": false,
        "supportsUnencrypted": dev.supports_unencrypted,
        "requiresPassword": dev.requires_password,
    })
}

pub fn merged_devices_json(dlna: &DlnaManager, airplay: &AirPlayManager) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = dlna
        .get_devices()
        .into_iter()
        .filter_map(|d| {
            let mut val = serde_json::to_value(d).ok()?;
            if let Some(obj) = val.as_object_mut() {
                obj.insert("protocol".to_string(), serde_json::json!("dlna"));
            }
            Some(val)
        })
        .collect();

    for ap in airplay.get_devices() {
        out.push(airplay_to_unified_json(&ap));
    }
    out
}

pub fn emit_merged_devices(app: &AppHandle, dlna: &DlnaManager, airplay: &AirPlayManager) {
    let merged = merged_devices_json(dlna, airplay);
    let _ = app.emit("dlna:devicesChanged", &merged);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnownDlnaDevice {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub location: String,
    pub last_used: u64,
}

fn known_devices_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rooPlayer")
        .join("dlna_known_devices.json")
}

pub fn load_known_dlna_devices() -> Vec<KnownDlnaDevice> {
    let path = known_devices_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn remember_dlna_device(dev: &DlnaDevice) {
    if dev.location.is_empty() && dev.ip.is_empty() {
        return;
    }
    let path = known_devices_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut list = load_known_dlna_devices();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    list.retain(|k| k.id != dev.id && !(k.ip == dev.ip && !dev.ip.is_empty() && k.name == dev.name));
    list.insert(
        0,
        KnownDlnaDevice {
            id: dev.id.clone(),
            name: dev.name.clone(),
            ip: dev.ip.clone(),
            location: dev.location.clone(),
            last_used: now,
        },
    );
    list.truncate(16);

    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(&path, json);
    }
}

/// Fast warm-probe of remembered DLNA devices (used on startup & start of scan).
pub async fn warm_probe_known_dlna(app: &AppHandle, manager: &DlnaManager) -> Vec<DlnaDevice> {
    let known = load_known_dlna_devices();
    if known.is_empty() {
        return Vec::new();
    }
    let pairs: Vec<(String, String)> = known.into_iter().map(|k| (k.location, k.ip)).collect();
    let found = probe_known_devices(&pairs).await;
    if !found.is_empty() {
        let mut merged = manager.get_devices();
        for dev in &found {
            if let Some(existing) = merged.iter_mut().find(|d| d.id == dev.id) {
                *existing = dev.clone();
            } else {
                merged.push(dev.clone());
            }
        }
        manager.set_devices(merged.clone());
        let _ = app.emit("dlna:devicesChanged", &merged);
    }
    found
}

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
pub async fn dlna_get_devices(
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
) -> Result<Vec<serde_json::Value>, String> {
    Ok(merged_devices_json(&manager, &airplay))
}

#[tauri::command]
pub async fn dlna_scan(
    app: AppHandle,
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
) -> Result<Vec<serde_json::Value>, String> {
    // Phase 1 (< 100ms): Instantly probe remembered DLNA devices by location/IP
    let fast_found = warm_probe_known_dlna(&app, &manager).await;

    // Phase 2: Concurrent SSDP (DLNA) + mDNS (AirPlay) discovery
    let (ssdp_res, ap_res) = tokio::join!(
        discover_dlna_devices(2),
        tokio::task::spawn_blocking(|| discover_airplay_devices(1800))
    );
    let ssdp_found = ssdp_res.unwrap_or_default();
    let ap_found = ap_res.unwrap_or_default();

    let mut merged: std::collections::HashMap<String, DlnaDevice> = std::collections::HashMap::new();
    for dev in fast_found {
        merged.insert(dev.id.clone(), dev);
    }
    for dev in ssdp_found {
        merged.insert(dev.id.clone(), dev);
    }

    // Refresh remembered device location/IP if a previously known device changed address
    let known = load_known_dlna_devices();
    for dev in merged.values() {
        if known.iter().any(|k| k.id == dev.id && (k.ip != dev.ip || k.location != dev.location)) {
            remember_dlna_device(dev);
        }
    }

    let dlna_devices: Vec<DlnaDevice> = merged.into_values().collect();
    manager.set_devices(dlna_devices);
    airplay.set_devices(ap_found);

    let all_devices = merged_devices_json(&manager, &airplay);
    let _ = app.emit("dlna:devicesChanged", &all_devices);
    Ok(all_devices)
}

#[tauri::command]
pub async fn dlna_probe_ip(manager: State<'_, DlnaManager>, ip: String) -> Result<DlnaDevice, String> {
    let dev = probe_device_ip(&ip).await.map_err(|e| e.to_string())?;
    remember_dlna_device(&dev);
    let mut list = manager.get_devices();
    if let Some(existing) = list.iter_mut().find(|d| d.id == dev.id) {
        *existing = dev.clone();
    } else {
        list.push(dev.clone());
    }
    manager.set_devices(list);
    Ok(dev)
}

#[tauri::command]
pub async fn dlna_select_device(
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
    device_id: String,
) -> Result<bool, String> {
    if device_id.starts_with("airplay:") {
        let _ = manager.select_device("local");
        return Ok(airplay.get_device(&device_id).is_some());
    }
    let ok = manager.select_device(&device_id);
    if ok {
        if let Some(dev) = manager.get_devices().into_iter().find(|d| d.id == device_id) {
            remember_dlna_device(&dev);
        }
    }
    Ok(ok)
}

static TOKEN_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[tauri::command]
pub async fn dlna_play_track(
    app: AppHandle,
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
    device_id: String,
    track: serde_json::Value,
    start_sec: Option<f64>,
    dsp_options: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let title = track.get("title").and_then(|v| v.as_str()).unwrap_or("Track");
    let artist = track.get("artist").and_then(|v| v.as_str()).unwrap_or("Artist");
    let album = track.get("album").and_then(|v| v.as_str()).unwrap_or("Album");

    // Route AirPlay devices directly to native RAOP engine (44.1kHz / 16-bit ALAC lossless)
    if device_id.starts_with("airplay:") {
        let file_path = track.get("filePath").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if file_path.is_empty() {
            return Err("Track has no filePath".to_string());
        }
        let start = start_sec.unwrap_or(0.0);
        let peq_json = dsp_options
            .as_ref()
            .and_then(|o| o.get("peq"))
            .map(|v| v.to_string())
            .filter(|s| !s.is_empty() && s != "null");
        let conv_json = dsp_options
            .as_ref()
            .and_then(|o| o.get("conv"))
            .map(|v| v.to_string())
            .filter(|s| !s.is_empty() && s != "null");

        let app_prog = app.clone();
        let app_end = app.clone();
        let dev_prog = device_id.clone();
        let dev_end = device_id.clone();
        let on_progress: Arc<dyn Fn(f64, f64, &str) + Send + Sync> = Arc::new(move |pos, dur, state| {
            let _ = app_prog.emit("dlna:progress", serde_json::json!({
                "deviceId": dev_prog,
                "currentSec": pos,
                "positionSec": pos,
                "durationSec": dur,
                "maxSec": pos,
                "relTime": format!("{:.1}", pos),
                "trackDuration": format!("{:.1}", dur),
                "state": state
            }));
        });
        let on_ended: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = app_end.emit("dlna:trackEnded", serde_json::json!({
                "deviceId": dev_end
            }));
        });

        let ap_clone = airplay.inner().clone();
        let dev_id_clone = device_id.clone();
        let title_owned = title.to_string();
        let artist_owned = artist.to_string();
        let album_owned = album.to_string();
        let res = tokio::task::spawn_blocking(move || {
            ap_clone.play(
                &dev_id_clone,
                &file_path,
                start,
                &title_owned,
                &artist_owned,
                &album_owned,
                peq_json.as_deref(),
                conv_json.as_deref(),
                on_progress,
                on_ended,
            )
        })
        .await
        .map_err(|e| e.to_string())?;
        return match res {
            Ok(_) => Ok(serde_json::json!({ "success": true, "ok": true, "protocol": "airplay" })),
            Err(e) => {
                tracing::error!("AirPlay play failed on {}: {}", device_id, e);
                Ok(serde_json::json!({
                    "success": false,
                    "ok": false,
                    "error": e.to_string(),
                    "msg": e.to_string()
                }))
            }
        };
    }
    let file_path = track.get("filePath").or_else(|| track.get("path")).and_then(|v| v.as_str()).unwrap_or("");
    let raw_cover = track.get("cover").or_else(|| track.get("coverPath")).and_then(|v| v.as_str()).unwrap_or("");
    let start = start_sec.unwrap_or(0.0);
    let duration_sec = track.get("duration").and_then(|v| v.as_f64());

    let dev = manager.get_devices().into_iter().find(|d| d.id == device_id);
    if let Some(ref d) = dev {
        remember_dlna_device(d);
    }
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
    let track_sample_rate = track.get("sampleRate").and_then(|v| v.as_u64()).unwrap_or(44100) as u32;
    let dsd_hz: Option<u32> = if is_dsd_upsample {
        match dsd_mode.to_lowercase().as_str() {
            "dsd128" => Some(5644800),
            "dsd256" => Some(11289600),
            "dsd512" => Some(22579200),
            _ => Some(2822400),
        }
    } else if is_native_dsf {
        Some(if track_sample_rate > 0 { track_sample_rate } else { 2822400 })
    } else {
        None
    };
    let pcm_hz: Option<u32> = if !is_dsd {
        let hz = if target_rate > 0 { target_rate } else { track_sample_rate };
        if hz > 0 { Some(hz) } else { None }
    } else {
        None
    };

    let stream_filename = if is_dsd { "audio.dsf" } else { "audio.wav" };
    let stream_url = format!("http://{}:39840/stream/{}/{}", local_ip, stream_token, stream_filename);

    // Cancel ONLY this device's prior progressive stream & stale gapless stream (other zones keep streaming!)
    if let Some(old_tok) = manager.replace_device_token(&device_id, stream_token.clone()) {
        roo_streamer::cancel_progressive_stream(&old_tok);
    }
    if let Some(old_next) = manager.take_device_next_token(&device_id) {
        roo_streamer::cancel_progressive_stream(&old_next);
    }

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
            dsd_mode: if dsd_mode.is_empty() { None } else { Some(dsd_mode.clone()) },
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
    let app_clone3 = app.clone();
    let dev_id_prog = device_id.clone();
    let dev_id_end = device_id.clone();
    let dev_id_gapless = device_id.clone();
    let res = manager.play(
        &device_id,
        &stream_url,
        title,
        artist,
        album,
        Some(&cover_url),
        dsd_hz,
        pcm_hz,
        duration_sec,
        start,
        move |pos, dur, state| {
            let _ = app_clone.emit("dlna:progress", serde_json::json!({
                "deviceId": dev_id_prog,
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
            let _ = app_clone2.emit("dlna:trackEnded", serde_json::json!({
                "deviceId": dev_id_end
            }));
        },
        move || {
            let _ = app_clone3.emit("dlna:gaplessAdvanced", serde_json::json!({
                "deviceId": dev_id_gapless
            }));
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
pub async fn dlna_queue_next_track(
    manager: State<'_, DlnaManager>,
    device_id: String,
    track: serde_json::Value,
    dsp_options: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    if device_id.starts_with("airplay:") {
        return Ok(serde_json::json!({ "ok": false, "unsupported": true }));
    }
    let file_path = track.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
    if file_path.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "reason": "no filePath" }));
    }

    let title = track.get("title").and_then(|v| v.as_str()).unwrap_or("Unknown Track");
    let artist = track.get("artist").and_then(|v| v.as_str()).unwrap_or("Unknown Artist");
    let album = track.get("album").and_then(|v| v.as_str()).unwrap_or("");
    let raw_cover = track.get("cover").and_then(|v| v.as_str()).unwrap_or("");
    let duration_sec = track.get("duration").and_then(|v| v.as_f64()).filter(|&d| d > 0.0);

    let dev = manager.get_devices().into_iter().find(|d| d.id == device_id);
    let target_ip_or_host = dev.as_ref()
        .map(|d| if !d.ip.is_empty() { d.ip.clone() } else { d.location.clone() })
        .unwrap_or_default();
    let local_ip = resolve_local_ip_for_target(&target_ip_or_host);

    let stream_token = format!(
        "nxt{:x}{:04x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        TOKEN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );

    let cover_url = format!("http://{}:39840/cover/{}/cover.jpg", local_ip, stream_token);
    roo_streamer::register_cover_for_token(
        &stream_token,
        if raw_cover.is_empty() { None } else { Some(raw_cover.to_string()) },
        Some(file_path.to_string()),
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
    let track_sample_rate = track.get("sampleRate").and_then(|v| v.as_u64()).unwrap_or(44100) as u32;
    let dsd_hz: Option<u32> = if is_dsd_upsample {
        match dsd_mode.to_lowercase().as_str() {
            "dsd128" => Some(5644800),
            "dsd256" => Some(11289600),
            "dsd512" => Some(22579200),
            _ => Some(2822400),
        }
    } else if is_native_dsf {
        Some(if track_sample_rate > 0 { track_sample_rate } else { 2822400 })
    } else {
        None
    };
    let pcm_hz: Option<u32> = if !is_dsd {
        let hz = if target_rate > 0 { target_rate } else { track_sample_rate };
        if hz > 0 { Some(hz) } else { None }
    } else {
        None
    };

    let stream_filename = if is_dsd { "audio.dsf" } else { "audio.wav" };
    let stream_url = format!("http://{}:39840/stream/{}/{}", local_ip, stream_token, stream_filename);

    if let Some(old_next) = manager.replace_device_next_token(&device_id, stream_token.clone()) {
        roo_streamer::cancel_progressive_stream(&old_next);
    }

    if is_native_dsf && !is_dsd_upsample && peq_json.is_empty() && conv_json.is_empty() {
        roo_streamer::register_direct_file_stream(
            &stream_token,
            std::path::Path::new(file_path),
            "audio/x-dsf",
        ).map_err(|e| e.to_string())?;
    } else {
        let opts = roo_streamer::DspStreamOptions {
            file_path: file_path.to_string(),
            start_sec: 0.0,
            target_rate,
            dsd_mode: if dsd_mode.is_empty() { None } else { Some(dsd_mode.clone()) },
            dsd_order,
            filter_phase: filter_phase.clone(),
            peq_json: if peq_json.is_empty() { None } else { Some(peq_json.clone()) },
            conv_json: if conv_json.is_empty() { None } else { Some(conv_json.clone()) },
            ultrasonic_filter: Some(ultrasonic_filter.clone()),
            dop: Some(false),
            dither,
        };
        let token_clone = stream_token.clone();
        tokio::task::spawn_blocking(move || {
            roo_streamer::start_progressive_stream(&opts, &token_clone)
        }).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    }

    match manager.queue_next(
        &device_id,
        &stream_url,
        title,
        artist,
        album,
        Some(&cover_url),
        dsd_hz,
        pcm_hz,
        duration_sec,
    ).await {
        Ok(_) => Ok(serde_json::json!({ "ok": true, "streamUrl": stream_url })),
        Err(e) => {
            if let Some(tok) = manager.take_device_next_token(&device_id) {
                roo_streamer::cancel_progressive_stream(&tok);
            }
            tracing::debug!("[DLNA Gapless] SetNextAVTransportURI unsupported on {}: {}", device_id, e);
            Ok(serde_json::json!({ "ok": false, "unsupported": true }))
        }
    }
}

#[tauri::command]
pub async fn dlna_pause(
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
    device_id: String,
) -> Result<bool, String> {
    if device_id.starts_with("airplay:") {
        return airplay.pause(&device_id).map(|_| true).map_err(|e| e.to_string());
    }
    manager.pause(&device_id).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_resume(
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
    device_id: String,
) -> Result<bool, String> {
    if device_id.starts_with("airplay:") {
        return airplay.resume(&device_id).map(|_| true).map_err(|e| e.to_string());
    }
    manager.resume(&device_id).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_stop(
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
    device_id: String,
) -> Result<bool, String> {
    if device_id.starts_with("airplay:") {
        airplay.stop(&device_id);
        return Ok(true);
    }
    if let Some(old_tok) = manager.take_device_token(&device_id) {
        roo_streamer::cancel_progressive_stream(&old_tok);
    }
    if let Some(old_next) = manager.take_device_next_token(&device_id) {
        roo_streamer::cancel_progressive_stream(&old_next);
    }
    manager.stop(&device_id).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_seek(
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
    device_id: String,
    seconds: f64,
) -> Result<bool, String> {
    if device_id.starts_with("airplay:") {
        return airplay.seek(&device_id, seconds).map(|_| true).map_err(|e| e.to_string());
    }
    manager.seek(&device_id, seconds).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_set_volume(
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
    device_id: String,
    volume: u32,
) -> Result<bool, String> {
    if device_id.starts_with("airplay:") {
        return airplay.set_volume(&device_id, volume).map(|_| true).map_err(|e| e.to_string());
    }
    manager.set_volume(&device_id, volume).await.map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn dlna_fake_renderer_actions() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true }))
}
