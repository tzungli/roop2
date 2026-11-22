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
    headers.insert("transferMode.dlna.org", "Interactive".parse().unwrap());
    let dlna_pn = if mime.contains("png") {
        "PNG_MED"
    } else {
        "JPEG_MED"
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
        return StatusCode::NOT_FOUND.into_response();
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
                tracing::error!("DSP streaming generation failed for {}: {}", opts.file_path, e);
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
            let slice_len = end.saturating_sub(start) + 1;

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
            headers_mut.insert("transferMode.dlna.org", "Streaming".parse().unwrap());
            headers_mut.insert(
                "contentFeatures.dlna.org",
                "DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000".parse().unwrap(),
            );
            return resp;
        }
    }

    let mut resp = (StatusCode::OK, (*stream.data).clone()).into_response();
    let headers_mut = resp.headers_mut();
    headers_mut.insert(header::CONTENT_TYPE, stream.mime_type.parse().unwrap());
    headers_mut.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    headers_mut.insert(header::CONTENT_LENGTH, total_len.to_string().parse().unwrap());
    headers_mut.insert("transferMode.dlna.org", "Streaming".parse().unwrap());
    headers_mut.insert(
        "contentFeatures.dlna.org",
        "DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000".parse().unwrap(),
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dlna_image_response() {
        let resp = dlna_image_response(StatusCode::OK, "image/jpeg", vec![1, 2, 3]);
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

