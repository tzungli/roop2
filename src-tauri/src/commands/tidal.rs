use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use anyhow::Result;

pub struct TidalState {
    pub cache_file: PathBuf,
    pub http_client: reqwest::Client,
    pub auth_data: Arc<RwLock<Option<serde_json::Value>>>,
}

impl TidalState {
    pub fn new(cache_dir: PathBuf) -> Self {
        let cache_file = cache_dir.join("tidal-auth.json");
        let initial_auth = if let Ok(data) = fs::read_to_string(&cache_file) {
            serde_json::from_str(&data).ok()
        } else {
            None
        };
        Self {
            cache_file,
            http_client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            auth_data: Arc::new(RwLock::new(initial_auth)),
        }
    }
}

#[tauri::command]
pub async fn tidal_get_stored_auth(state: State<'_, TidalState>) -> Result<Option<serde_json::Value>, String> {
    Ok(state.auth_data.read().unwrap().clone())
}

#[tauri::command]
pub async fn tidal_save_auth(state: State<'_, TidalState>, data: serde_json::Value) -> Result<(), String> {
    *state.auth_data.write().unwrap() = Some(data.clone());
    if let Some(parent) = state.cache_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json_str) = serde_json::to_string_pretty(&data) {
        let _ = fs::write(&state.cache_file, json_str);
    }
    Ok(())
}

#[tauri::command]
pub async fn tidal_logout(state: State<'_, TidalState>) -> Result<(), String> {
    *state.auth_data.write().unwrap() = None;
    let _ = fs::remove_file(&state.cache_file);
    Ok(())
}

#[tauri::command]
pub async fn tidal_focus_login_window(app: AppHandle) -> Result<bool, String> {
    if let Some(win) = app.get_webview_window("tidal-login") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub async fn tidal_probe_auth(app: AppHandle) -> Result<serde_json::Value, String> {
    if let Some(win) = app.get_webview_window("tidal-login") {
        let _ = win.eval(r#"
            (function() {
                try {
                    for (var i = 0; i < localStorage.length; i++) {
                        var v = localStorage.getItem(localStorage.key(i));
                        if (!v) continue;
                        var m = v.match(/eyJ[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{10,}/);
                        if (m) {
                            window.location.href = 'https://rooplayer.local/auth?token=' + encodeURIComponent(m[0]);
                            return;
                        }
                    }
                } catch(_) {}
            })()
        "#);
    }
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn tidal_login_with_window(
    app: AppHandle,
    state: State<'_, TidalState>,
) -> Result<serde_json::Value, String> {
    // 1. Close any existing login window
    if let Some(existing) = app.get_webview_window("tidal-login") {
        let _ = existing.close();
    }

    let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
    let close_tx = Arc::new(Mutex::new(Some(close_tx)));

    let login_url = tauri::Url::parse("https://listen.tidal.com/login")
        .map_err(|e| format!("Failed to parse login URL: {}", e))?;

    // In-page token capture script (Intercept headers, OAuth json responses, and storage)
    let init_script = r#"
        (function() {
            var captured = false;
            function sendToken(token) {
                if (captured || !token) return;
                token = String(token).trim();
                if (token.length < 25) return;
                if (!token.startsWith('eyJ') && token.indexOf('.') === -1) return;
                captured = true;
                try {
                    window.location.href = 'https://rooplayer.local/auth?token=' + encodeURIComponent(token);
                } catch(_) {}
            }

            // Hook fetch (both request headers and json responses)
            var origFetch = window.fetch;
            window.fetch = function() {
                try {
                    var req = arguments[0];
                    if (req && req.headers) {
                        var h0 = req.headers;
                        var auth0 = (typeof Headers !== 'undefined' && h0 instanceof Headers)
                            ? (h0.get('authorization') || h0.get('Authorization'))
                            : (h0['Authorization'] || h0['authorization']);
                        if (auth0 && String(auth0).toLowerCase().indexOf('bearer ') === 0) {
                            sendToken(String(auth0).slice(7).trim());
                        }
                    }
                    var opts = arguments[1];
                    if (opts && opts.headers) {
                        var h = opts.headers;
                        var auth = null;
                        if (typeof Headers !== 'undefined' && h instanceof Headers) {
                            auth = h.get('authorization') || h.get('Authorization');
                        } else if (typeof h === 'object') {
                            auth = h['Authorization'] || h['authorization'];
                        }
                        if (auth && String(auth).toLowerCase().indexOf('bearer ') === 0) {
                            sendToken(String(auth).slice(7).trim());
                        }
                    }
                } catch(_) {}

                var p = origFetch.apply(this, arguments);
                if (p && typeof p.then === 'function') {
                    p.then(function(res) {
                        try {
                            if (res && res.clone) {
                                res.clone().json().then(function(data) {
                                    if (data) {
                                        if (data.access_token) sendToken(data.access_token);
                                        else if (data.token) sendToken(data.token);
                                        else if (data.session && data.session.token) sendToken(data.session.token);
                                    }
                                }).catch(function() {});
                            }
                        } catch(_) {}
                    }).catch(function() {});
                }
                return p;
            };

            // Hook XMLHttpRequest
            if (typeof XMLHttpRequest !== 'undefined' && XMLHttpRequest.prototype) {
                var origSetHeader = XMLHttpRequest.prototype.setRequestHeader;
                XMLHttpRequest.prototype.setRequestHeader = function(header, value) {
                    try {
                        if (header && String(header).toLowerCase() === 'authorization' && value && String(value).toLowerCase().indexOf('bearer ') === 0) {
                            sendToken(String(value).slice(7).trim());
                        }
                    } catch(_) {}
                    return origSetHeader.apply(this, arguments);
                };
            }

            // Periodic storage scan (every 400ms)
            function scanStorage() {
                if (captured) return;
                try {
                    var stores = [window.localStorage, window.sessionStorage];
                    for (var sIdx = 0; sIdx < stores.length; sIdx++) {
                        var s = stores[sIdx];
                        if (!s) continue;
                        for (var i = 0; i < s.length; i++) {
                            var k = s.key(i);
                            var v = s.getItem(k);
                            if (!v) continue;
                            var m = v.match(/eyJ[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{10,}/);
                            if (m) {
                                sendToken(m[0]);
                                return;
                            }
                        }
                    }
                } catch(_) {}
            }

            setInterval(scanStorage, 400);
            if (document.readyState === 'complete' || document.readyState === 'interactive') {
                scanStorage();
            } else {
                document.addEventListener('DOMContentLoaded', scanStorage);
            }

            // In-page floating sync banner
            setInterval(function() {
                if (captured) return;
                if (window.location && window.location.hostname && window.location.hostname.indexOf('tidal.com') !== -1 && window.location.pathname.indexOf('/login') === -1) {
                    if (!document.getElementById('rooplayer-sync-banner')) {
                        var banner = document.createElement('div');
                        banner.id = 'rooplayer-sync-banner';
                        banner.style.cssText = 'position:fixed;top:12px;left:50%;transform:translateX(-50%);z-index:99999999;background:#000;border:2px solid #00e5ff;padding:8px 20px;border-radius:24px;color:#00e5ff;font-family:sans-serif;font-size:13px;font-weight:bold;cursor:pointer;box-shadow:0 6px 20px rgba(0,229,255,0.4);display:flex;align-items:center;gap:8px;';
                        banner.innerHTML = '<span>💎 已進入 TIDAL 首頁？點此立即同步授權至 rooPlayer</span>';
                        banner.onclick = function() {
                            banner.innerHTML = '<span>⏳ 正在同步授權...</span>';
                            scanStorage();
                            try { fetch('https://api.tidal.com/v1/sessions').catch(function() {}); } catch(_) {}
                        };
                        if (document.body) document.body.appendChild(banner);
                    }
                }
            }, 1000);
        })();
    "#;

    let token_tx_clone = token_tx.clone();
    let win = WebviewWindowBuilder::new(&app, "tidal-login", WebviewUrl::External(login_url))
        .title("TIDAL 官方帳號登入 — rooPlayer")
        .inner_size(560.0, 780.0)
        .center()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
        .initialization_script(init_script)
        .on_navigation(move |url| {
            if url.host_str() == Some("rooplayer.local") {
                if let Some(query) = url.query() {
                    for param in query.split('&') {
                        let mut parts = param.splitn(2, '=');
                        if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                            if k == "token" {
                                let decoded = urlencoding::decode(v).unwrap_or_default().to_string();
                                let _ = token_tx_clone.send(decoded);
                            }
                        }
                    }
                }
                return false;
            }
            true
        })
        .build()
        .map_err(|e| format!("Failed to create TIDAL login window: {}", e))?;

    let _ = win.show();
    let _ = win.unminimize();
    let _ = win.set_focus();

    let close_tx_clone = close_tx.clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            if let Ok(mut lock) = close_tx_clone.lock() {
                if let Some(tx) = lock.take() {
                    let _ = tx.send(());
                }
            }
        }
    });

    // Wait for token OR user window close
    tokio::select! {
        Some(token) = token_rx.recv() => {
            tracing::info!("TIDAL token received, verifying with sessions endpoint...");
            let client = &state.http_client;
            let sess_resp = client
                .get("https://api.tidal.com/v1/sessions")
                .header("Authorization", format!("Bearer {}", token))
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
                .send()
                .await;

            let (user_id, session_id, country_code) = match sess_resp {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        (
                            json.get("userId").and_then(|v| v.as_i64()).map(|id| id.to_string()).unwrap_or_default(),
                            json.get("sessionId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            json.get("countryCode").and_then(|v| v.as_str()).unwrap_or("TW").to_string(),
                        )
                    } else {
                        (String::new(), String::new(), "TW".to_string())
                    }
                }
                _ => (String::new(), String::new(), "TW".to_string()),
            };

            let now_iso = {
                let dur = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                format!("{}Z", dur.as_secs())
            };

            let auth_data = serde_json::json!({
                "accessToken": token,
                "userId": user_id,
                "sessionId": session_id,
                "countryCode": country_code,
                "channel": "",
                "updatedAt": now_iso
            });

            *state.auth_data.write().unwrap() = Some(auth_data.clone());
            if let Some(parent) = state.cache_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(
                &state.cache_file,
                serde_json::to_string_pretty(&auth_data).unwrap_or_default(),
            );

            // Close login window
            if let Some(w) = app.get_webview_window("tidal-login") {
                let _ = w.close();
            }

            Ok(serde_json::json!({
                "success": true,
                "auth": auth_data
            }))
        }
        _ = close_rx => {
            Ok(serde_json::json!({
                "success": false,
                "error": "使用者關閉了登入視窗"
            }))
        }
    }
}

#[tauri::command]
pub async fn tidal_api(
    state: State<'_, TidalState>,
    options: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let endpoint = options.get("endpoint").and_then(|v| v.as_str()).unwrap_or("");
    let method = options.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
    let token = options.get("token").and_then(|v| v.as_str()).unwrap_or("");

    let url = if endpoint.starts_with("http") {
        endpoint.to_string()
    } else {
        format!("https://api.tidal.com/v1{}", endpoint)
    };

    let method_upper = method.to_uppercase();
    let mut req = match method_upper.as_str() {
        "POST" => state.http_client.post(&url),
        "PUT" => state.http_client.put(&url),
        "DELETE" => state.http_client.delete(&url),
        "PATCH" => state.http_client.patch(&url),
        _ => state.http_client.get(&url),
    };

    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    if method_upper != "GET" && method_upper != "HEAD" {
        if let Some(body) = options.get("body") {
            if !body.is_null() {
                req = req.json(body);
            }
        }
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("TIDAL API HTTP {}: {}", status.as_u16(), text));
    }

    let json_val: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        format!("JSON parse error ({}): {}", e, text)
    })?;
    Ok(json_val)
}

#[tauri::command]
pub async fn tidal_get_stream_url(
    _track_id: serde_json::Value,
    _playback_info: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn tidal_cache_track(
    _track_id: serde_json::Value,
    _stream_url_or_info: serde_json::Value,
    _optional_info: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({ "ok": true }))
}
