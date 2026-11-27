use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use crate::commands::http::shared_http_client;

const KKBOX_CLIENT_ID: &str = "e23a2fc7882033ef9cb60286769deada";
const KKBOX_CLIENT_SECRET: &str = "06ee6dfd7c6f0052022076f921ecd765";
const KKBOX_WIDGET_FALLBACK_TOKEN: &str = "cZymbQ8NVO59NbJg-xs0rw==";

#[derive(Default)]
struct KkboxAuthCache {
    access_token: Option<String>,
    expires_at: Option<Instant>,
    widget_token: Option<String>,
    widget_expires_at: Option<Instant>,
}

static KKBOX_CACHE: LazyLock<RwLock<KkboxAuthCache>> = LazyLock::new(|| RwLock::new(KkboxAuthCache::default()));

async fn get_kkbox_access_token() -> Result<String, String> {
    {
        let r = KKBOX_CACHE.read().await;
        if let (Some(ref tok), Some(exp)) = (&r.access_token, r.expires_at) {
            if Instant::now() + Duration::from_secs(60) < exp {
                return Ok(tok.clone());
            }
        }
    }

    let client = shared_http_client();
    let form_body = format!(
        "grant_type=client_credentials&client_id={}&client_secret={}",
        urlencoding::encode(KKBOX_CLIENT_ID),
        urlencoding::encode(KKBOX_CLIENT_SECRET)
    );
    let resp = client
        .post("https://account.kkbox.com/oauth2/token")
        .timeout(Duration::from_secs(10))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body)
        .send()
        .await
        .map_err(|e| format!("KKBOX auth error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("KKBOX Auth HTTP {}", resp.status().as_u16()));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let tok = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "KKBOX Auth missing access_token".to_string())?
        .to_string();
    let expires_in = json.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(2_592_000);

    let mut w = KKBOX_CACHE.write().await;
    w.access_token = Some(tok.clone());
    w.expires_at = Some(Instant::now() + Duration::from_secs(expires_in));
    Ok(tok)
}

#[tauri::command]
pub async fn kkbox_api(endpoint: String, _territory: Option<String>) -> Result<serde_json::Value, String> {
    let token = get_kkbox_access_token().await?;
    let url = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint
    } else {
        format!("https://api.kkbox.com/v1.1{}", endpoint)
    };
    let resp = shared_http_client()
        .get(&url)
        .timeout(Duration::from_secs(12))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("KKBOX API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("KKBOX API HTTP {}", resp.status().as_u16()));
    }
    resp.json::<serde_json::Value>().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn kkbox_get_stream_url(track_id: String, territory: Option<String>) -> Result<String, String> {
    let terr = territory.unwrap_or_else(|| "TW".to_string());
    let client = shared_http_client();

    let mut widget_tok = {
        let r = KKBOX_CACHE.read().await;
        match (&r.widget_token, r.widget_expires_at) {
            (Some(ref t), Some(exp)) if Instant::now() < exp => Some(t.clone()),
            _ => None,
        }
    };

    if widget_tok.is_none() {
        let widget_url = format!(
            "https://widget.kkbox.com/v1/?id={}&type=song&terr={}",
            urlencoding::encode(&track_id),
            urlencoding::encode(&terr)
        );
        if let Ok(w_resp) = client.get(&widget_url).timeout(Duration::from_secs(8)).send().await {
            if let Ok(html) = w_resp.text().await {
                if let Some(pos) = html.find("&quot;accessToken&quot;:&quot;") {
                    let after = &html[pos + "&quot;accessToken&quot;:&quot;".len()..];
                    if let Some(end) = after.find("&quot;") {
                        let tok = after[..end].to_string();
                        if !tok.is_empty() {
                            let mut w = KKBOX_CACHE.write().await;
                            w.widget_token = Some(tok.clone());
                            w.widget_expires_at = Some(Instant::now() + Duration::from_secs(3600));
                            widget_tok = Some(tok);
                        }
                    }
                }
            }
        }
    }

    let token = widget_tok.unwrap_or_else(|| KKBOX_WIDGET_FALLBACK_TOKEN.to_string());
    let payload = serde_json::json!({
        "track_id": track_id,
        "access_token": token,
        "territory": terr,
    });

    let p_resp = client
        .post("https://api-web.kkbox.com.tw/v2/track-previews")
        .timeout(Duration::from_secs(10))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("KKBOX stream request failed: {}", e))?;

    if !p_resp.status().is_success() {
        return Err(format!("KKBOX Audio HTTP {}", p_resp.status().as_u16()));
    }

    let data: serde_json::Value = p_resp.json().await.map_err(|e| e.to_string())?;
    data.get("preview_url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "未回傳音訊串流網址".to_string())
}
