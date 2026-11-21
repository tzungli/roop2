use tauri::{AppHandle, Emitter, State};
use roo_dlna::{discover_dlna_devices, probe_device_ip, DlnaDevice, DlnaManager};

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
    _dsp_options: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let title = track.get("title").and_then(|v| v.as_str()).unwrap_or("Track");
    let artist = track.get("artist").and_then(|v| v.as_str()).unwrap_or("Artist");
    let file_path = track.get("filePath").or_else(|| track.get("path")).and_then(|v| v.as_str()).unwrap_or("");
    let start = start_sec.unwrap_or(0.0);

    let stream_url = format!(
        "http://127.0.0.1:39840/sox-stream?file_path={}&start_sec={}",
        urlencoding::encode(file_path),
        start
    );

    let app_clone = app.clone();
    let res = manager.play(&device_id, &stream_url, title, artist, start, move |pos, dur| {
        let _ = app_clone.emit("dlna:progress", serde_json::json!({
            "positionSec": pos,
            "durationSec": dur,
            "relTime": format!("{:.1}", pos),
            "trackDuration": format!("{:.1}", dur)
        }));
    }).await;

    match res {
        Ok(_) => Ok(serde_json::json!({ "ok": true })),
        Err(e) => Ok(serde_json::json!({ "ok": false, "msg": e.to_string() })),
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
