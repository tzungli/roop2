use std::collections::HashMap;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct NativeHttpRequest {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct NativeHttpResponse {
    pub status: u16,
    pub ok: bool,
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[tauri::command]
pub async fn native_http_request(
    request: NativeHttpRequest,
) -> Result<NativeHttpResponse, String> {
    let timeout = std::time::Duration::from_millis(request.timeout_ms.unwrap_or(20000));
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())?;

    let method = match request.method.as_deref().unwrap_or("GET").to_uppercase().as_str() {
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        _ => reqwest::Method::GET,
    };

    let mut req_builder = client.request(method, &request.url);

    // Default User-Agent if not provided
    let mut has_ua = false;
    if let Some(headers) = request.headers {
        let mut header_map = HeaderMap::new();
        for (k, v) in headers {
            if k.eq_ignore_ascii_case("user-agent") {
                has_ua = true;
            }
            if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&v)) {
                header_map.insert(name, val);
            }
        }
        req_builder = req_builder.headers(header_map);
    }

    if !has_ua {
        req_builder = req_builder.header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 rooPlayer/0.8.4",
        );
    }

    if let Some(body) = request.body {
        req_builder = req_builder.body(body);
    }

    let resp = req_builder
        .send()
        .await
        .map_err(|e| format!("Native HTTP send failed: {}", e))?;

    let status = resp.status().as_u16();
    let ok = resp.status().is_success();

    let mut resp_headers = HashMap::new();
    for (name, val) in resp.headers() {
        if let Ok(v_str) = val.to_str() {
            resp_headers.insert(name.as_str().to_string(), v_str.to_string());
        }
    }

    let body = resp.text().await.map_err(|e| format!("Failed to read response body: {}", e))?;

    Ok(NativeHttpResponse {
        status,
        ok,
        headers: resp_headers,
        body,
    })
}
