use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower_http::cors::CorsLayer;
use crate::session::{
    generate_audio_stream, get_progressive_session, DspStreamOptions, GeneratedStream, ProgressiveSession,
};
use anyhow::Result;

pub static STREAM_CACHE: std::sync::LazyLock<RwLock<HashMap<String, Arc<GeneratedStream>>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

static COVER_TOKEN_REGISTRY: std::sync::LazyLock<RwLock<HashMap<String, (Option<String>, Option<String>)>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn register_cover_for_token(token: &str, cover_path: Option<String>, file_path: Option<String>) {
    let mut write = COVER_TOKEN_REGISTRY.write().unwrap();
    if write.len() > 200 {
        write.clear();
    }
    write.insert(token.to_string(), (cover_path, file_path));
}

pub fn prewarm_stream(opts: &DspStreamOptions) -> Result<Arc<GeneratedStream>> {
    let cache_key = opts.cache_key();

    {
        let read = STREAM_CACHE.read().unwrap();
        if let Some(s) = read.get(&cache_key) {
            return Ok(s.clone());
        }
    }

    let gen = generate_audio_stream(opts)?;
    let arc_s = Arc::new(gen);
    let mut write = STREAM_CACHE.write().unwrap();
    if write.len() > 6 {
        write.clear();
    }
    write.insert(cache_key, arc_s.clone());
    Ok(arc_s)
}

pub fn prewarm_stream_with_token(opts: &DspStreamOptions, token: &str) -> Result<Arc<GeneratedStream>> {
    let cache_key = opts.cache_key();

    {
        let read = STREAM_CACHE.read().unwrap();
        if let Some(s) = read.get(&cache_key) {
            let mut write = STREAM_CACHE.write().unwrap();
            write.insert(token.to_string(), s.clone());
            return Ok(s.clone());
        }
    }

    let gen = generate_audio_stream(opts)?;
    let arc_s = Arc::new(gen);
    let mut write = STREAM_CACHE.write().unwrap();
    if write.len() > 6 {
        write.clear();
    }
    write.insert(cache_key, arc_s.clone());
    write.insert(token.to_string(), arc_s.clone());
    Ok(arc_s)
}

pub struct StreamServerState;

pub struct StreamServer {
    port: u16,
    state: Arc<StreamServerState>,
}

impl StreamServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            state: Arc::new(StreamServerState),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run(&self) -> Result<()> {
        let app = Router::new()
            .route("/stream/:token/:filename", get(handle_stream_token))
            .route("/stream/:token", get(handle_stream_token_simple))
            .route("/cover/:token/:filename", get(handle_cover_token))
            .route("/cover/:token", get(handle_cover_token_simple))
            .route("/sox-stream", get(handle_stream))
            .route("/dsd-stream", get(handle_dsd_stream))
            .route("/cover", get(handle_cover))
            .layer(CorsLayer::permissive())
            .with_state(self.state.clone());

        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("Audio stream server listening on http://{}", addr);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

const FALLBACK_JPEG: &[u8] = &[
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x01, 0x00, 0x48,
    0x00, 0x48, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08,
    0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0a, 0x0c, 0x14, 0x0d, 0x0c, 0x0b, 0x0b, 0x0c, 0x19, 0x12,
    0x13, 0x0f, 0x14, 0x1d, 0x1a, 0x1f, 0x1e, 0x1d, 0x1a, 0x1c, 0x1c, 0x20, 0x24, 0x2e, 0x27, 0x20,
    0x22, 0x2c, 0x23, 0x1c, 0x1c, 0x28, 0x37, 0x29, 0x2c, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1f, 0x27,
    0x39, 0x3d, 0x38, 0x32, 0x3c, 0x2e, 0x33, 0x34, 0x32, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x01,
    0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00, 0x1f, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04,
    0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f,
    0x00, 0xbf, 0x80, 0xff, 0xd9,
];

fn dlna_image_response(status: StatusCode, mime: &str, data: Vec<u8>) -> Response {
    let mut resp = (status, data).into_response();
    let headers = resp.headers_mut();
    if let Ok(hv) = mime.parse() {
        headers.insert(header::CONTENT_TYPE, hv);
    } else {
        headers.insert(header::CONTENT_TYPE, "image/jpeg".parse().unwrap());
    }
    headers.insert(header::CACHE_CONTROL, "public, max-age=2592000, immutable".parse().unwrap());
    headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    headers.insert("transferMode.dlna.org", "Interactive".parse().unwrap());
    let dlna_pn = if mime.contains("png") {
        "PNG_LRG"
    } else {
        "JPEG_LRG"
    };
    let dlna_features = format!("DLNA.ORG_PN={};DLNA.ORG_OP=00;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=00900000000000000000000000000000", dlna_pn);
    if let Ok(val) = dlna_features.parse() {
        headers.insert("contentFeatures.dlna.org", val);
    }
    resp
}

#[derive(serde::Deserialize)]
struct CoverQuery {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    size: Option<u32>,
}

async fn handle_cover(Query(q): Query<CoverQuery>) -> Response {
    resolve_cover_response(q).await
}

async fn handle_cover_token(
    axum::extract::Path((token, _filename)): axum::extract::Path<(String, String)>,
) -> Response {
    serve_cover_for_token(&token).await
}

async fn handle_cover_token_simple(
    axum::extract::Path(token): axum::extract::Path<String>,
) -> Response {
    serve_cover_for_token(&token).await
}

async fn serve_cover_for_token(token: &str) -> Response {
    let pair = {
        let read = COVER_TOKEN_REGISTRY.read().unwrap();
        read.get(token).cloned()
    };
    let (cover_path, file_path) = pair.unwrap_or((None, None));
    let q = CoverQuery {
        path: cover_path,
        url: None,
        file_path,
        size: Some(360),
    };
    resolve_cover_response(q).await
}

async fn resolve_cover_response(q: CoverQuery) -> Response {
    let target_size = q.size.unwrap_or(360);

    // 1. Remote HTTP/HTTPS URL proxy for DLNA devices
    let remote_url = q.url.as_deref().or_else(|| {
        q.path.as_deref().filter(|p| p.starts_with("http://") || p.starts_with("https://"))
    });

    if let Some(url) = remote_url {
        if let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build() {
            if let Ok(resp) = client.get(url).send().await {
                if resp.status().is_success() {
                    let ctype = resp.headers().get(header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("image/jpeg")
                        .to_string();
                    if let Ok(bytes) = resp.bytes().await {
                        return dlna_image_response(StatusCode::OK, &ctype, bytes.to_vec());
                    }
                }
            }
        }
    }

    // 2. Resolve local file path
    let mut resolved_image_path = None;

    if let Some(raw_path) = q.path.as_deref() {
        let clean = raw_path
            .trim_start_matches("media://")
            .trim_start_matches("file:///")
            .trim_start_matches("file://");
        let p = std::path::Path::new(clean);
        if p.exists() && p.is_file() {
            resolved_image_path = Some(p.to_path_buf());
        }
    }

    // 3. Fallback: check track folder or embedded picture in audio file
    if resolved_image_path.is_none() {
        if let Some(track_path_str) = q.file_path.as_deref() {
            let tp = std::path::Path::new(track_path_str);
            if let Some(parent) = tp.parent() {
                let candidates = ["cover.jpg", "folder.jpg", "front.jpg", "Cover.jpg", "Folder.jpg", "cover.png", "folder.png"];
                for c in candidates {
                    let cand_path = parent.join(c);
                    if cand_path.exists() && cand_path.is_file() {
                        resolved_image_path = Some(cand_path);
                        break;
                    }
                }
            }

            // Embedded picture fallback via lofty
            if resolved_image_path.is_none() && tp.exists() && tp.is_file() {
                use lofty::file::TaggedFileExt;
                if let Ok(tagged_file) = lofty::probe::Probe::open(tp).and_then(|pr| pr.read().map_err(Into::into)) {
                    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());
                    if let Some(t) = tag {
                        if let Some(pic) = t.pictures().first() {
                            let data = pic.data().to_vec();
                            return dlna_image_response(StatusCode::OK, "image/jpeg", data);
                        }
                    }
                }
            }
        }
    }

    let Some(p) = resolved_image_path else {
        return dlna_image_response(StatusCode::OK, "image/jpeg", FALLBACK_JPEG.to_vec());
    };

    // If size == 0, serve original file directly
    if target_size == 0 {
        if let Ok(data) = std::fs::read(&p) {
            let p_lower = p.to_string_lossy().to_lowercase();
            let mime = if p_lower.ends_with(".png") {
                "image/png"
            } else if p_lower.ends_with(".webp") {
                "image/webp"
            } else {
                "image/jpeg"
            };
            return dlna_image_response(StatusCode::OK, mime, data);
        }
        return dlna_image_response(StatusCode::OK, "image/jpeg", FALLBACK_JPEG.to_vec());
    }

    // Check if thumbnail already exists on disk (e.g. {stem}_thumb360.jpg)
    let parent = p.parent().unwrap_or(&p);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("thumb");
    let thumb_name = format!("{}_thumb{}.jpg", stem, target_size);
    let thumb_path = parent.join(thumb_name);

    if thumb_path.exists() {
        if let Ok(data) = std::fs::read(&thumb_path) {
            return dlna_image_response(StatusCode::OK, "image/jpeg", data);
        }
    }

    // Read original
    let Ok(orig_data) = std::fs::read(&p) else {
        return dlna_image_response(StatusCode::OK, "image/jpeg", FALLBACK_JPEG.to_vec());
    };

    // If original file is already very small (< 40 KB), serve directly
    if orig_data.len() < 40 * 1024 {
        return dlna_image_response(StatusCode::OK, "image/jpeg", orig_data);
    }

    // Downscale using image crate
    if let Ok(img) = image::load_from_memory(&orig_data) {
        let (w, h) = (img.width(), img.height());
        if w <= target_size && h <= target_size {
            return dlna_image_response(StatusCode::OK, "image/jpeg", orig_data);
        }

        let thumb = img.thumbnail(target_size, target_size);
        let mut out = Vec::new();
        if thumb.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg).is_ok() {
            let _ = std::fs::write(&thumb_path, &out);
            return dlna_image_response(StatusCode::OK, "image/jpeg", out);
        }
    }

    // Fallback: serve original
    dlna_image_response(StatusCode::OK, "image/jpeg", orig_data)
}

async fn handle_stream_token(
    axum::extract::Path((token, _filename)): axum::extract::Path<(String, String)>,
    method: axum::http::Method,
    headers: HeaderMap,
    State(_state): State<Arc<StreamServerState>>,
) -> Response {
    handle_stream_for_token(&token, method, headers).await
}

async fn handle_stream_token_simple(
    axum::extract::Path(token): axum::extract::Path<String>,
    method: axum::http::Method,
    headers: HeaderMap,
    State(_state): State<Arc<StreamServerState>>,
) -> Response {
    handle_stream_for_token(&token, method, headers).await
}

async fn handle_stream_for_token(token: &str, method: axum::http::Method, headers: HeaderMap) -> Response {
    if let Some(session) = get_progressive_session(token) {
        return serve_progressive_stream(&session, method, &headers).await;
    }

    let stream = {
        let read = STREAM_CACHE.read().unwrap();
        read.get(token).cloned()
    };
    match stream {
        Some(s) => serve_stream_data(&s, &headers),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn serve_progressive_stream(
    session: &Arc<ProgressiveSession>,
    method: axum::http::Method,
    headers: &HeaderMap,
) -> Response {
    let total_len = session.total_bytes;
    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let is_dsd = session.mime_type.contains("dsf") || session.mime_type.contains("dsd");
    let mime_type = if is_dsd {
        let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("");
        if accept.contains("audio/dsf") && !accept.contains("audio/x-dsf") {
            "audio/dsf"
        } else {
            "audio/x-dsf"
        }
    } else {
        session.mime_type
    };
    let dlna_features = "DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000";

    if method == axum::http::Method::HEAD {
        let mut resp = StatusCode::OK.into_response();
        let headers_mut = resp.headers_mut();
        headers_mut.insert(header::CONTENT_TYPE, mime_type.parse().unwrap());
        headers_mut.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
        headers_mut.insert(header::CONTENT_LENGTH, total_len.to_string().parse().unwrap());
        headers_mut.insert("transferMode.dlna.org", "Streaming".parse().unwrap());
        headers_mut.insert("contentFeatures.dlna.org", dlna_features.parse().unwrap());
        headers_mut.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
        return resp;
    }

    let (status, start, end) = if let Some(range_str) = range_header {
        if let Some((s, e)) = parse_range_header(range_str, total_len) {
            (StatusCode::PARTIAL_CONTENT, s, e)
        } else {
            return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
        }
    } else {
        (StatusCode::OK, 0, total_len.saturating_sub(1))
    };

    let content_len = if end >= start { end - start + 1 } else { 0 };

    struct ReaderState {
        file_path: std::path::PathBuf,
        current_pos: u64,
        end_pos: u64,
        total_len: u64,
        bytes_written: Arc<std::sync::atomic::AtomicU64>,
        is_completed: Arc<std::sync::atomic::AtomicBool>,
        is_cancelled: Arc<std::sync::atomic::AtomicBool>,
        notify: Arc<tokio::sync::Notify>,
    }

    let initial_state = ReaderState {
        file_path: session.temp_file_path.clone(),
        current_pos: start,
        end_pos: end,
        total_len,
        bytes_written: session.bytes_written.clone(),
        is_completed: session.is_completed.clone(),
        is_cancelled: session.is_cancelled.clone(),
        notify: session.notify.clone(),
    };

    let stream = futures_util::stream::unfold(initial_state, |mut state| async move {
        if state.current_pos > state.end_pos || state.is_cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }

        let is_tail_probe = state.end_pos >= state.total_len.saturating_sub(256 * 1024)
            && state.current_pos >= state.total_len.saturating_sub(256 * 1024);

        if !is_tail_probe {
            loop {
                if state.is_cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    return None;
                }
                let written = state.bytes_written.load(std::sync::atomic::Ordering::Acquire);
                if state.current_pos < written {
                    break;
                }
                if state.is_completed.load(std::sync::atomic::Ordering::Acquire) {
                    if state.current_pos >= written {
                        return None;
                    }
                    break;
                }
                let _ = tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    state.notify.notified(),
                ).await;
            }
        }

        let written = state.bytes_written.load(std::sync::atomic::Ordering::Acquire);
        let available = if is_tail_probe {
            state.end_pos + 1 - state.current_pos
        } else {
            written.saturating_sub(state.current_pos)
        };
        let to_read = (state.end_pos + 1 - state.current_pos).min(available).min(128 * 1024) as usize;

        if to_read == 0 {
            return None;
        }

        let curr_pos = state.current_pos;
        let p = state.file_path.clone();

        let read_res = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
            use std::io::{Read, Seek, SeekFrom};
            let mut file = std::fs::File::open(&p)?;
            file.seek(SeekFrom::Start(curr_pos))?;
            let mut buf = vec![0u8; to_read];
            file.read_exact(&mut buf)?;
            Ok(buf)
        }).await;

        match read_res {
            Ok(Ok(bytes)) => {
                let bytes_len = bytes.len() as u64;
                state.current_pos += bytes_len;
                Some((Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(bytes)), state))
            }
            Ok(Err(e)) => {
                tracing::warn!("Progressive stream read error at pos {}: {}", curr_pos, e);
                None
            }
            Err(e) => {
                tracing::warn!("Progressive stream spawn_blocking error at pos {}: {}", curr_pos, e);
                None
            }
        }
    });

    let body = axum::body::Body::from_stream(stream);
    let mut resp = (status, body).into_response();
    let headers_mut = resp.headers_mut();
    headers_mut.insert(header::CONTENT_TYPE, mime_type.parse().unwrap());
    headers_mut.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    headers_mut.insert(header::CONTENT_LENGTH, content_len.to_string().parse().unwrap());
    if status == StatusCode::PARTIAL_CONTENT {
        headers_mut.insert(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, total_len).parse().unwrap(),
        );
    }
    headers_mut.insert("transferMode.dlna.org", "Streaming".parse().unwrap());
    headers_mut.insert("contentFeatures.dlna.org", dlna_features.parse().unwrap());
    headers_mut.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    resp
}

async fn handle_dsd_stream(
    Query(mut opts): Query<DspStreamOptions>,
    headers: HeaderMap,
    State(state): State<Arc<StreamServerState>>,
) -> Response {
    if opts.dsd_mode.is_none() {
        opts.dsd_mode = Some("dsd64".to_string());
    }
    if opts.dop.is_none() {
        opts.dop = Some(false);
    }
    handle_stream(Query(opts), headers, State(state)).await
}

async fn handle_stream(
    Query(opts): Query<DspStreamOptions>,
    headers: HeaderMap,
    State(_state): State<Arc<StreamServerState>>,
) -> Response {
    let cache_key = opts.cache_key();

    let stream = {
        let read_cache = STREAM_CACHE.read().unwrap();
        read_cache.get(&cache_key).cloned()
    };

    let stream = match stream {
        Some(s) => s,
        None => {
            let opts_clone = opts.clone();
            let gen_res = tokio::task::spawn_blocking(move || generate_audio_stream(&opts_clone))
                .await
                .map_err(|e| anyhow::anyhow!("DSP worker join error: {}", e))
                .and_then(|r| r);
            match gen_res {
                Ok(gen) => {
                    let arc_s = Arc::new(gen);
                    let mut write_cache = STREAM_CACHE.write().unwrap();
                    if write_cache.len() > 6 {
                        write_cache.clear();
                    }
                    write_cache.insert(cache_key, arc_s.clone());
                    arc_s
                }
                Err(e) => {
                    tracing::error!("DSP streaming generation failed for {}: {}", opts.file_path, e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, format!("DSP processing error: {}", e)).into_response();
                }
            }
        }
    };

    serve_stream_data(&stream, &headers)
}

fn serve_stream_data(stream: &GeneratedStream, headers: &HeaderMap) -> Response {
    let total_len = stream.total_bytes;
    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());

    let is_dsd = stream.mime_type.contains("dsf") || stream.mime_type.contains("dsd");
    let mime_type = if is_dsd {
        let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("");
        if accept.contains("audio/dsf") && !accept.contains("audio/x-dsf") {
            "audio/dsf"
        } else {
            "audio/x-dsf"
        }
    } else {
        stream.mime_type
    };

    let dlna_features = "DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000";

    if let Some(range_str) = range_header {
        if let Some(range) = parse_range_header(range_str, total_len) {
            let start = range.0 as usize;
            let end = range.1 as usize;
            let slice_len = end.saturating_sub(start) + 1;

            if start >= stream.data.len() || end >= stream.data.len() || start > end {
                return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            }

            let slice = stream.data[start..=end].to_vec();

            let mut resp = (StatusCode::PARTIAL_CONTENT, slice).into_response();
            let headers_mut = resp.headers_mut();
            headers_mut.insert(header::CONTENT_TYPE, mime_type.parse().unwrap());
            headers_mut.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
            headers_mut.insert(
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", start, end, total_len).parse().unwrap(),
            );
            headers_mut.insert(header::CONTENT_LENGTH, slice_len.to_string().parse().unwrap());
            headers_mut.insert("transferMode.dlna.org", "Streaming".parse().unwrap());
            headers_mut.insert(
                "contentFeatures.dlna.org",
                dlna_features.parse().unwrap(),
            );
            headers_mut.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
            return resp;
        }
    }

    let mut resp = (StatusCode::OK, (*stream.data).clone()).into_response();
    let headers_mut = resp.headers_mut();
    headers_mut.insert(header::CONTENT_TYPE, mime_type.parse().unwrap());
    headers_mut.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    headers_mut.insert(header::CONTENT_LENGTH, total_len.to_string().parse().unwrap());
    headers_mut.insert("transferMode.dlna.org", "Streaming".parse().unwrap());
    headers_mut.insert(
        "contentFeatures.dlna.org",
        dlna_features.parse().unwrap(),
    );
    headers_mut.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    resp
}

fn parse_range_header(s: &str, total: u64) -> Option<(u64, u64)> {
    if !s.starts_with("bytes=") {
        return None;
    }
    let s = s[6..].trim();
    if let Some(suffix) = s.strip_prefix('-') {
        let n: u64 = suffix.trim().parse().ok()?;
        let start = total.saturating_sub(n);
        let end = total.saturating_sub(1);
        return Some((start, end));
    }
    let parts: Vec<&str> = s.split('-').collect();
    if parts.is_empty() {
        return None;
    }

    let start = parts[0].trim().parse::<u64>().ok()?;
    let end = if parts.len() > 1 && !parts[1].trim().is_empty() {
        parts[1].trim().parse::<u64>().ok()?.min(total.saturating_sub(1))
    } else {
        total.saturating_sub(1)
    };

    if start > end || start >= total {
        return None;
    }

    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dlna_image_response() {
        let resp = dlna_image_response(StatusCode::OK, "image/jpeg", vec![1, 2, 3]);
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

