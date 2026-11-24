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

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Helper to spawn a hidden command on Windows without flashing a console window.
fn hidden_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Detect active local IPv4 address(es) for LAN scanning.
fn detect_local_ipv4s() -> Vec<std::net::Ipv4Addr> {
    let mut ips = Vec::new();
    // Primary outbound LAN interface via UDP route probe (no packet sent)
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(std::net::SocketAddr::V4(v4)) = sock.local_addr() {
                let ip = *v4.ip();
                if !ip.is_loopback() && !ip.is_link_local() && !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
    }
    ips
}

/// RFC 1002 NetBIOS Node Status (NBSTAT) UDP port 137 query to resolve NAS / Windows hostname in ~5 ms.
async fn query_netbios_hostname(ip: std::net::Ipv4Addr) -> Option<String> {
    let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await.ok()?;
    let mut pkt = [0u8; 50];
    pkt[0] = 0x13; pkt[1] = 0x37; // Transaction ID
    pkt[5] = 0x01; // QDCOUNT = 1
    pkt[12] = 0x20; // Name length = 32
    pkt[13] = 0x43; pkt[14] = 0x4b; // '*'
    for i in (15..45).step_by(2) {
        pkt[i] = 0x41;
        pkt[i + 1] = 0x41;
    }
    pkt[45] = 0x00; // Terminator
    pkt[46] = 0x00; pkt[47] = 0x21; // QTYPE = NBSTAT
    pkt[48] = 0x00; pkt[49] = 0x01; // QCLASS = IN

    let target = std::net::SocketAddr::from((ip, 137));
    sock.send_to(&pkt, target).await.ok()?;

    let mut buf = [0u8; 1024];
    let (n, _) = tokio::time::timeout(
        std::time::Duration::from_millis(350),
        sock.recv_from(&mut buf),
    ).await.ok()?.ok()?;

    if n < 57 {
        return None;
    }
    let num_names = buf[56] as usize;
    let mut offset = 57usize;
    for _ in 0..num_names {
        if offset + 18 > n {
            break;
        }
        let suffix = buf[offset + 15];
        let flags = u16::from_be_bytes([buf[offset + 16], buf[offset + 17]]);
        let is_group = (flags & 0x8000) != 0;
        if suffix == 0x00 && !is_group {
            let raw = &buf[offset..offset + 15];
            let name = String::from_utf8_lossy(raw).trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
        offset += 18;
    }
    None
}

/// Enumerate public Disk shares on a Windows/Samba server via `net view \\<ip>` (UTF-8 codepage, hidden).
fn enumerate_smb_shares(ip: &str) -> Vec<serde_json::Value> {
    let mut shares = Vec::new();
    let out = hidden_command("cmd")
        .args(["/C", &format!("chcp 65001 >nul && net view \\\\{}", ip)])
        .output();
    let Ok(output) = out else {
        return shares;
    };
    if !output.status.success() {
        return shares;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut in_table = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("-----") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if trimmed.is_empty()
            || trimmed.starts_with("The command completed")
            || trimmed.starts_with("命令已經成功")
            || trimmed.starts_with("命令成功")
        {
            break;
        }
        // Columns separated by whitespace: <ShareName>  Disk|磁碟|磁盘  [UsedAs]  [Comment...]
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.len() >= 2 {
            // Find the type token ("Disk", "磁碟", "磁盘")
            if let Some(type_pos) = tokens.iter().position(|&t| t.eq_ignore_ascii_case("Disk") || t == "磁碟" || t == "磁盘") {
                if type_pos >= 1 {
                    let name = tokens[..type_pos].join(" ");
                    if !name.is_empty() && !name.ends_with('$') {
                        let comment = if type_pos + 1 < tokens.len() {
                            tokens[type_pos + 1..].join(" ")
                        } else {
                            String::new()
                        };
                        shares.push(serde_json::json!({
                            "name": name,
                            "comment": comment,
                        }));
                    }
                }
            }
        }
    }
    shares
}

#[tauri::command]
pub async fn samba_test_share(unc_path: String) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || test_samba_share(&unc_path).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn samba_connect_and_scan(
    _app: AppHandle,
    _state: State<'_, LibraryState>,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let host = params
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .trim_start_matches(['\\', '/'])
        .to_string();
    let share = params
        .get("share")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .trim_matches(['\\', '/'])
        .to_string();
    let sub_path = params
        .get("subPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .trim_matches(['\\', '/'])
        .replace('/', "\\");
    let username = params
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let password = params
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let (unc_base, unc_full) = if !host.is_empty() && !share.is_empty() {
        let base = format!("\\\\{}\\{}", host, share);
        let full = if sub_path.is_empty() {
            base.clone()
        } else {
            format!("{}\\{}", base, sub_path)
        };
        (base, full)
    } else if let Some(raw_unc) = params.get("uncPath").and_then(|v| v.as_str()) {
        let u = raw_unc.trim().to_string();
        (u.clone(), u)
    } else {
        return Ok(serde_json::json!({
            "success": false,
            "error": "請輸入有效的伺服器 IP/主機名稱與共用名稱 (Share Name)"
        }));
    };

    tokio::task::spawn_blocking(move || {
        // Authenticate with `net use` when credentials are provided
        if !username.is_empty() {
            let user_arg = format!("/user:{}", username);
            let run_net_use = || {
                let mut cmd = hidden_command("net");
                cmd.arg("use").arg(&unc_base);
                if !password.is_empty() {
                    cmd.arg(&password);
                } else {
                    cmd.arg("");
                }
                cmd.arg(&user_arg).arg("/persistent:no");
                cmd.output()
            };

            if let Ok(out) = run_net_use() {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    // Error 1219: existing session conflict -> disconnect share & retry once
                    if stderr.contains("1219") {
                        let _ = hidden_command("net")
                            .args(["use", &unc_base, "/delete", "/y"])
                            .output();
                        if let Ok(retry_out) = run_net_use() {
                            if !retry_out.status.success() {
                                let err_msg = String::from_utf8_lossy(&retry_out.stderr).trim().to_string();
                                return serde_json::json!({
                                    "success": false,
                                    "error": if err_msg.is_empty() { "Windows SMB 認證失敗，請檢查帳號密碼或先中斷既有連線。".to_string() } else { err_msg }
                                });
                            }
                        }
                    } else {
                        let err_msg = stderr.trim().to_string();
                        return serde_json::json!({
                            "success": false,
                            "error": if err_msg.is_empty() { "Windows SMB 認證失敗，請檢查 IP、共用名稱或帳密。".to_string() } else { err_msg }
                        });
                    }
                }
            }
        }

        // Verify directory accessibility
        match std::fs::read_dir(&unc_full) {
            Ok(_) => serde_json::json!({
                "success": true,
                "uncPath": unc_full
            }),
            Err(e) => serde_json::json!({
                "success": false,
                "error": format!("無法存取 {}（{}）", unc_full, e)
            }),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn samba_discover_servers() -> Result<serde_json::Value, String> {
    let local_ips = detect_local_ipv4s();
    if local_ips.is_empty() {
        return Ok(serde_json::json!({
            "success": true,
            "servers": []
        }));
    }

    // Deduplicate /24 prefixes from active local LAN interface(s)
    let mut prefixes: Vec<[u8; 3]> = Vec::new();
    for ip in &local_ips {
        let o = ip.octets();
        let p = [o[0], o[1], o[2]];
        if !prefixes.contains(&p) {
            prefixes.push(p);
        }
    }

    // Concurrently probe TCP port 445 across the /24 subnet(s)
    let mut handles = Vec::new();
    for prefix in prefixes {
        for host_octet in 1u8..=254u8 {
            let ip = std::net::Ipv4Addr::new(prefix[0], prefix[1], prefix[2], host_octet);
            if local_ips.contains(&ip) {
                continue; // skip own PC
            }
            handles.push(tokio::spawn(async move {
                let addr = std::net::SocketAddr::from((ip, 445));
                match tokio::time::timeout(
                    std::time::Duration::from_millis(350),
                    tokio::net::TcpStream::connect(addr),
                )
                .await
                {
                    Ok(Ok(_stream)) => Some(ip),
                    _ => None,
                }
            }));
        }
    }

    let mut responsive_ips = Vec::new();
    for h in handles {
        if let Ok(Some(ip)) = h.await {
            responsive_ips.push(ip);
        }
    }
    responsive_ips.sort();

    // Resolve NetBIOS hostnames & enumerate public shares concurrently
    let mut detail_handles = Vec::new();
    for ip in responsive_ips {
        detail_handles.push(tokio::spawn(async move {
            let ip_str = ip.to_string();
            let host_name = query_netbios_hostname(ip)
                .await
                .unwrap_or_else(|| ip_str.clone());
            let ip_for_shares = ip_str.clone();
            let shares = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                tokio::task::spawn_blocking(move || enumerate_smb_shares(&ip_for_shares)),
            )
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default();

            serde_json::json!({
                "ip": ip_str,
                "hostName": host_name,
                "serverDesc": "SAMBA / NAS 伺服器 (SMB 445)",
                "shares": shares
            })
        }));
    }

    let mut servers = Vec::new();
    for h in detail_handles {
        if let Ok(srv) = h.await {
            servers.push(srv);
        }
    }

    Ok(serde_json::json!({
        "success": true,
        "servers": servers
    }))
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
) -> Result<usize, String> {
    let parsed: Vec<TrackRecord> = tracks
        .iter()
        .map(TrackRecord::from_json_value)
        .collect();
    let count = parsed.len();
    state.db.sync_library_tracks(&parsed).map_err(|e| e.to_string())?;
    Ok(count)
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
    let rating_count = extra
        .get("albumRatings")
        .and_then(|r| r.as_object())
        .map(|m| m.len())
        .unwrap_or(0);
    let listen_later_count = extra
        .get("listenLater")
        .and_then(|l| l.as_array())
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
        "historyCount": history_count,
        "ratingCount": rating_count,
        "listenLaterCount": listen_later_count
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

    let mut existing_folders: std::collections::HashSet<String> = state
        .db
        .get_folders()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let mut existing_tracks: std::collections::HashSet<String> = state
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
                if folder_str.trim().is_empty() {
                    continue;
                }
                if existing_folders.contains(folder_str) {
                    folders_skipped.push(serde_json::json!({
                        "path": folder_str,
                        "reason": "已存在於曲庫中"
                    }));
                } else {
                    if state.db.add_folder(folder_str).is_ok() {
                        existing_folders.insert(folder_str.to_string());
                        folders_added.push(folder_str.to_string());
                    }
                }
            }
        }
    }

    let extra = json
        .get("extra")
        .cloned()
        .unwrap_or_else(|| json.clone());

    // Also persist imported networkFolders into SQLite folders table so they survive restart
    if let Some(net_folders) = extra.get("networkFolders").and_then(|v| v.as_array()) {
        for nf in net_folders {
            if let Some(folder_str) = nf.as_str() {
                if !folder_str.trim().is_empty() && !existing_folders.contains(folder_str) {
                    if state.db.add_folder(folder_str).is_ok() {
                        existing_folders.insert(folder_str.to_string());
                        folders_added.push(folder_str.to_string());
                    }
                }
            }
        }
    }

    let mut tracks_to_insert: Vec<TrackRecord> = Vec::new();
    let mut tracks_skipped = Vec::new();

    let tracks_array = json
        .get("tracks")
        .or_else(|| json.get("data").and_then(|d| d.get("tracks")))
        .and_then(|v| v.as_array());

    if let Some(tracks) = tracks_array {
        tracks_to_insert.reserve(tracks.len());
        for t_val in tracks {
            let mut track = TrackRecord::from_json_value(t_val);
            if track.path.trim().is_empty() {
                tracks_skipped.push(serde_json::json!({
                    "title": track.title,
                    "artist": track.artist,
                    "path": "",
                    "reason": "曲目缺少有效路徑，已略過"
                }));
                continue;
            }
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
                existing_tracks.insert(track.path.clone());
                tracks_to_insert.push(track);
            }
        }
    }

    let tracks_added_count = state
        .db
        .bulk_upsert_tracks(&tracks_to_insert)
        .map_err(|e| format!("寫入曲庫資料失敗: {}", e))?;

    let imported_favorites = extra.get("favorites").cloned();
    let imported_favorite_snapshots = extra.get("favoriteSnapshots").cloned();
    let imported_playlists = extra.get("playlists").cloned();
    let imported_network_shares = extra.get("networkShares").cloned();
    let imported_network_folders = extra.get("networkFolders").cloned();
    let imported_play_stats = extra.get("playStats").cloned();
    let imported_playback_history = extra.get("playbackHistory").cloned();
    let imported_recently_played_albums = extra.get("recentlyPlayedAlbums").cloned();
    let imported_album_ratings = extra.get("albumRatings").cloned();
    let imported_listen_later = extra.get("listenLater").cloned();

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
        "importedAlbumRatings": imported_album_ratings,
        "importedListenLater": imported_listen_later,
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
