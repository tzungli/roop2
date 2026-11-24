use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Emitter, State, Window};
use roo_dlna::DlnaManager;

static PENDING_MAXIMIZE: AtomicBool = AtomicBool::new(false);

pub fn mark_pending_maximize() {
    PENDING_MAXIMIZE.store(true, Ordering::SeqCst);
}

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
    if PENDING_MAXIMIZE.swap(false, Ordering::SeqCst) {
        let _ = window.maximize();
        let _ = window.emit("window:maximizedChange", true);
    }
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub async fn window_close(window: Window, dlna: State<'_, DlnaManager>) -> Result<(), String> {
    tracing::info!("Closing window — stopping active DLNA renderer & streams");
    // Stop DLNA renderer BEFORE cancelling HTTP streams so it receives Pause/Stop/ClearURI cleanly
    dlna.stop_active().await;
    roo_streamer::cancel_all_progressive_streams();
    window.close().map_err(|e| e.to_string())
}
