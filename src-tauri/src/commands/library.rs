use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use roo_library::{
    scan_directory, test_samba_share, LibraryDatabase, TrackRecord,
};

pub struct LibraryState {
    pub db: Arc<LibraryDatabase>,
    pub covers_dir: PathBuf,
}

#[tauri::command]
pub async fn dialog_select_folder() -> Result<Option<String>, String> {
    // Native dialog handled via rfd or tauri-plugin-dialog
    let folder = rfd::AsyncFileDialog::new().pick_folder().await;
    Ok(folder.map(|f| f.path().to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn dialog_select_network_folder() -> Result<Option<String>, String> {
    dialog_select_folder().await
}

#[tauri::command]
pub async fn samba_test_share(unc_path: String) -> Result<bool, String> {
    test_samba_share(&unc_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn samba_connect_and_scan(
    app: AppHandle,
    state: State<'_, LibraryState>,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let unc = params.get("uncPath").and_then(|v| v.as_str()).unwrap_or("");
    if unc.is_empty() {
        return Err("Missing UNC path".to_string());
    }
    library_scan_folder(app, state, unc.to_string()).await
}

#[tauri::command]
pub async fn samba_discover_servers() -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

#[tauri::command]
pub async fn library_scan_folder(
    app: AppHandle,
    state: State<'_, LibraryState>,
    folder_path: String,
) -> Result<serde_json::Value, String> {
    let p = Path::new(&folder_path);
    let covers = state.covers_dir.clone();
    let app_clone = app.clone();

    let tracks = scan_directory(p, &covers, move |prog| {
        let _ = app_clone.emit("library:scanProgress", serde_json::json!({
            "current": prog.current,
            "total": prog.total,
            "file": prog.file
        }));
    }).map_err(|e| e.to_string())?;

    for t in &tracks {
        let _ = state.db.insert_or_update_track(t);
    }
    let _ = state.db.add_folder(&folder_path);

    Ok(serde_json::json!({
        "ok": true,
        "tracks": tracks
    }))
}

#[tauri::command]
pub async fn library_load_saved(state: State<'_, LibraryState>) -> Result<serde_json::Value, String> {
    let tracks = state.db.get_all_tracks().map_err(|e| e.to_string())?;
    let folders = state.db.get_folders().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "tracks": tracks,
        "folders": folders
    }))
}

#[tauri::command]
pub async fn library_save_tracks(
    state: State<'_, LibraryState>,
    tracks: Vec<TrackRecord>,
) -> Result<(), String> {
    for t in &tracks {
        let _ = state.db.insert_or_update_track(t);
    }
    Ok(())
}

#[tauri::command]
pub async fn library_remove_folder(
    state: State<'_, LibraryState>,
    folder_path: String,
) -> Result<(), String> {
    state.db.remove_folder(&folder_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn library_update_album_artist(
    _state: State<'_, LibraryState>,
    _album_title: String,
    _artist_name: String,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn library_export(
    _default_filename: Option<String>,
    _extra_data: Option<serde_json::Value>,
) -> Result<Option<String>, String> {
    Ok(None)
}

#[tauri::command]
pub async fn library_import() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn playlist_read_file() -> Result<Option<Vec<String>>, String> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Playlist", &["m3u", "m3u8"])
        .pick_file()
        .await;
    if let Some(f) = file {
        let list = roo_library::read_playlist_file(f.path()).map_err(|e| e.to_string())?;
        Ok(Some(list))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn playlist_write_file(
    suggested_name: Option<String>,
    text: String,
    ext: Option<String>,
) -> Result<bool, String> {
    let extension = ext.unwrap_or_else(|| "m3u".to_string());
    let mut dialog = rfd::AsyncFileDialog::new().add_filter("Playlist", &[&extension]);
    if let Some(name) = suggested_name {
        dialog = dialog.set_file_name(&name);
    }
    if let Some(file) = dialog.save_file().await {
        std::fs::write(file.path(), text).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn shell_open_external(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn util_translate_to_zh_tw(text: String) -> Result<String, String> {
    // OpenCC / simple pass-through if already Traditional Chinese
    Ok(text)
}
