use tauri::{Emitter, Window};

#[tauri::command]
pub async fn window_minimize(window: Window) -> Result<(), String> {
    tracing::info!("Minimizing window");
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn window_maximize_toggle(window: Window) -> Result<bool, String> {
    let is_max = window.is_maximized().map_err(|e| e.to_string())?;
    let new_state = !is_max;
    tracing::info!("Toggle maximize window: currently is_max={}, toggling to {}", is_max, new_state);
    if is_max {
        window.unmaximize().map_err(|e| e.to_string())?;
    } else {
        window.maximize().map_err(|e| e.to_string())?;
    }
    let _ = window.emit("window:maximizedChange", new_state);
    Ok(new_state)
}

#[tauri::command]
pub async fn window_is_maximized(window: Window) -> Result<bool, String> {
    window.is_maximized().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn window_start_dragging(window: Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn window_show(window: Window) -> Result<(), String> {
    tracing::info!("Showing main window");
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub async fn window_close(window: Window) -> Result<(), String> {
    tracing::info!("Closing window");
    window.close().map_err(|e| e.to_string())
}
