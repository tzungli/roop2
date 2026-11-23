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

    let scanned_tracks = scan_directory(p, &covers, move |prog| {
        let _ = app_clone.emit("library:scanProgress", serde_json::json!({
            "current": prog.current,
            "total": prog.total,
            "file": prog.file
        }));
    }).map_err(|e| e.to_string())?;

    for t in &scanned_tracks {
        let _ = state.db.insert_or_update_track(t);
    }
    let _ = state.db.add_folder(&folder_path);

    let all_tracks = state.db.get_all_tracks().map_err(|e| e.to_string())?;
    let all_folders = state.db.get_folders().map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "ok": true,
        "tracks": all_tracks,
        "folders": all_folders,
        "scannedTracks": scanned_tracks,
        "newCount": scanned_tracks.len(),
        "folderPath": folder_path
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
    tracks: Vec<serde_json::Value>,
) -> Result<(), String> {
    let mut parsed: Vec<TrackRecord> = Vec::with_capacity(tracks.len());
    for (idx, val) in tracks.into_iter().enumerate() {
        match serde_json::from_value::<TrackRecord>(val) {
            Ok(rec) => parsed.push(rec),
            Err(e) => {
                tracing::warn!("Skipping malformed track at index {} during library_save_tracks: {}", idx, e);
            }
        }
    }
    state.db.sync_library_tracks(&parsed).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn library_remove_folder(
    state: State<'_, LibraryState>,
    folder_path: String,
) -> Result<serde_json::Value, String> {
    state.db.remove_folder(&folder_path).map_err(|e| e.to_string())?;
    let all_tracks = state.db.get_all_tracks().map_err(|e| e.to_string())?;
    let all_folders = state.db.get_folders().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "ok": true,
        "tracks": all_tracks,
        "folders": all_folders
    }))
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
    state: State<'_, LibraryState>,
    default_filename: Option<String>,
    extra_data: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let default_name = default_filename.unwrap_or_else(|| {
        format!("rooplayer_backup_{}.json", now_secs)
    });

    let dialog = rfd::AsyncFileDialog::new()
        .add_filter("rooPlayer 資料庫備份 (*.json)", &["json"])
        .set_file_name(&default_name);

    let file_handle = dialog.save_file().await;
    let file = match file_handle {
        Some(f) => f,
        None => return Ok(serde_json::json!({ "canceled": true })),
    };

    let tracks = state.db.get_all_tracks().map_err(|e| e.to_string())?;
    let folders = state.db.get_folders().map_err(|e| e.to_string())?;
    let extra = extra_data.unwrap_or_else(|| serde_json::json!({}));

    let fav_albums = extra
        .get("favorites")
        .and_then(|f| f.get("albums"))
        .and_then(|a| a.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let fav_tracks = extra
        .get("favorites")
        .and_then(|f| f.get("tracks"))
        .and_then(|a| a.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let fav_artists = extra
        .get("favorites")
        .and_then(|f| f.get("artists"))
        .and_then(|a| a.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let playlist_count = extra
        .get("playlists")
        .and_then(|p| p.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let net_folder_count = extra
        .get("networkFolders")
        .and_then(|n| n.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let history_count = extra
        .get("playbackHistory")
        .and_then(|h| h.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let export_payload = serde_json::json!({
        "app": "rooPlayer",
        "version": "0.1.0",
        "exportedAt": now_secs,
        "folders": folders,
        "tracks": tracks,
        "extra": extra
    });

    let json_str = serde_json::to_string_pretty(&export_payload).map_err(|e| e.to_string())?;
    std::fs::write(file.path(), json_str).map_err(|e| e.to_string())?;

    let file_path_str = file.path().to_string_lossy().to_string();

    Ok(serde_json::json!({
        "success": true,
        "filePath": file_path_str,
        "folderCount": folders.len(),
        "trackCount": tracks.len(),
        "favoriteCounts": {
            "albums": fav_albums,
            "tracks": fav_tracks,
            "artists": fav_artists
        },
        "playlistCount": playlist_count,
        "networkFolderCount": net_folder_count,
        "historyCount": history_count
    }))
}

#[tauri::command]
pub async fn library_import(
    state: State<'_, LibraryState>,
) -> Result<serde_json::Value, String> {
    let file_handle = rfd::AsyncFileDialog::new()
        .add_filter("rooPlayer 資料庫備份 (*.json)", &["json"])
        .pick_file()
        .await;

    let file = match file_handle {
        Some(f) => f,
        None => return Ok(serde_json::json!({ "canceled": true })),
    };

    let content = std::fs::read_to_string(file.path())
        .map_err(|e| format!("讀取檔案失敗: {}", e))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("JSON 解析失敗: {}", e))?;

    let existing_folders: std::collections::HashSet<String> = state
        .db
        .get_folders()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let existing_tracks: std::collections::HashSet<String> = state
        .db
        .get_all_tracks()
        .unwrap_or_default()
        .into_iter()
        .map(|t| t.path)
        .collect();

    let mut folders_added = Vec::new();
    let mut folders_skipped = Vec::new();

    let folders_array = json
        .get("folders")
        .or_else(|| json.get("data").and_then(|d| d.get("folders")))
        .and_then(|v| v.as_array());

    if let Some(folders) = folders_array {
        for f in folders {
            if let Some(folder_str) = f.as_str() {
                if existing_folders.contains(folder_str) {
                    folders_skipped.push(serde_json::json!({
                        "path": folder_str,
                        "reason": "已存在於曲庫中"
                    }));
                } else {
                    let _ = state.db.add_folder(folder_str);
                    folders_added.push(folder_str.to_string());
                }
            }
        }
    }

    let mut tracks_added_count = 0usize;
    let mut tracks_skipped = Vec::new();

    let tracks_array = json
        .get("tracks")
        .or_else(|| json.get("data").and_then(|d| d.get("tracks")))
        .and_then(|v| v.as_array());

    if let Some(tracks) = tracks_array {
        for t_val in tracks {
            if let Ok(mut track) = serde_json::from_value::<TrackRecord>(t_val.clone()) {
                if track.album_artist.is_empty() {
                    track.album_artist = track.artist.clone();
                }
                if existing_tracks.contains(&track.path) {
                    tracks_skipped.push(serde_json::json!({
                        "title": track.title,
                        "artist": track.artist,
                        "path": track.path,
                        "reason": "曲目路徑已存在，已自動略過重複"
                    }));
                } else {
                    let _ = state.db.insert_or_update_track(&track);
                    tracks_added_count += 1;
                }
            }
        }
    }

    let extra = json
        .get("extra")
        .cloned()
        .unwrap_or_else(|| json.clone());

    let imported_favorites = extra.get("favorites").cloned();
    let imported_favorite_snapshots = extra.get("favoriteSnapshots").cloned();
    let imported_playlists = extra.get("playlists").cloned();
    let imported_network_shares = extra.get("networkShares").cloned();
    let imported_network_folders = extra.get("networkFolders").cloned();
    let imported_play_stats = extra.get("playStats").cloned();
    let imported_playback_history = extra.get("playbackHistory").cloned();
    let imported_recently_played_albums = extra.get("recentlyPlayedAlbums").cloned();

    let all_tracks = state.db.get_all_tracks().map_err(|e| e.to_string())?;
    let all_folders = state.db.get_folders().map_err(|e| e.to_string())?;
    let file_path_str = file.path().to_string_lossy().to_string();

    Ok(serde_json::json!({
        "success": true,
        "filePath": file_path_str,
        "data": {
            "tracks": all_tracks,
            "folders": all_folders
        },
        "importedFavorites": imported_favorites,
        "importedFavoriteSnapshots": imported_favorite_snapshots,
        "importedPlaylists": imported_playlists,
        "importedNetworkShares": imported_network_shares,
        "importedNetworkFolders": imported_network_folders,
        "importedPlayStats": imported_play_stats,
        "importedPlaybackHistory": imported_playback_history,
        "importedRecentlyPlayedAlbums": imported_recently_played_albums,
        "report": {
            "folders": {
                "added": folders_added,
                "skipped": folders_skipped
            },
            "tracks": {
                "addedCount": tracks_added_count,
                "skippedCount": tracks_skipped.len(),
                "skipped": tracks_skipped
            }
        }
    }))
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
