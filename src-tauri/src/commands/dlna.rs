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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnownAirPlayDevice {
    pub id: String,
    pub name: String,
    pub model: String,
    pub ip: String,
    pub port: u16,
    #[serde(default = "default_true")]
    pub supports_unencrypted: bool,
    #[serde(default)]
    pub requires_password: bool,
    pub last_seen: u64,
}

fn default_true() -> bool {
    true
}

fn known_devices_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rooPlayer")
        .join("dlna_known_devices.json")
}

fn known_airplay_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rooPlayer")
        .join("airplay_known_devices.json")
}

pub fn load_known_dlna_devices() -> Vec<KnownDlnaDevice> {
    let path = known_devices_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn load_known_airplay_devices() -> Vec<KnownAirPlayDevice> {
    let path = known_airplay_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn remember_dlna_devices(devs: &[DlnaDevice]) {
    if devs.is_empty() {
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

    for dev in devs {
        if dev.location.is_empty() && dev.ip.is_empty() {
            continue;
        }
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
    }
    list.truncate(24);

    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn remember_dlna_device(dev: &DlnaDevice) {
    remember_dlna_devices(std::slice::from_ref(dev));
}

pub fn remember_airplay_devices(devs: &[AirPlayDevice]) {
    if devs.is_empty() {
        return;
    }
    let path = known_airplay_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut list = load_known_airplay_devices();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    for dev in devs {
        if dev.ip.is_empty() || dev.port == 0 {
            continue;
        }
        list.retain(|k| k.id != dev.id && !(k.ip == dev.ip && k.port == dev.port));
        list.insert(
            0,
            KnownAirPlayDevice {
                id: dev.id.clone(),
                name: dev.name.clone(),
                model: dev.model.clone(),
                ip: dev.ip.clone(),
                port: dev.port,
                supports_unencrypted: dev.supports_unencrypted,
                requires_password: dev.requires_password,
                last_seen: now,
            },
        );
    }
    list.truncate(24);

    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(&path, json);
    }
}

/// Fast warm-probe of remembered DLNA devices (< 100ms for online devices).
pub async fn warm_probe_known_dlna(manager: &DlnaManager) -> Vec<DlnaDevice> {
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
        manager.set_devices(merged);
    }
    found
}

/// Fast parallel TCP reachability warm-probe of remembered AirPlay devices (< 50ms on LAN).
pub async fn warm_probe_known_airplay(airplay: &AirPlayManager) -> Vec<AirPlayDevice> {
    let known = load_known_airplay_devices();
    if known.is_empty() {
        return Vec::new();
    }
    let mut handles = Vec::with_capacity(known.len());
    for k in known {
        handles.push(tokio::spawn(async move {
            let addr = format!("{}:{}", k.ip, k.port);
            let ok = tokio::time::timeout(
                std::time::Duration::from_millis(300),
                tokio::net::TcpStream::connect(&addr),
            )
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false);
            if ok {
                Some(AirPlayDevice {
                    id: k.id,
                    name: k.name,
                    model: k.model,
                    ip: k.ip,
                    port: k.port,
                    protocol: "airplay".to_string(),
                    supports_unencrypted: k.supports_unencrypted,
                    requires_password: k.requires_password,
                })
            } else {
                None
            }
        }));
    }

    let mut found = Vec::new();
    for h in handles {
        if let Ok(Some(dev)) = h.await {
            found.push(dev);
        }
    }
    if !found.is_empty() {
        let mut merged = airplay.get_devices();
        for dev in &found {
            if let Some(existing) = merged.iter_mut().find(|d| d.id == dev.id) {
                *existing = dev.clone();
            } else {
                merged.push(dev.clone());
            }
        }
        airplay.set_devices(merged);
    }
    found
}

/// Concurrent Phase-1 warm-probe of all remembered DLNA & AirPlay devices, emitting immediately.
pub async fn warm_probe_all_known(
    app: &AppHandle,
    manager: &DlnaManager,
    airplay: &AirPlayManager,
) -> (Vec<DlnaDevice>, Vec<AirPlayDevice>) {
    let (dlna_found, ap_found) = tokio::join!(
        warm_probe_known_dlna(manager),
        warm_probe_known_airplay(airplay)
    );
    if !dlna_found.is_empty() || !ap_found.is_empty() {
        emit_merged_devices(app, manager, airplay);
    }
    (dlna_found, ap_found)
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
    // Phase 1 (< 100ms): Instantly warm-probe all remembered DLNA & AirPlay addresses in parallel
    let (fast_dlna, fast_ap) = warm_probe_all_known(&app, &manager, &airplay).await;

    // Phase 2: Concurrent SSDP (DLNA) + mDNS (AirPlay) multicast discovery for new/DHCP-changed devices
    let (ssdp_res, ap_res) = tokio::join!(
        discover_dlna_devices(2),
        tokio::task::spawn_blocking(|| discover_airplay_devices(1200))
    );
    let ssdp_found = ssdp_res.unwrap_or_default();
    let ap_found = ap_res.unwrap_or_default();

    let mut merged_dlna: std::collections::HashMap<String, DlnaDevice> = std::collections::HashMap::new();
    for dev in fast_dlna {
        merged_dlna.insert(dev.id.clone(), dev);
    }
    for dev in ssdp_found {
        merged_dlna.insert(dev.id.clone(), dev);
    }

    let mut merged_ap: std::collections::HashMap<String, AirPlayDevice> = std::collections::HashMap::new();
    for dev in fast_ap {
        merged_ap.insert(dev.id.clone(), dev);
    }
    for dev in ap_found {
        merged_ap.insert(dev.id.clone(), dev);
    }

    let dlna_devices: Vec<DlnaDevice> = merged_dlna.into_values().collect();
    let ap_devices: Vec<AirPlayDevice> = merged_ap.into_values().collect();

    // Persist ALL discovered DLNA & AirPlay addresses so future startups & scans resolve in < 100ms
    remember_dlna_devices(&dlna_devices);
    remember_airplay_devices(&ap_devices);

    manager.set_devices(dlna_devices);
    airplay.set_devices(ap_devices);

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
            dither: dither.clone(),
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

    let make_callbacks = |app_h: &AppHandle, dev_id: &str| {
        let app_c1 = app_h.clone();
        let app_c2 = app_h.clone();
        let app_c3 = app_h.clone();
        let id1 = dev_id.to_string();
        let id2 = dev_id.to_string();
        let id3 = dev_id.to_string();
        let on_prog = move |pos: f64, dur: f64, state: &str| {
            let _ = app_c1.emit("dlna:progress", serde_json::json!({
                "deviceId": id1,
                "currentSec": pos,
                "positionSec": pos,
                "durationSec": dur,
                "maxSec": pos,
                "relTime": format!("{:.1}", pos),
                "trackDuration": format!("{:.1}", dur),
                "state": state
            }));
        };
        let on_end = move || {
            let _ = app_c2.emit("dlna:trackEnded", serde_json::json!({
                "deviceId": id2
            }));
        };
        let on_gap = move || {
            let _ = app_c3.emit("dlna:gaplessAdvanced", serde_json::json!({
                "deviceId": id3
            }));
        };
        (on_prog, on_end, on_gap)
    };

    let (on_prog, on_end, on_gap) = make_callbacks(&app, &device_id);
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
        on_prog,
        on_end,
        on_gap,
    ).await;

    match res {
        Ok(_) => Ok(serde_json::json!({
            "success": true,
            "ok": true,
            "token": stream_token
        })),
        Err(orig_err) => {
            // Auto-fallback: if renderer rejected DSD or >48kHz PCM, downgrade cleanly to standard 24-bit WAV
            let mut fallback_rates: Vec<u32> = Vec::new();
            if is_dsd || pcm_hz.unwrap_or(0) > 96000 {
                let safe_hi = if !is_dsd && track_sample_rate % 48000 == 0 { 96000 } else { 88200 };
                fallback_rates.push(safe_hi);
                fallback_rates.push(48000);
            } else if pcm_hz.unwrap_or(0) > 48000 {
                fallback_rates.push(48000);
            }

            if !fallback_rates.is_empty() && !file_path.is_empty() {
                let err_reason = orig_err.to_string();
                tracing::warn!(
                    "DLNA play failed ({}) on {}; attempting automatic PCM downgrade: {:?}",
                    err_reason, device_id, fallback_rates
                );
                for fb_rate in fallback_rates {
                    let fb_token = format!(
                        "{:x}{:04x}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis(),
                        TOKEN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    );
                    let fb_cover_url = format!("http://{}:39840/cover/{}/cover.jpg", local_ip, fb_token);
                    roo_streamer::register_cover_for_token(
                        &fb_token,
                        if raw_cover.is_empty() { None } else { Some(raw_cover.to_string()) },
                        Some(file_path.to_string()),
                    );
                    if let Some(old_tok) = manager.replace_device_token(&device_id, fb_token.clone()) {
                        roo_streamer::cancel_progressive_stream(&old_tok);
                    }

                    let fb_opts = roo_streamer::DspStreamOptions {
                        file_path: file_path.to_string(),
                        start_sec: start,
                        target_rate: fb_rate,
                        dsd_mode: None,
                        dsd_order,
                        filter_phase: filter_phase.clone(),
                        peq_json: if peq_json.is_empty() { None } else { Some(peq_json.clone()) },
                        conv_json: if conv_json.is_empty() { None } else { Some(conv_json.clone()) },
                        ultrasonic_filter: Some(ultrasonic_filter.clone()),
                        dop: Some(false),
                        dither: dither.clone(),
                    };
                    let fb_tok_clone = fb_token.clone();
                    let fb_start = tokio::task::spawn_blocking(move || {
                        roo_streamer::start_progressive_stream(&fb_opts, &fb_tok_clone)
                    }).await.map_err(|e| e.to_string())?;

                    if fb_start.is_err() {
                        continue;
                    }

                    let fb_url = format!("http://{}:39840/stream/{}/audio.wav", local_ip, fb_token);
                    let (fb_prog, fb_end, fb_gap) = make_callbacks(&app, &device_id);
                    if manager.play(
                        &device_id,
                        &fb_url,
                        title,
                        artist,
                        album,
                        Some(&fb_cover_url),
                        None,
                        Some(fb_rate),
                        duration_sec,
                        start,
                        fb_prog,
                        fb_end,
                        fb_gap,
                    ).await.is_ok() {
                        return Ok(serde_json::json!({
                            "success": true,
                            "ok": true,
                            "token": fb_token,
                            "fallback": true,
                            "fallbackRate": fb_rate,
                            "fallbackReason": err_reason
                        }));
                    }
                }
            }

            tracing::error!("DLNA play failed: {}", orig_err);
            Ok(serde_json::json!({
                "success": false,
                "ok": false,
                "error": orig_err.to_string(),
                "msg": orig_err.to_string()
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
pub async fn dlna_get_volume(
    manager: State<'_, DlnaManager>,
    airplay: State<'_, AirPlayManager>,
    device_id: String,
) -> Result<serde_json::Value, String> {
    if device_id.starts_with("airplay:") {
        let (verified_opt, vol_opt) = airplay.get_volume_status(&device_id);
        return Ok(serde_json::json!({
            "protocol": "airplay",
            "supported": verified_opt.unwrap_or(true),
            "verified": verified_opt.is_some(),
            "syncable": false,
            "volume": vol_opt
        }));
    }
    match manager.get_volume(&device_id).await {
        Ok(vol) => Ok(serde_json::json!({
            "protocol": "dlna",
            "supported": true,
            "verified": true,
            "syncable": true,
            "volume": vol
        })),
        Err(e) => Ok(serde_json::json!({
            "protocol": "dlna",
            "supported": false,
            "verified": true,
            "syncable": false,
            "volume": serde_json::Value::Null,
            "reason": e.to_string()
        })),
    }
}

#[tauri::command]
pub async fn dlna_fake_renderer_actions() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true }))
}
