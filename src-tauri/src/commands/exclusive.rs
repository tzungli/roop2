use tauri::{AppHandle, Emitter, State};
use roo_output::{
    probe_exclusive_device, ExclusiveController, ExclusiveStartOptions, ExclusiveState, ProbeResult,
};

#[tauri::command]
pub async fn exclusive_probe(force: Option<bool>) -> Result<ProbeResult, String> {
    let _ = force;
    Ok(probe_exclusive_device(None))
}

#[tauri::command]
pub async fn exclusive_start(
    app: AppHandle,
    controller: State<'_, ExclusiveController>,
    options: ExclusiveStartOptions,
) -> Result<serde_json::Value, String> {
    let app_clone = app.clone();
    let res = controller.start(options.clone(), move |event_val| {
        let _ = app_clone.emit("exclusive:status", event_val);
    });

    match res {
        Ok(_) => Ok(serde_json::json!({
            "ok": true,
            "rate": options.rate
        })),
        Err(e) => Ok(serde_json::json!({
            "ok": false,
            "code": "RP-WASAPI-START",
            "msg": e
        })),
    }
}

#[tauri::command]
pub async fn exclusive_pause(controller: State<'_, ExclusiveController>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": controller.pause() }))
}

#[tauri::command]
pub async fn exclusive_resume(controller: State<'_, ExclusiveController>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": controller.resume() }))
}

#[tauri::command]
pub async fn exclusive_seek(
    controller: State<'_, ExclusiveController>,
    stream_seconds: Option<f64>,
) -> Result<serde_json::Value, String> {
    let sec = stream_seconds.unwrap_or(0.0);
    Ok(serde_json::json!({ "ok": controller.seek(sec) }))
}

#[tauri::command]
pub async fn exclusive_stop(controller: State<'_, ExclusiveController>) -> Result<serde_json::Value, String> {
    controller.stop();
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn exclusive_state(controller: State<'_, ExclusiveController>) -> Result<ExclusiveState, String> {
    Ok(controller.state())
}
