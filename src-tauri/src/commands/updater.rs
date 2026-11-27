use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

const GITHUB_RELEASES_LATEST_URL: &str = "https://api.github.com/repos/tzungli/roop2/releases/latest";
const ALLOWED_URL_PREFIXES: &[&str] = &[
    "https://github.com/tzungli/roop2/",
    "https://objects.githubusercontent.com/",
    "https://release-assets.githubusercontent.com/",
];

pub fn is_microsoft_store_package() -> bool {
    if std::env::var("ROOPLAYER_STORE_BUILD").ok().as_deref() == Some("1") {
        return true;
    }
    if let Ok(exe) = std::env::current_exe() {
        let lower = exe.to_string_lossy().to_lowercase();
        if lower.contains("\\windowsapps\\") || lower.contains("/windowsapps/") {
            return true;
        }
    }
    false
}

pub fn parse_semver(v: &str) -> (u64, u64, u64) {
    let trimmed = v.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    let core = trimmed.split(['-', '+']).next().unwrap_or("");
    let mut parts = core.split('.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

pub fn is_newer_version(current: &str, latest: &str) -> bool {
    parse_semver(latest) > parse_semver(current)
}

pub fn is_allowed_release_url(url: &str) -> bool {
    ALLOWED_URL_PREFIXES.iter().any(|prefix| url.starts_with(prefix))
}

pub fn parse_sha256_for_asset(checksums_text: &str, asset_name: &str) -> Option<String> {
    let target = asset_name.trim().to_lowercase();
    for line in checksums_text.lines() {
        let clean = line.trim().trim_start_matches('\u{feff}');
        if clean.is_empty() {
            continue;
        }
        let mut parts = clean.split_whitespace();
        let hash = match parts.next() {
            Some(h) if h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()) => h.to_lowercase(),
            _ => continue,
        };
        let fname = match parts.next() {
            Some(f) => f.trim_start_matches('*').to_lowercase(),
            None => continue,
        };
        if fname == target {
            return Some(hash);
        }
    }
    None
}

#[tauri::command]
pub async fn updater_check() -> Result<serde_json::Value, String> {
    let current_ver = env!("CARGO_PKG_VERSION").to_string();
    if is_microsoft_store_package() {
        return Ok(serde_json::json!({
            "currentVersion": current_ver,
            "latestVersion": current_ver,
            "updateAvailable": false,
            "isStoreManaged": true
        }));
    }

    let client = crate::commands::http::shared_http_client();

    let resp = client
        .get(GITHUB_RELEASES_LATEST_URL)
        .timeout(std::time::Duration::from_secs(10))
        .header("User-Agent", format!("rooPlayer/{}", current_ver))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("無法連線至 GitHub 更新伺服器: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API 回傳錯誤狀態: HTTP {}", resp.status().as_u16()));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析更新資訊失敗: {}", e))?;

    let tag_name = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let latest_ver = tag_name.trim_start_matches(|c| c == 'v' || c == 'V').to_string();
    let release_url = json
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://github.com/tzungli/roop2/releases/latest")
        .to_string();
    let release_notes = json
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let published_at = json
        .get("published_at")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut chosen_asset: Option<(String, String, u64)> = None;
    let mut msi_fallback: Option<(String, String, u64)> = None;
    let mut checksums_url: Option<String> = None;

    if let Some(assets) = json.get("assets").and_then(|v| v.as_array()) {
        for a in assets {
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let dl_url = a.get("browser_download_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let size = a.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            let lower = name.to_lowercase();
            if lower == "sha256sums.txt" && !dl_url.is_empty() {
                checksums_url = Some(dl_url);
            } else if lower.ends_with("-setup.exe") && !dl_url.is_empty() {
                chosen_asset = Some((name, dl_url, size));
            } else if lower.ends_with(".msi") && !dl_url.is_empty() {
                msi_fallback = Some((name, dl_url, size));
            }
        }
    }

    let picked = chosen_asset.or(msi_fallback);
    let update_available = !latest_ver.is_empty() && is_newer_version(&current_ver, &latest_ver);

    Ok(serde_json::json!({
        "currentVersion": current_ver,
        "latestVersion": latest_ver,
        "tagName": tag_name,
        "updateAvailable": update_available,
        "isStoreManaged": false,
        "releaseUrl": release_url,
        "releaseNotes": release_notes,
        "publishedAt": published_at,
        "assetName": picked.as_ref().map(|p| p.0.clone()),
        "assetUrl": picked.as_ref().map(|p| p.1.clone()),
        "assetSize": picked.as_ref().map(|p| p.2).unwrap_or(0),
        "checksumsUrl": checksums_url
    }))
}

#[tauri::command]
pub async fn updater_download_and_install(
    app: AppHandle,
    asset_url: String,
    asset_name: String,
    checksums_url: Option<String>,
) -> Result<serde_json::Value, String> {
    if is_microsoft_store_package() {
        return Err("Microsoft Store 版本由商店自動管理更新".to_string());
    }
    if !is_allowed_release_url(&asset_url) {
        return Err("更新下載網址不在允許的 GitHub 官方網域內".to_string());
    }
    let safe_name = std::path::Path::new(&asset_name)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "無效的安裝檔名稱".to_string())?
        .to_string();
    let lower_name = safe_name.to_lowercase();
    if !(lower_name.ends_with("-setup.exe") || lower_name.ends_with(".msi")) {
        return Err("僅允許執行官方 Windows 安裝程式 (-setup.exe / .msi)".to_string());
    }

    let client = crate::commands::http::shared_http_client();

    let _ = app.emit("updater:progress", serde_json::json!({
        "stage": "downloading",
        "downloaded": 0u64,
        "total": 0u64,
        "percent": 0u32
    }));

    let mut resp = client
        .get(&asset_url)
        .timeout(std::time::Duration::from_secs(300))
        .header("User-Agent", format!("rooPlayer/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .map_err(|e| format!("下載更新檔失敗: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("下載更新檔失敗: HTTP {}", resp.status().as_u16()));
    }

    let total_bytes = resp.content_length().unwrap_or(0);
    let update_dir: PathBuf = std::env::temp_dir().join("rooPlayer-updates");
    std::fs::create_dir_all(&update_dir).map_err(|e| format!("建立暫存目錄失敗: {}", e))?;
    let dest_path = update_dir.join(&safe_name);

    let mut file = std::fs::File::create(&dest_path)
        .map_err(|e| format!("建立更新檔失敗: {}", e))?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_emit_pct: u32 = 0;

    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("下載中斷: {}", e))? {
        file.write_all(&chunk).map_err(|e| format!("寫入檔案失敗: {}", e))?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        let pct = if total_bytes > 0 {
            ((downloaded * 100) / total_bytes).min(100) as u32
        } else {
            0
        };
        if pct >= last_emit_pct.saturating_add(2) || downloaded == total_bytes {
            last_emit_pct = pct;
            let _ = app.emit("updater:progress", serde_json::json!({
                "stage": "downloading",
                "downloaded": downloaded,
                "total": total_bytes,
                "percent": pct
            }));
        }
    }
    file.flush().map_err(|e| format!("寫入完成失敗: {}", e))?;
    drop(file);

    let actual_sha256 = format!("{:x}", hasher.finalize());

    // Verify SHA-256 against official SHA256SUMS.txt if present
    let mut verified = false;
    if let Some(cs_url) = checksums_url.filter(|u| !u.trim().is_empty()) {
        if !is_allowed_release_url(&cs_url) {
            let _ = std::fs::remove_file(&dest_path);
            return Err("校驗檔網址不在允許的 GitHub 官方網域內".to_string());
        }
        let _ = app.emit("updater:progress", serde_json::json!({
            "stage": "verifying",
            "downloaded": downloaded,
            "total": total_bytes,
            "percent": 100u32
        }));
        let cs_resp = client
            .get(&cs_url)
            .header("User-Agent", format!("rooPlayer/{}", env!("CARGO_PKG_VERSION")))
            .send()
            .await
            .map_err(|e| format!("無法下載 SHA256SUMS.txt: {}", e))?;
        if cs_resp.status().is_success() {
            let cs_text = cs_resp.text().await.unwrap_or_default();
            if let Some(expected) = parse_sha256_for_asset(&cs_text, &safe_name) {
                if expected != actual_sha256 {
                    let _ = std::fs::remove_file(&dest_path);
                    return Err(format!(
                        "SHA-256 校驗失敗，已中止安裝並刪除檔案（預期 {}，實際 {}）",
                        &expected[..12],
                        &actual_sha256[..12]
                    ));
                }
                verified = true;
            }
        }
    }

    let _ = app.emit("updater:progress", serde_json::json!({
        "stage": "launching",
        "downloaded": downloaded,
        "total": total_bytes,
        "percent": 100u32,
        "verified": verified
    }));

    // Launch installer detached
    let spawn_res = if lower_name.ends_with(".msi") {
        std::process::Command::new("msiexec")
            .arg("/i")
            .arg(&dest_path)
            .spawn()
    } else {
        std::process::Command::new(&dest_path).spawn()
    };

    if let Err(e) = spawn_res {
        return Err(format!("啟動安裝程式失敗: {}", e));
    }

    // Exit app shortly so installer can overwrite rooplayer.exe cleanly
    let app_exit = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(650)).await;
        app_exit.exit(0);
    });

    Ok(serde_json::json!({
        "ok": true,
        "verified": verified,
        "sha256": actual_sha256,
        "installerPath": dest_path.to_string_lossy()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparison() {
        assert!(is_newer_version("0.2.3", "0.2.4"));
        assert!(is_newer_version("0.2.3", "v0.3.0"));
        assert!(is_newer_version("0.2.3", "1.0.0"));
        assert!(!is_newer_version("0.2.3", "0.2.3"));
        assert!(!is_newer_version("0.2.3", "v0.2.2"));
    }

    #[test]
    fn test_parse_sha256_for_asset() {
        let sample = "\u{feff}52c7326b74bb8ef5625acb500177f41451dd6ab1caf544a2db4899c7f1a8bbe7  rooPlayer_0.2.3_x64_en-US.msi\r\n\
                      56a575f225bcf97ed1fa31fba120baf9ba05d58a81beb6d4b03926b243e5a1a4  rooPlayer_0.2.3_x64-setup.exe\r\n";
        assert_eq!(
            parse_sha256_for_asset(sample, "rooPlayer_0.2.3_x64-setup.exe").as_deref(),
            Some("56a575f225bcf97ed1fa31fba120baf9ba05d58a81beb6d4b03926b243e5a1a4")
        );
        assert_eq!(parse_sha256_for_asset(sample, "missing.exe"), None);
    }

    #[test]
    fn test_allowed_release_url() {
        assert!(is_allowed_release_url("https://github.com/tzungli/roop2/releases/download/v0.2.3/rooPlayer_0.2.3_x64-setup.exe"));
        assert!(!is_allowed_release_url("https://evil.example.com/rooPlayer_0.2.3_x64-setup.exe"));
    }
}
