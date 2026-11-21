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
use crate::session::{generate_audio_stream, DspStreamOptions, GeneratedStream};
use anyhow::Result;

pub struct StreamServerState {
    cache: RwLock<HashMap<String, Arc<GeneratedStream>>>,
}

pub struct StreamServer {
    port: u16,
    state: Arc<StreamServerState>,
}

impl StreamServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            state: Arc::new(StreamServerState {
                cache: RwLock::new(HashMap::new()),
            }),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run(&self) -> Result<()> {
        let app = Router::new()
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

#[derive(serde::Deserialize)]
struct CoverQuery {
    path: String,
    #[serde(default)]
    size: Option<u32>,
}

async fn handle_cover(Query(q): Query<CoverQuery>) -> Response {
    let p = std::path::Path::new(&q.path);
    if !p.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let target_size = q.size.unwrap_or(360);

    // If size == 0, serve original file directly
    if target_size == 0 {
        if let Ok(data) = std::fs::read(p) {
            let p_lower = q.path.to_lowercase();
            let mime = if p_lower.ends_with(".png") {
                "image/png"
            } else if p_lower.ends_with(".webp") {
                "image/webp"
            } else {
                "image/jpeg"
            };
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, "public, max-age=2592000, immutable"),
                ],
                data,
            ).into_response();
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    // Check if thumbnail already exists on disk (e.g. {stem}_thumb360.jpg)
    let parent = p.parent().unwrap_or(p);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("thumb");
    let thumb_name = format!("{}_thumb{}.jpg", stem, target_size);
    let thumb_path = parent.join(thumb_name);

    if thumb_path.exists() {
        if let Ok(data) = std::fs::read(&thumb_path) {
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/jpeg"),
                    (header::CACHE_CONTROL, "public, max-age=2592000, immutable"),
                ],
                data,
            ).into_response();
        }
    }

    // Read original
    let Ok(orig_data) = std::fs::read(p) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // If original file is already very small (< 40 KB), serve directly
    if orig_data.len() < 40 * 1024 {
        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=2592000, immutable"),
            ],
            orig_data,
        ).into_response();
    }

    // Downscale using image crate
    if let Ok(img) = image::load_from_memory(&orig_data) {
        let (w, h) = (img.width(), img.height());
        if w <= target_size && h <= target_size {
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/jpeg"),
                    (header::CACHE_CONTROL, "public, max-age=2592000, immutable"),
                ],
                orig_data,
            ).into_response();
        }

        let thumb = img.thumbnail(target_size, target_size);
        let mut out = Vec::new();
        if thumb.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg).is_ok() {
            let _ = std::fs::write(&thumb_path, &out);
            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "image/jpeg"),
                    (header::CACHE_CONTROL, "public, max-age=2592000, immutable"),
                ],
                out,
            ).into_response();
        }
    }

    // Fallback: serve original
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "public, max-age=2592000, immutable"),
        ],
        orig_data,
    ).into_response()
}

async fn handle_dsd_stream(
    Query(mut opts): Query<DspStreamOptions>,
    headers: HeaderMap,
    State(state): State<Arc<StreamServerState>>,
) -> Response {
    if opts.dsd_mode.is_none() {
        opts.dsd_mode = Some("dsd64".to_string());
    }
    handle_stream(Query(opts), headers, State(state)).await
}

async fn handle_stream(
    Query(opts): Query<DspStreamOptions>,
    headers: HeaderMap,
    State(state): State<Arc<StreamServerState>>,
) -> Response {
    let cache_key = format!(
        "{}:{}:{}:{}:{}:{:?}:{:?}",
        opts.file_path, opts.start_sec, opts.target_rate, opts.dsd_mode.as_deref().unwrap_or(""),
        opts.filter_phase, opts.peq_json, opts.conv_json
    );

    let stream = {
        let read_cache = state.cache.read().unwrap();
        read_cache.get(&cache_key).cloned()
    };

    let stream = match stream {
        Some(s) => s,
        None => match generate_audio_stream(&opts) {
            Ok(gen) => {
                let arc_s = Arc::new(gen);
                let mut write_cache = state.cache.write().unwrap();
                if !write_cache.is_empty() {
                    write_cache.clear();
                }
                write_cache.insert(cache_key, arc_s.clone());
                arc_s
            }
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("DSP processing error: {}", e)).into_response();
            }
        },
    };

    let total_len = stream.total_bytes;
    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());

    if let Some(range_str) = range_header {
        if let Some(range) = parse_range_header(range_str, total_len) {
            let start = range.0 as usize;
            let end = range.1 as usize;
            let slice_len = end - start + 1;

            if start >= stream.data.len() || end >= stream.data.len() || start > end {
                return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            }

            let slice = stream.data[start..=end].to_vec();

            let mut resp = (StatusCode::PARTIAL_CONTENT, slice).into_response();
            let headers_mut = resp.headers_mut();
            headers_mut.insert(header::CONTENT_TYPE, stream.mime_type.parse().unwrap());
            headers_mut.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
            headers_mut.insert(
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", start, end, total_len).parse().unwrap(),
            );
            headers_mut.insert(header::CONTENT_LENGTH, slice_len.to_string().parse().unwrap());
            return resp;
        }
    }

    let mut resp = (StatusCode::OK, (*stream.data).clone()).into_response();
    let headers_mut = resp.headers_mut();
    headers_mut.insert(header::CONTENT_TYPE, stream.mime_type.parse().unwrap());
    headers_mut.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    headers_mut.insert(header::CONTENT_LENGTH, total_len.to_string().parse().unwrap());
    resp
}

fn parse_range_header(s: &str, total: u64) -> Option<(u64, u64)> {
    if !s.starts_with("bytes=") {
        return None;
    }
    let parts: Vec<&str> = s[6..].split('-').collect();
    if parts.is_empty() {
        return None;
    }

    let start = parts[0].parse::<u64>().ok()?;
    let end = if parts.len() > 1 && !parts[1].is_empty() {
        parts[1].parse::<u64>().ok()?.min(total.saturating_sub(1))
    } else {
        total.saturating_sub(1)
    };

    Some((start, end))
}
