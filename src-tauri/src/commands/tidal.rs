use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::Semaphore;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTidalEntry {
    pub ts_secs: u64,
    pub data: serde_json::Value,
}

pub struct TidalState {
    pub cache_file: PathBuf,
    pub api_cache_file: PathBuf,
    pub http_client: reqwest::Client,
    pub auth_data: Arc<RwLock<Option<serde_json::Value>>>,
    pub api_cache: Arc<RwLock<HashMap<String, CachedTidalEntry>>>,
    pub api_semaphore: Arc<Semaphore>,
    pub rate_limit_until: Arc<Mutex<Option<Instant>>>,
    pub active_queries: Arc<AtomicUsize>,
}

struct TidalQueryGuard<'a> {
    app: &'a AppHandle,
    counter: &'a Arc<AtomicUsize>,
}

impl<'a> TidalQueryGuard<'a> {
    fn enter(app: &'a AppHandle, counter: &'a Arc<AtomicUsize>) -> Self {
        let count = counter.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = app.emit(
            "tidal:queryState",
            serde_json::json!({ "active": true, "count": count }),
        );
        Self { app, counter }
    }
}

impl<'a> Drop for TidalQueryGuard<'a> {
    fn drop(&mut self) {
        let prev = self.counter.fetch_sub(1, Ordering::SeqCst);
        let count = prev.saturating_sub(1);
        let _ = self.app.emit(
            "tidal:queryState",
            serde_json::json!({ "active": count > 0, "count": count }),
        );
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl TidalState {
    pub fn new(cache_dir: PathBuf) -> Self {
        let cache_file = cache_dir.join("tidal-auth.json");
        let api_cache_file = cache_dir.join("tidal-api-cache.json");
        let initial_auth = if let Ok(data) = fs::read_to_string(&cache_file) {
            serde_json::from_str(&data).ok()
        } else {
            None
        };
        let now = now_unix_secs();
        let initial_api_cache: HashMap<String, CachedTidalEntry> =
            if let Ok(data) = fs::read_to_string(&api_cache_file) {
                serde_json::from_str::<HashMap<String, CachedTidalEntry>>(&data)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(_, v)| now.saturating_sub(v.ts_secs) < 14 * 86400)
                    .collect()
            } else {
                HashMap::new()
            };
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            reqwest::header::ORIGIN,
            reqwest::header::HeaderValue::from_static("https://listen.tidal.com"),
        );
        default_headers.insert(
            reqwest::header::REFERER,
            reqwest::header::HeaderValue::from_static("https://listen.tidal.com/"),
        );
        default_headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json, text/plain, */*"),
        );
        default_headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("zh-TW,zh;q=0.9,en-US;q=0.8,en;q=0.7"),
        );
        default_headers.insert(
            reqwest::header::HeaderName::from_static("sec-fetch-dest"),
            reqwest::header::HeaderValue::from_static("empty"),
        );
        default_headers.insert(
            reqwest::header::HeaderName::from_static("sec-fetch-mode"),
            reqwest::header::HeaderValue::from_static("cors"),
        );
        default_headers.insert(
            reqwest::header::HeaderName::from_static("sec-fetch-site"),
            reqwest::header::HeaderValue::from_static("same-site"),
        );
        Self {
            cache_file,
            api_cache_file,
            http_client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
                .default_headers(default_headers)
                .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
                .connect_timeout(Duration::from_secs(6))
                .timeout(Duration::from_secs(12))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            auth_data: Arc::new(RwLock::new(initial_auth)),
            api_cache: Arc::new(RwLock::new(initial_api_cache)),
            api_semaphore: Arc::new(Semaphore::new(3)),
            rate_limit_until: Arc::new(Mutex::new(None)),
            active_queries: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn cache_ttl_secs(endpoint: &str) -> u64 {
        if endpoint.starts_with("/sessions") || endpoint.contains("playbackinfo") {
            return 0;
        }
        if endpoint.starts_with("/albums/") || endpoint.starts_with("/artists/") {
            return 14 * 86400; // 14 days for album/artist metadata, covers, portraits, discographies & reviews
        }
        if endpoint.contains("/favorites/") || endpoint.contains("/users/") {
            return 1800; // 30 minutes for user collection (auto-invalidated on writes)
        }
        24 * 3600 // 24 hours for search, curated Hi-Res picks, official playlists, home modules
    }

    pub fn get_cached_api(&self, key: &str, ttl_secs: u64) -> Option<serde_json::Value> {
        if ttl_secs == 0 {
            return None;
        }
        let now = now_unix_secs();
        let guard = self.api_cache.read().ok()?;
        let entry = guard.get(key)?;
        if now.saturating_sub(entry.ts_secs) <= ttl_secs {
            Some(entry.data.clone())
        } else {
            None
        }
    }

    /// Stale-on-error fallback: returns any previously cached response even if expired,
    /// protecting the UI during temporary offline or HTTP 429/503 windows.
    pub fn get_stale_cached_api(&self, key: &str) -> Option<serde_json::Value> {
        let guard = self.api_cache.read().ok()?;
        guard.get(key).map(|entry| entry.data.clone())
    }

    pub fn put_cached_api(&self, key: String, data: serde_json::Value) {
        let now = now_unix_secs();
        let snapshot = {
            let Ok(mut guard) = self.api_cache.write() else { return };
            guard.insert(key, CachedTidalEntry { ts_secs: now, data });
            if guard.len() > 1500 {
                let mut pairs: Vec<(String, u64)> =
                    guard.iter().map(|(k, v)| (k.clone(), v.ts_secs)).collect();
                pairs.sort_by_key(|(_, ts)| *ts);
                let remove_count = guard.len().saturating_sub(1250);
                for (k, _) in pairs.into_iter().take(remove_count) {
                    guard.remove(&k);
                }
            }
            guard.clone()
        };
        if let Some(parent) = self.api_cache_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json_str) = serde_json::to_string(&snapshot) {
            let _ = fs::write(&self.api_cache_file, json_str);
        }
    }

    pub fn invalidate_user_cache(&self) {
        if let Ok(mut guard) = self.api_cache.write() {
            guard.retain(|k, _| !k.contains("/users/") && !k.contains("/favorites/") && !k.contains("/playlists/"));
        }
    }

    pub async fn wait_rate_limit_cooldown(&self) {
        let wait_dur = {
            let guard = self.rate_limit_until.lock().unwrap();
            guard.and_then(|until| until.checked_duration_since(Instant::now()))
        };
        if let Some(dur) = wait_dur {
            tokio::time::sleep(dur).await;
        }
    }

    pub fn set_rate_limit_cooldown(&self, dur: Duration) {
        let target = Instant::now() + dur;
        let mut guard = self.rate_limit_until.lock().unwrap();
        let next = match *guard {
            Some(existing) if existing > target => existing,
            _ => target,
        };
        *guard = Some(next);
    }
}

fn parse_retry_after_secs(headers: &reqwest::header::HeaderMap, attempt: u32) -> Duration {
    if let Some(val) = headers.get(reqwest::header::RETRY_AFTER).and_then(|h| h.to_str().ok()) {
        if let Ok(secs) = val.trim().parse::<u64>() {
            return Duration::from_secs(secs.clamp(1, 15));
        }
    }
    // Exponential backoff: 1.2s, 2.5s, 5.0s
    let ms = match attempt {
        0 => 1200,
        1 => 2500,
        _ => 5000,
    };
    Duration::from_millis(ms)
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

/// Silently refresh an expired TIDAL JWT access token in a hidden background WebView2 window
/// using the persisted `listen.tidal.com` login cookies. Ignores the previous expired token
/// and verifies any captured candidate against `/v1/sessions` before accepting it.
#[tauri::command]
pub async fn tidal_silent_refresh(
    app: AppHandle,
    state: State<'_, TidalState>,
) -> Result<serde_json::Value, String> {
    if let Some(existing) = app.get_webview_window("tidal-silent-refresh") {
        let _ = existing.close();
    }

    let old_token = state
        .auth_data
        .read()
        .unwrap()
        .as_ref()
        .and_then(|v| v.get("accessToken"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let refresh_url = tauri::Url::parse("https://listen.tidal.com/")
        .map_err(|e| format!("Failed to parse TIDAL URL: {}", e))?;

    let old_token_json = serde_json::to_string(&old_token).unwrap_or_else(|_| "\"\"".to_string());
    let init_script = format!(
        r#"
        (function() {{
            var oldToken = {old_token_json};
            var sentSet = {{}};
            function sendToken(token) {{
                if (!token) return;
                token = String(token).trim();
                if (token.length < 25) return;
                if (!token.startsWith('eyJ') && token.indexOf('.') === -1) return;
                if (oldToken && token === oldToken) return;
                if (sentSet[token]) return;
                sentSet[token] = true;
                try {{
                    window.location.href = 'https://rooplayer.local/silent-auth?token=' + encodeURIComponent(token);
                }} catch(_) {{}}
            }}

            var origFetch = window.fetch;
            window.fetch = function() {{
                try {{
                    var req = arguments[0];
                    if (req && req.headers) {{
                        var h0 = req.headers;
                        var auth0 = (typeof Headers !== 'undefined' && h0 instanceof Headers)
                            ? (h0.get('authorization') || h0.get('Authorization'))
                            : (h0['Authorization'] || h0['authorization']);
                        if (auth0 && String(auth0).toLowerCase().indexOf('bearer ') === 0) {{
                            sendToken(String(auth0).slice(7).trim());
                        }}
                    }}
                    var opts = arguments[1];
                    if (opts && opts.headers) {{
                        var h = opts.headers;
                        var auth = (typeof Headers !== 'undefined' && h instanceof Headers)
                            ? (h.get('authorization') || h.get('Authorization'))
                            : (h['Authorization'] || h['authorization']);
                        if (auth && String(auth).toLowerCase().indexOf('bearer ') === 0) {{
                            sendToken(String(auth).slice(7).trim());
                        }}
                    }}
                }} catch(_) {{}}

                var p = origFetch.apply(this, arguments);
                if (p && typeof p.then === 'function') {{
                    p.then(function(res) {{
                        try {{
                            if (res && res.clone) {{
                                res.clone().json().then(function(data) {{
                                    if (data) {{
                                        if (data.access_token) sendToken(data.access_token);
                                        else if (data.token) sendToken(data.token);
                                        else if (data.session && data.session.token) sendToken(data.session.token);
                                    }}
                                }}).catch(function() {{}});
                            }}
                        }} catch(_) {{}}
                    }}).catch(function() {{}});
                }}
                return p;
            }};

            if (typeof XMLHttpRequest !== 'undefined' && XMLHttpRequest.prototype) {{
                var origSetHeader = XMLHttpRequest.prototype.setRequestHeader;
                XMLHttpRequest.prototype.setRequestHeader = function(header, value) {{
                    try {{
                        if (header && String(header).toLowerCase() === 'authorization' && value && String(value).toLowerCase().indexOf('bearer ') === 0) {{
                            sendToken(String(value).slice(7).trim());
                        }}
                    }} catch(_) {{}}
                    return origSetHeader.apply(this, arguments);
                }};
            }}

            function scanStorage() {{
                try {{
                    var stores = [window.localStorage, window.sessionStorage];
                    for (var sIdx = 0; sIdx < stores.length; sIdx++) {{
                        var s = stores[sIdx];
                        if (!s) continue;
                        for (var i = 0; i < s.length; i++) {{
                            var v = s.getItem(s.key(i));
                            if (!v) continue;
                            var matches = v.match(/eyJ[A-Za-z0-9_-]{{15,}}\.[A-Za-z0-9_-]{{15,}}\.[A-Za-z0-9_-]{{10,}}/g);
                            if (matches) {{
                                for (var j = 0; j < matches.length; j++) {{
                                    sendToken(matches[j]);
                                }}
                            }}
                        }}
                    }}
                }} catch(_) {{}}
            }}

            setInterval(scanStorage, 350);
            setTimeout(scanStorage, 200);
        }})();
        "#
    );

    let token_tx_clone = token_tx.clone();
    let win_res = WebviewWindowBuilder::new(&app, "tidal-silent-refresh", WebviewUrl::External(refresh_url))
        .title("TIDAL Silent Refresh")
        .inner_size(400.0, 400.0)
        .visible(false)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
        .initialization_script(&init_script)
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
        .build();

    if let Err(e) = win_res {
        return Ok(serde_json::json!({ "success": false, "error": format!("Silent refresh window error: {}", e) }));
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(6);
    let mut refreshed_auth: Option<serde_json::Value> = None;
    let mut tested_tokens = std::collections::HashSet::<String>::new();

    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, token_rx.recv()).await {
            Ok(Some(token)) => {
                if !tested_tokens.insert(token.clone()) {
                    continue;
                }
                let sess_resp = state
                    .http_client
                    .get("https://api.tidal.com/v1/sessions")
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;

                if let Ok(resp) = sess_resp {
                    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        break;
                    }
                    if resp.status().is_success() {
                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                            let user_id = json.get("userId").and_then(|v| v.as_i64()).map(|id| id.to_string()).unwrap_or_default();
                            let session_id = json.get("sessionId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let country_code = json.get("countryCode").and_then(|v| v.as_str()).unwrap_or("TW").to_string();
                            let now_iso = format!(
                                "{}Z",
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs()
                            );
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
                            tracing::info!("TIDAL token silently refreshed successfully");
                            refreshed_auth = Some(auth_data);
                            break;
                        }
                    }
                }
            }
            _ => break,
        }
    }

    if let Some(w) = app.get_webview_window("tidal-silent-refresh") {
        let _ = w.close();
    }

    if let Some(auth) = refreshed_auth {
        Ok(serde_json::json!({ "success": true, "auth": auth }))
    } else {
        Ok(serde_json::json!({ "success": false, "error": "Silent refresh timed out" }))
    }
}

#[tauri::command]
pub async fn tidal_api(
    app: AppHandle,
    state: State<'_, TidalState>,
    options: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let endpoint = options.get("endpoint").and_then(|v| v.as_str()).unwrap_or("").trim();
    let method = options.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
    let mut token = options
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if token.is_empty() {
        if let Some(stored) = state.auth_data.read().unwrap().as_ref() {
            if let Some(t) = stored.get("accessToken").and_then(|v| v.as_str()) {
                token = t.trim().to_string();
            }
        }
    }

    // Route /search to api.tidal.com/v2/search (TIDAL web player tokens get HTTP 429 on /v1/search,
    // whereas /v2/search returns 200 OK with the exact same JSON schema).
    let url = if endpoint.starts_with("http") {
        endpoint.to_string()
    } else if endpoint.starts_with("/v2/") {
        format!("https://api.tidal.com{}", endpoint)
    } else if endpoint == "/search" || endpoint.starts_with("/search?") {
        format!("https://api.tidal.com/v2{}", endpoint)
    } else {
        format!("https://api.tidal.com/v1{}", endpoint)
    };

    let method_upper = method.to_uppercase();
    let force_refresh = options
        .get("forceRefresh")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let ttl_secs = if method_upper == "GET" {
        TidalState::cache_ttl_secs(endpoint)
    } else {
        state.invalidate_user_cache();
        0
    };

    if method_upper == "GET" && !force_refresh && ttl_secs > 0 {
        if let Some(cached) = state.get_cached_api(&url, ttl_secs) {
            return Ok(cached);
        }
    }

    let _query_guard = TidalQueryGuard::enter(&app, &state.active_queries);

    let max_retries: u32 = 1;
    let mut last_err = "TIDAL API 請求過於頻繁（HTTP 429），請稍候再試".to_string();

    for attempt in 0..=max_retries {
        state.wait_rate_limit_cooldown().await;
        let _permit = state
            .api_semaphore
            .acquire()
            .await
            .map_err(|e| format!("TIDAL 限流器錯誤: {}", e))?;

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

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                drop(_permit);
                if method_upper == "GET" && ttl_secs > 0 {
                    if let Some(stale) = state.get_stale_cached_api(&url) {
                        tracing::warn!("[TIDAL] Network error on {} ({}), serving stale cache", endpoint, e);
                        return Ok(stale);
                    }
                }
                return Err(e.to_string());
            }
        };
        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            let wait_dur = parse_retry_after_secs(resp.headers(), attempt);
            let err_text = resp.text().await.unwrap_or_default();
            drop(_permit);
            state.set_rate_limit_cooldown(wait_dur);
            let snippet: String = err_text.chars().take(120).collect();
            last_err = if snippet.trim().is_empty() {
                format!("TIDAL API HTTP {}（{}）", status.as_u16(), url.split('?').next().unwrap_or(&url))
            } else {
                format!("TIDAL API HTTP {}: {}", status.as_u16(), snippet.trim())
            };
            if attempt < max_retries {
                tracing::warn!("[TIDAL] HTTP {} on {} — backing off {:?}", status.as_u16(), endpoint, wait_dur);
                tokio::time::sleep(wait_dur).await;
                continue;
            }
            if method_upper == "GET" && ttl_secs > 0 {
                if let Some(stale) = state.get_stale_cached_api(&url) {
                    tracing::warn!("[TIDAL] HTTP {} on {} — serving stale cache fallback", status.as_u16(), endpoint);
                    return Ok(stale);
                }
            }
            return Err(last_err);
        }

        let text = resp.text().await.map_err(|e| e.to_string())?;
        drop(_permit);

        if !status.is_success() {
            return Err(format!("TIDAL API HTTP {}: {}", status.as_u16(), text));
        }

        let json_val: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            format!("JSON parse error ({}): {}", e, text)
        })?;
        if method_upper == "GET" && ttl_secs > 0 {
            state.put_cached_api(url.clone(), json_val.clone());
        }
        return Ok(json_val);
    }

    Err(last_err)
}

fn clean_track_id(track_id: &serde_json::Value) -> String {
    let raw = match track_id {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.trim_start_matches("tidal_").to_string(),
        _ => track_id.to_string().replace('"', ""),
    };
    raw.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect()
}

fn decode_base64_str(input: &str) -> Option<String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut map = [255u8; 256];
    for (i, &b) in TABLE.iter().enumerate() {
        map[b as usize] = i as u8;
    }
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in &clean {
        if b == b'=' { break; }
        let val = map[b as usize];
        if val == 255 { return None; }
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    String::from_utf8(out).ok()
}

#[derive(Debug, Clone)]
pub struct ManifestSegments {
    pub init_url: Option<String>,
    pub segment_urls: Vec<String>,
}

pub fn parse_tidal_manifest(manifest_str: &str) -> Option<ManifestSegments> {
    let trimmed = manifest_str.trim();

    // 1. BTS JSON Manifest
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(arr) = v.get("urls").and_then(|u| u.as_array()) {
                let urls: Vec<String> = arr.iter().filter_map(|s| s.as_str().map(String::from)).collect();
                if !urls.is_empty() {
                    return Some(ManifestSegments {
                        init_url: None,
                        segment_urls: urls,
                    });
                }
            }
        }
        return None;
    }

    // 2. DASH XML Manifest
    let tpl_start = trimmed.find("<SegmentTemplate")?;
    let tpl_end = trimmed[tpl_start..].find('>')? + tpl_start;
    let tpl_tag = &trimmed[tpl_start..=tpl_end];

    let get_attr = |name: &str| -> Option<String> {
        let pattern = format!("{}=\"", name);
        let start = tpl_tag.find(&pattern)? + pattern.len();
        let end = tpl_tag[start..].find('"')? + start;
        Some(tpl_tag[start..end].to_string())
    };

    let media_tpl = get_attr("media")?;
    let init_tpl = get_attr("initialization");
    let start_number: u32 = get_attr("startNumber").and_then(|s| s.parse().ok()).unwrap_or(1);

    let mut segment_count = 0u32;
    if let Some(timeline_start) = trimmed.find("<SegmentTimeline>") {
        if let Some(timeline_end) = trimmed.find("</SegmentTimeline>") {
            let timeline_str = &trimmed[timeline_start..timeline_end];
            for s_match in timeline_str.split("<S ") {
                if let Some(s_end) = s_match.find("/>") {
                    let s_tag = &s_match[..s_end];
                    let r_count: u32 = if let Some(r_idx) = s_tag.find("r=\"") {
                        let r_val_str = &s_tag[r_idx + 3..];
                        if let Some(q_end) = r_val_str.find('"') {
                            r_val_str[..q_end].parse().unwrap_or(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    segment_count += r_count + 1;
                }
            }
        }
    }

    if segment_count == 0 {
        if let Some(dur_str) = get_attr("duration") {
            if let Ok(seg_dur) = dur_str.parse::<f64>() {
                let timescale: f64 = get_attr("timescale").and_then(|s| s.parse().ok()).unwrap_or(1.0);
                if let Some(mpd_idx) = trimmed.find("mediaPresentationDuration=\"PT") {
                    let dur_part = &trimmed[mpd_idx + 28..];
                    if let Some(end_q) = dur_part.find('"') {
                        let dur_text = &dur_part[..end_q];
                        let mut total_sec = 0.0;
                        let mut temp = String::new();
                        for c in dur_text.chars() {
                            if c.is_digit(10) || c == '.' {
                                temp.push(c);
                            } else if c == 'H' {
                                if let Ok(val) = temp.parse::<f64>() { total_sec += val * 3600.0; }
                                temp.clear();
                            } else if c == 'M' {
                                if let Ok(val) = temp.parse::<f64>() { total_sec += val * 60.0; }
                                temp.clear();
                            } else if c == 'S' {
                                if let Ok(val) = temp.parse::<f64>() { total_sec += val; }
                                temp.clear();
                            }
                        }
                        if seg_dur > 0.0 && total_sec > 0.0 {
                            segment_count = ((total_sec * timescale) / seg_dur).ceil() as u32;
                        }
                    }
                }
            }
        }
    }

    if segment_count == 0 {
        segment_count = 1;
    }

    let mut segment_urls = Vec::with_capacity(segment_count as usize);
    for i in 0..segment_count {
        let num = start_number + i;
        let url = media_tpl.replace("$Number$", &num.to_string());
        segment_urls.push(url);
    }

    let init_url = init_tpl.map(|t| t.replace("$Number$", &start_number.to_string()));

    Some(ManifestSegments {
        init_url,
        segment_urls,
    })
}

async fn ensure_tidal_track_cached(
    app: Option<&AppHandle>,
    state: &TidalState,
    safe_id: &str,
    playback_info: Option<&serde_json::Value>,
) -> Result<PathBuf, String> {
    let cache_dir = state
        .cache_file
        .parent()
        .map(|p| p.join("tidal_cache"))
        .unwrap_or_else(|| PathBuf::from("tidal_cache"));

    let _ = fs::create_dir_all(&cache_dir);
    let target_file = cache_dir.join(format!("{}.flac", safe_id));

    // Fast check if already cached
    if target_file.exists() {
        if let Ok(meta) = fs::metadata(&target_file) {
            if meta.len() > 64 * 1024 {
                let token = format!("tidal_{}", safe_id);
                let _ = roo_streamer::register_direct_file_stream(&token, &target_file, "audio/flac");
                return Ok(target_file);
            }
        }
    }

    let _query_guard = app.map(|a| TidalQueryGuard::enter(a, &state.active_queries));

    // Resolve manifest or direct URLs
    let mut manifest_str = String::new();
    let mut direct_urls = Vec::new();

    if let Some(info) = playback_info {
        if let Some(s) = info.as_str() {
            if s.starts_with("http") {
                direct_urls.push(s.to_string());
            }
        } else {
            if let Some(m) = info.get("manifest").and_then(|v| v.as_str()) {
                if m.trim().starts_with('<') || m.trim().starts_with('{') {
                    manifest_str = m.to_string();
                } else if let Some(decoded) = decode_base64_str(m) {
                    manifest_str = decoded;
                }
            }
            if let Some(urls) = info.get("urls").and_then(|u| u.as_array()) {
                for u in urls {
                    if let Some(s) = u.as_str() {
                        direct_urls.push(s.to_string());
                    }
                }
            }
        }
    }

    // If neither manifest nor direct URLs provided, fetch from TIDAL API directly
    if manifest_str.is_empty() && direct_urls.is_empty() {
        let (token, country_code) = {
            let guard = state.auth_data.read().unwrap();
            if let Some(auth) = guard.as_ref() {
                let tok = auth.get("accessToken").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let cc = auth.get("countryCode").and_then(|v| v.as_str()).unwrap_or("TW").to_string();
                (tok, cc)
            } else {
                (String::new(), "TW".to_string())
            }
        };

        if token.is_empty() {
            return Err("TIDAL 尚未登入且未提供串流 Manifest".to_string());
        }

        let url = format!(
            "https://api.tidal.com/v1/tracks/{}/playbackinfopostpaywall?countryCode={}&audioquality=LOSSLESS&playbackmode=STREAM&assetpresentation=FULL",
            safe_id, country_code
        );

        let mut body_json_opt: Option<serde_json::Value> = None;
        for attempt in 0..=3u32 {
            state.wait_rate_limit_cooldown().await;
            let _permit = state
                .api_semaphore
                .acquire()
                .await
                .map_err(|e| format!("TIDAL 限流器錯誤: {}", e))?;

            let resp = state
                .http_client
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .map_err(|e| format!("取得 TIDAL 播放資訊失敗: {}", e))?;

            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                || resp.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
            {
                let wait_dur = parse_retry_after_secs(resp.headers(), attempt);
                drop(_permit);
                state.set_rate_limit_cooldown(wait_dur);
                if attempt < 3 {
                    tokio::time::sleep(wait_dur).await;
                    continue;
                }
                return Err(format!("TIDAL 播放授權請求過於頻繁 HTTP {}", resp.status()));
            }

            if !resp.status().is_success() {
                return Err(format!("TIDAL 播放授權錯誤 HTTP {}", resp.status()));
            }

            let parsed: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("解析播放資訊 JSON 失敗: {}", e))?;
            body_json_opt = Some(parsed);
            break;
        }
        let body_json = body_json_opt
            .ok_or_else(|| "TIDAL 播放授權請求過於頻繁 HTTP 429".to_string())?;

        if let Some(m) = body_json.get("manifest").and_then(|v| v.as_str()) {
            if m.trim().starts_with('<') || m.trim().starts_with('{') {
                manifest_str = m.to_string();
            } else if let Some(decoded) = decode_base64_str(m) {
                manifest_str = decoded;
            }
        }
        if let Some(urls) = body_json.get("urls").and_then(|u| u.as_array()) {
            for u in urls {
                if let Some(s) = u.as_str() {
                    direct_urls.push(s.to_string());
                }
            }
        }
    }

    let mut all_urls = Vec::new();
    if !direct_urls.is_empty() {
        all_urls = direct_urls;
    } else if !manifest_str.is_empty() {
        let segments = parse_tidal_manifest(&manifest_str)
            .ok_or_else(|| "無法解析 TIDAL 串流 Manifest 分段".to_string())?;
        if let Some(init) = &segments.init_url {
            all_urls.push(init.clone());
        }
        all_urls.extend(segments.segment_urls);
    } else {
        return Err("無法取得 TIDAL 串流 Manifest 或音訊連結".to_string());
    }

    let total = all_urls.len();
    if total == 0 {
        return Err("未找到任何分段下載連結".to_string());
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
    let mut handles = Vec::with_capacity(total);

    for (idx, url) in all_urls.into_iter().enumerate() {
        let sem = semaphore.clone();
        let cl = state.http_client.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
            let resp = cl.get(&url).send().await.map_err(|e| format!("分段 {} 下載失敗: {}", idx, e))?;
            if !resp.status().is_success() {
                return Err(format!("分段 {} 回應 HTTP {}", idx, resp.status()));
            }
            let bytes = resp.bytes().await.map_err(|e| format!("分段 {} 讀取失敗: {}", idx, e))?;
            Ok::<_, String>((idx, bytes))
        }));
    }

    let mut results = Vec::with_capacity(total);
    for handle in handles {
        let res = handle.await.map_err(|e| format!("下載任務中止: {}", e))??;
        results.push(res);
    }
    results.sort_by_key(|(idx, _)| *idx);

    let total_size: usize = results.iter().map(|(_, b)| b.len()).sum();
    let mut concatenated = Vec::with_capacity(total_size);
    for (_, b) in results {
        concatenated.extend_from_slice(&b);
    }

    let flac_bytes = if roo_decoder::is_isobmff(&concatenated) {
        roo_decoder::extract_flac_from_isobmff(&concatenated).unwrap_or(concatenated)
    } else {
        concatenated
    };

    let temp_path = cache_dir.join(format!("{}.tmp_{}.flac", safe_id, std::process::id()));
    fs::write(&temp_path, &flac_bytes).map_err(|e| format!("寫入快取檔案失敗: {}", e))?;
    fs::rename(&temp_path, &target_file).map_err(|e| format!("覆寫快取檔案失敗: {}", e))?;

    let token = format!("tidal_{}", safe_id);
    let _ = roo_streamer::register_direct_file_stream(&token, &target_file, "audio/flac");

    Ok(target_file)
}

#[tauri::command]
pub async fn tidal_get_stream_url(
    app: AppHandle,
    state: State<'_, TidalState>,
    track_id: serde_json::Value,
    playback_info: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let safe_id = clean_track_id(&track_id);
    if safe_id.is_empty() {
        return Err("缺少有效的曲目 ID".to_string());
    }

    let target_file = ensure_tidal_track_cached(Some(&app), &state, &safe_id, playback_info.as_ref()).await?;

    let bit_depth = playback_info.as_ref().and_then(|i| i.get("bitDepth")).and_then(|v| v.as_u64()).unwrap_or(16);
    let sample_rate = playback_info.as_ref().and_then(|i| i.get("sampleRate")).and_then(|v| v.as_u64()).unwrap_or(44100);
    let audio_quality = playback_info.as_ref().and_then(|i| i.get("audioQuality")).and_then(|v| v.as_str()).unwrap_or("LOSSLESS");

    let token = format!("tidal_{}", safe_id);
    let stream_url = format!("http://127.0.0.1:39840/stream/{}/track.flac", token);

    Ok(serde_json::json!({
        "streamUrl": stream_url,
        "filePath": target_file.to_string_lossy(),
        "bitDepth": bit_depth,
        "sampleRate": sample_rate,
        "audioQuality": audio_quality
    }))
}

#[tauri::command]
pub async fn tidal_cache_track(
    app: AppHandle,
    state: State<'_, TidalState>,
    track_id: serde_json::Value,
    stream_url_or_info: Option<serde_json::Value>,
    optional_info: Option<serde_json::Value>,
) -> Result<Option<String>, String> {
    let safe_id = clean_track_id(&track_id);
    if safe_id.is_empty() {
        return Ok(None);
    }

    let cache_dir = state
        .cache_file
        .parent()
        .map(|p| p.join("tidal_cache"))
        .unwrap_or_else(|| PathBuf::from("tidal_cache"));

    let target_file = cache_dir.join(format!("{}.flac", safe_id));
    if target_file.exists() {
        if let Ok(meta) = fs::metadata(&target_file) {
            if meta.len() > 64 * 1024 {
                let token = format!("tidal_{}", safe_id);
                let _ = roo_streamer::register_direct_file_stream(&token, &target_file, "audio/flac");
                return Ok(Some(target_file.to_string_lossy().into_owned()));
            }
        }
    }

    if stream_url_or_info.is_none() && optional_info.is_none() {
        return Ok(None);
    }

    let info_ref = optional_info.as_ref().or(stream_url_or_info.as_ref());
    match ensure_tidal_track_cached(Some(&app), &state, &safe_id, info_ref).await {
        Ok(path) => Ok(Some(path.to_string_lossy().into_owned())),
        Err(e) => {
            tracing::warn!("[TIDAL] Cache track {} error: {}", safe_id, e);
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_track_id() {
        assert_eq!(clean_track_id(&serde_json::json!(12345)), "12345");
        assert_eq!(clean_track_id(&serde_json::json!("tidal_67890")), "67890");
        assert_eq!(clean_track_id(&serde_json::json!("999-abc")), "999-abc");
    }

    #[test]
    fn test_decode_base64_str() {
        let text = "Hello TIDAL";
        let encoded = "SGVsbG8gVElEQUw=";
        assert_eq!(decode_base64_str(encoded).unwrap(), text);
    }

    #[test]
    fn test_parse_bts_manifest() {
        let manifest = r#"{"mimeType":"audio/flac","urls":["https://sp-pr-cf.audio.tidal.com/1.flac"]}"#;
        let res = parse_tidal_manifest(manifest).expect("Should parse BTS");
        assert_eq!(res.init_url, None);
        assert_eq!(res.segment_urls.len(), 1);
        assert_eq!(res.segment_urls[0], "https://sp-pr-cf.audio.tidal.com/1.flac");
    }

    #[test]
    fn test_parse_dash_manifest() {
        let dash = r#"<MPD mediaPresentationDuration="PT30.000S">
            <Period>
                <AdaptationSet>
                    <Representation id="1">
                        <SegmentTemplate timescale="1000" initialization="https://example.com/init_$Number$.mp4" media="https://example.com/seg_$Number$.mp4" startNumber="1">
                            <SegmentTimeline>
                                <S d="10000" r="2"/>
                            </SegmentTimeline>
                        </SegmentTemplate>
                    </Representation>
                </AdaptationSet>
            </Period>
        </MPD>"#;
        let res = parse_tidal_manifest(dash).expect("Should parse DASH");
        assert_eq!(res.init_url, Some("https://example.com/init_1.mp4".to_string()));
        assert_eq!(res.segment_urls.len(), 3);
        assert_eq!(res.segment_urls[0], "https://example.com/seg_1.mp4");
        assert_eq!(res.segment_urls[1], "https://example.com/seg_2.mp4");
        assert_eq!(res.segment_urls[2], "https://example.com/seg_3.mp4");
    }

    #[tokio::test]
    async fn test_live_v2_search_with_stored_auth() {
        let Some(local_data) = dirs::data_local_dir() else { return };
        let dir = local_data.join("rooPlayer");
        let st = TidalState::new(dir);
        let auth = st.auth_data.read().unwrap().clone();
        let Some(auth_val) = auth else { return };
        let token = auth_val.get("accessToken").and_then(|v| v.as_str()).unwrap_or("");
        if token.is_empty() { return; }
        let t0 = Instant::now();
        let resp = st
            .http_client
            .get("https://api.tidal.com/v2/search?countryCode=TW&query=Audiophile+Hi-Res+Master&limit=5")
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .expect("reqwest send");
        let code = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        println!("LIVE_V2_SEARCH_STATUS={} ELAPSED_MS={} BODY_LEN={}", code, t0.elapsed().as_millis(), body.len());
        assert_eq!(code, 200, "Expected 200 OK from v2/search, got {}: {}", code, &body[..body.len().min(200)]);
    }
}
