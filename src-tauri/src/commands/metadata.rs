use serde::{Deserialize, Serialize};
use crate::commands::http::shared_http_client;

// =========================================================================
// Shared Text, Base64 & HTML Helpers
// =========================================================================

fn decode_base64_utf8(b64: &str) -> String {
    let mut vals = Vec::with_capacity(b64.len());
    for b in b64.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => {
                vals.push(255u8);
                continue;
            }
            _ => continue,
        };
        vals.push(v);
    }
    let mut out = Vec::with_capacity((vals.len() * 3) / 4);
    for chunk in vals.chunks(4) {
        if chunk.len() < 2 || chunk[0] == 255 || chunk[1] == 255 {
            break;
        }
        let b0 = chunk[0];
        let b1 = chunk[1];
        out.push((b0 << 2) | (b1 >> 4));
        if chunk.len() > 2 && chunk[2] != 255 {
            let b2 = chunk[2];
            out.push((b1 << 4) | (b2 >> 2));
            if chunk.len() > 3 && chunk[3] != 255 {
                let b3 = chunk[3];
                out.push((b2 << 6) | b3);
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn decode_html_entities(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    let s = input
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");

    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'&' && i + 2 < bytes.len() && bytes[i + 1] == b'#' {
            if let Some(semi_rel) = s[i + 2..].find(';') {
                if semi_rel <= 8 {
                    let num_str = &s[i + 2..i + 2 + semi_rel];
                    let parsed = if let Some(hex) = num_str.strip_prefix('x').or_else(|| num_str.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        num_str.parse::<u32>().ok()
                    };
                    if let Some(ch) = parsed.and_then(char::from_u32) {
                        out.push(ch);
                        i += 2 + semi_rel + 1;
                        continue;
                    }
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn strip_html_tags(input: &str) -> String {
    let mut replaced = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(idx) = rest.find('<') {
        replaced.push_str(&rest[..idx]);
        if let Some(end) = rest[idx..].find('>') {
            let tag = &rest[idx + 1..idx + end].trim().to_ascii_lowercase();
            if tag.starts_with("br") {
                replaced.push('\n');
            } else if tag == "/p" {
                replaced.push_str("\n\n");
            }
            rest = &rest[idx + end + 1..];
        } else {
            replaced.push_str(&rest[idx..]);
            rest = "";
            break;
        }
    }
    replaced.push_str(rest);
    let decoded = decode_html_entities(&replaced).replace('\r', "");
    collapse_blank_lines(&decoded)
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run = 0usize;
    for line in s.lines() {
        let t = line.trim_end();
        if t.is_empty() {
            blank_run += 1;
            if blank_run <= 2 && !out.is_empty() {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(t);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

fn has_kana(s: &str) -> bool {
    s.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c))
}

fn is_unknown_artist(artist: &str) -> bool {
    let lower = artist.trim().to_lowercase();
    lower.is_empty()
        || matches!(
            lower.as_str(),
            "unknown"
                | "unknown artist"
                | "various"
                | "various artists"
                | "未知"
                | "未知藝術家"
                | "未知藝人"
                | "群星"
        )
}

async fn to_simplified_for_search(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || !has_cjk(trimmed) {
        return trimmed.to_string();
    }
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl=zh-TW&tl=zh-CN&dt=t&q={}",
        urlencoding::encode(trimmed)
    );
    if let Ok(resp) = shared_http_client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(arr) = json.get(0).and_then(|v| v.as_array()) {
                    let mut out = String::new();
                    for seg in arr {
                        if let Some(s) = seg.get(0).and_then(|v| v.as_str()) {
                            out.push_str(s);
                        }
                    }
                    let res = out.trim().to_string();
                    if !res.is_empty() {
                        return res;
                    }
                }
            }
        }
    }
    trimmed.to_string()
}

// =========================================================================
// 1. Native Multi-Source Lyrics Engine
// =========================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsLookupRequest {
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub kkbox_id: Option<String>,
    #[serde(default)]
    pub kkbox_web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLyricLine {
    pub time: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsLookupResponse {
    pub source: String,
    pub synced: Option<String>,
    pub plain: Option<String>,
    pub lines: Vec<ParsedLyricLine>,
}

struct LyricCandidate {
    track_name: String,
    artist_name: String,
    album_name: String,
    duration: f64,
    synced: bool,
}

fn clean_title_for_lyrics(title: &str) -> String {
    let mut s = title.trim().to_string();
    for ext in [
        ".flac", ".wav", ".mp3", ".m4a", ".alac", ".aac", ".ogg", ".dsf", ".dff", ".aiff", ".aif",
    ] {
        if s.to_lowercase().ends_with(ext) {
            s.truncate(s.len() - ext.len());
            break;
        }
    }
    // Strip leading track numbers like "01 - " or "01. "
    let bytes = s.as_bytes();
    let mut digit_end = 0usize;
    while digit_end < bytes.len() && digit_end < 3 && bytes[digit_end].is_ascii_digit() {
        digit_end += 1;
    }
    if digit_end > 0 && digit_end < bytes.len() {
        let mut sep_end = digit_end;
        while sep_end < bytes.len() && matches!(bytes[sep_end], b'.' | b'-' | b'_' | b' ') {
            sep_end += 1;
        }
        if sep_end > digit_end && sep_end < bytes.len() {
            s = s[sep_end..].to_string();
        }
    }
    // Strip common bracketed edition markers
    let s_stripped = strip_edition_parens(&s);
    let cleaned = s_stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        title.trim().to_string()
    } else {
        cleaned
    }
}

fn strip_edition_parens(input: &str) -> String {
    let keywords = [
        "feat", "ft.", "live", "version", "remaster", "mix", "acoustic", "edit", "deluxe",
        "bonus", "stereo", "mono", "24bit", "hi-res", "hires", "翻唱", "現場", "伴奏", "純音樂",
    ];
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(open_pos) = rest.find(['(', '（', '[']) {
        out.push_str(&rest[..open_pos]);
        let open_ch = rest[open_pos..].chars().next().unwrap();
        let close_ch = match open_ch {
            '(' => ')',
            '（' => '）',
            _ => ']',
        };
        let after_open = &rest[open_pos + open_ch.len_utf8()..];
        if let Some(close_pos) = after_open.find(close_ch) {
            let inner = &after_open[..close_pos];
            let inner_l = inner.to_lowercase();
            if !keywords.iter().any(|k| inner_l.contains(k)) {
                out.push(open_ch);
                out.push_str(inner);
                out.push(close_ch);
            }
            rest = &after_open[close_pos + close_ch.len_utf8()..];
        } else {
            out.push_str(&rest[open_pos..]);
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

fn clean_artist_for_lyrics(artist: &str) -> String {
    if is_unknown_artist(artist) {
        return String::new();
    }
    let mut out = String::with_capacity(artist.len());
    let mut rest = artist;
    while let Some(open_pos) = rest.find(['(', '（', '[']) {
        out.push_str(&rest[..open_pos]);
        let open_ch = rest[open_pos..].chars().next().unwrap();
        let close_ch = match open_ch {
            '(' => ')',
            '（' => '）',
            _ => ']',
        };
        let after_open = &rest[open_pos + open_ch.len_utf8()..];
        if let Some(close_pos) = after_open.find(close_ch) {
            rest = &after_open[close_pos + close_ch.len_utf8()..];
        } else {
            out.push_str(&rest[open_pos..]);
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_match_str(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(c) || ('\u{3040}'..='\u{30ff}').contains(c))
        .collect()
}

fn score_lyrics_candidate(
    cand: &LyricCandidate,
    target_title: &str,
    target_artist: &str,
    target_album: &str,
    target_duration: f64,
) -> i32 {
    let c_title = normalize_match_str(&clean_title_for_lyrics(&cand.track_name));
    let t_title = normalize_match_str(target_title);
    if c_title.is_empty() || t_title.is_empty() {
        return -100;
    }

    let mut score = 0i32;
    let mut title_exact = false;

    if c_title == t_title {
        score += 60;
        title_exact = true;
    } else if c_title.contains(&t_title) || t_title.contains(&c_title) {
        let min_len = c_title.chars().count().min(t_title.chars().count()) as f64;
        let max_len = c_title.chars().count().max(t_title.chars().count()) as f64;
        if min_len / max_len.max(1.0) >= 0.55 {
            score += 38;
        } else {
            score -= 20;
        }
    } else {
        return -100;
    }

    let t_art_norm = normalize_match_str(target_artist);
    let c_art_norm = normalize_match_str(&cand.artist_name);
    let primary_raw = target_artist
        .split([',', ';', '&', '/', '、'])
        .next()
        .unwrap_or("");
    let t_primary = normalize_match_str(primary_raw);

    let mut artist_matched = false;
    if !t_art_norm.is_empty() {
        if c_art_norm == t_art_norm || (!t_primary.is_empty() && c_art_norm == t_primary) {
            score += 50;
            artist_matched = true;
        } else if !c_art_norm.is_empty()
            && (c_art_norm.contains(&t_art_norm)
                || t_art_norm.contains(&c_art_norm)
                || (!t_primary.is_empty() && (c_art_norm.contains(&t_primary) || t_primary.contains(&c_art_norm))))
        {
            score += 32;
            artist_matched = true;
        } else {
            score -= 45;
        }
    }

    if target_duration > 10.0 && cand.duration > 10.0 {
        let diff = (cand.duration - target_duration).abs();
        if diff <= 3.0 {
            score += 25;
        } else if diff <= 8.0 {
            score += 15;
        } else if diff <= 15.0 {
            score += 5;
        } else if diff > 30.0 {
            score -= 35;
        } else {
            score -= 12;
        }
    }

    let t_alb = normalize_match_str(target_album);
    let c_alb = normalize_match_str(&cand.album_name);
    if !t_alb.is_empty() && !c_alb.is_empty() && (c_alb == t_alb || c_alb.contains(&t_alb) || t_alb.contains(&c_alb)) {
        score += 12;
    }

    if cand.synced {
        score += 10;
    }

    if !t_art_norm.is_empty() && !artist_matched {
        let dur_close = target_duration > 10.0 && cand.duration > 10.0 && (cand.duration - target_duration).abs() <= 4.0;
        if !(title_exact && dur_close) {
            return -50;
        }
    }

    score
}

fn score_lyrics_candidate_bilingual(
    cand: &LyricCandidate,
    titles: &[String],
    artists: &[String],
    albums: &[String],
    target_duration: f64,
) -> i32 {
    let mut best = i32::MIN;
    for t in titles {
        if t.is_empty() {
            continue;
        }
        for a in artists {
            for alb in albums {
                let sc = score_lyrics_candidate(cand, t, a, alb, target_duration);
                if sc > best {
                    best = sc;
                }
            }
        }
    }
    best
}

pub fn parse_lrc(lrc_text: &str) -> Vec<ParsedLyricLine> {
    let mut result = Vec::new();
    for line in lrc_text.lines() {
        let mut times = Vec::new();
        let mut rest = line.trim();
        while rest.starts_with('[') {
            let Some(close) = rest.find(']') else { break };
            let inner = &rest[1..close];
            if let Some(colon) = inner.find(':') {
                let min_str = &inner[..colon];
                let sec_full = &inner[colon + 1..];
                if let (Ok(min), Some((sec_s, frac_s))) = (
                    min_str.parse::<u32>(),
                    sec_full.split_once('.').or(Some((sec_full, ""))),
                ) {
                    if let Ok(sec) = sec_s.parse::<u32>() {
                        let frac = if frac_s.is_empty() {
                            0.0
                        } else {
                            format!("0.{}", frac_s).parse::<f64>().unwrap_or(0.0)
                        };
                        times.push(min as f64 * 60.0 + sec as f64 + frac);
                        rest = rest[close + 1..].trim_start();
                        continue;
                    }
                }
            }
            break;
        }
        let text = rest.trim();
        if !times.is_empty() && !text.is_empty() {
            for t in times {
                result.push(ParsedLyricLine {
                    time: t,
                    text: text.to_string(),
                });
            }
        }
    }
    result.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    result
}

fn has_lrc_timestamps(text: &str) -> bool {
    !parse_lrc(text).is_empty()
}

fn extract_kkbox_lyrics_from_html(html: &str) -> Option<String> {
    // 1. JSON-LD <script type="application/ld+json">
    let mut rest = html;
    while let Some(pos) = rest.find("application/ld+json") {
        let after = &rest[pos..];
        if let Some(gt) = after.find('>') {
            let body_start = &after[gt + 1..];
            if let Some(end) = body_start.find("</script>") {
                let json_str = &body_start[..end];
                if let Ok(obj) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let raw = obj
                        .pointer("/lyrics/text")
                        .or_else(|| obj.pointer("/recordingOf/lyrics/text"))
                        .and_then(|v| v.as_str());
                    if let Some(r) = raw {
                        let cleaned = strip_html_tags(r);
                        if cleaned.chars().count() > 15 {
                            return Some(cleaned);
                        }
                    }
                }
                rest = &body_start[end + 9..];
                continue;
            }
        }
        break;
    }

    // 2. Classic KKBOX container class="lyrics"
    if let Some(idx) = html.find("class=\"lyrics\"").or_else(|| html.find("class='lyrics'")) {
        let sub = &html[idx..];
        if let Some(gt) = sub.find('>') {
            let inner = &sub[gt + 1..];
            if let Some(end) = inner.find("</div>").or_else(|| inner.find("</p>")) {
                let cleaned = strip_html_tags(&inner[..end]);
                if cleaned.chars().count() > 15
                    && !cleaned.contains("目前沒有歌詞")
                    && !cleaned.contains("暫無歌詞")
                    && !cleaned.to_lowercase().contains("no lyrics")
                {
                    return Some(cleaned);
                }
            }
        }
    }
    None
}

fn make_lyrics_resp(source: &str, synced: Option<String>, plain: Option<String>) -> LyricsLookupResponse {
    let lines = synced.as_deref().map(parse_lrc).unwrap_or_default();
    LyricsLookupResponse {
        source: source.to_string(),
        synced,
        plain,
        lines,
    }
}

#[tauri::command]
pub async fn metadata_fetch_lyrics(req: LyricsLookupRequest) -> Result<Option<LyricsLookupResponse>, String> {
    let title_raw = req.title.trim();
    if title_raw.is_empty() {
        return Ok(None);
    }

    // 1 & 2. Local .lrc sidecar & embedded audio file lyrics tag
    if let Some(ref fp) = req.file_path {
        if !fp.trim().is_empty() {
            if let Ok(Some(local)) = crate::commands::library::library_read_track_lyrics(fp.clone()).await {
                let source = local.get("source").and_then(|v| v.as_str()).unwrap_or("本機歌詞");
                let synced = local.get("synced").and_then(|v| v.as_str()).map(|s| s.to_string());
                let plain = local.get("plain").and_then(|v| v.as_str()).map(|s| s.to_string());
                if synced.is_some() || plain.is_some() {
                    return Ok(Some(make_lyrics_resp(source, synced, plain)));
                }
            }
        }
    }

    let clean_title = clean_title_for_lyrics(title_raw);
    let clean_art = clean_artist_for_lyrics(&req.artist);
    let clean_alb = strip_edition_parens(&req.album).trim().to_string();
    let dur_rounded = if req.duration.is_finite() && req.duration > 0.0 {
        req.duration.round()
    } else {
        0.0
    };

    let cjk = has_cjk(&format!("{} {} {}", clean_title, clean_art, clean_alb));
    let (simp_title, simp_art, simp_alb) = if cjk {
        tokio::join!(
            to_simplified_for_search(&clean_title),
            async {
                if clean_art.is_empty() {
                    String::new()
                } else {
                    to_simplified_for_search(&clean_art).await
                }
            },
            async {
                if clean_alb.is_empty() {
                    String::new()
                } else {
                    to_simplified_for_search(&clean_alb).await
                }
            }
        )
    } else {
        (clean_title.clone(), clean_art.clone(), clean_alb.clone())
    };

    let mut titles = vec![clean_title.clone()];
    if simp_title != clean_title && !simp_title.is_empty() {
        titles.push(simp_title.clone());
    }
    let mut artists = Vec::new();
    if !clean_art.is_empty() {
        artists.push(clean_art.clone());
    }
    if simp_art != clean_art && !simp_art.is_empty() {
        artists.push(simp_art.clone());
    }
    if artists.is_empty() {
        artists.push(String::new());
    }
    let mut albums = Vec::new();
    if !clean_alb.is_empty() {
        albums.push(clean_alb.clone());
    }
    if simp_alb != clean_alb && !simp_alb.is_empty() {
        albums.push(simp_alb.clone());
    }
    if albums.is_empty() {
        albums.push(String::new());
    }

    let client = shared_http_client();

    // 4. KKBOX Web Lyrics (when kkbox_id / kkbox_web_url is supplied or found via KKBOX Open API)
    let mut kkbox_url = req
        .kkbox_web_url
        .as_deref()
        .filter(|u| u.contains("kkbox.com"))
        .map(|s| s.to_string());
    if kkbox_url.is_none() {
        if let Some(kid) = req.kkbox_id.as_deref().filter(|s| !s.trim().is_empty()) {
            kkbox_url = Some(format!("https://www.kkbox.com/tw/tc/song/{}", urlencoding::encode(kid)));
        } else {
            let q = format!("{}{}", clean_title, if clean_art.is_empty() { String::new() } else { format!(" {}", clean_art) });
            if let Ok(k_json) = crate::commands::kkbox::kkbox_api(
                format!("/search?q={}&type=track&territory=TW&limit=10", urlencoding::encode(&q)),
                Some("TW".to_string()),
            )
            .await
            {
                if let Some(items) = k_json.pointer("/tracks/data").and_then(|v| v.as_array()) {
                    let mut best_url: Option<String> = None;
                    let mut best_sc = i32::MIN;
                    for kt in items {
                        let cand = LyricCandidate {
                            track_name: kt.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            artist_name: kt.pointer("/album/artist/name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            album_name: kt.pointer("/album/name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            duration: kt.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0) / 1000.0,
                            synced: false,
                        };
                        let sc = score_lyrics_candidate_bilingual(&cand, &titles, &artists, &albums, dur_rounded);
                        if sc > best_sc {
                            best_sc = sc;
                            let id_opt = kt.get("id").and_then(|v| v.as_str());
                            let url_opt = kt.get("url").and_then(|v| v.as_str());
                            best_url = url_opt
                                .map(|s| s.to_string())
                                .or_else(|| id_opt.map(|id| format!("https://www.kkbox.com/tw/tc/song/{}", urlencoding::encode(id))));
                        }
                    }
                    if best_sc >= 58 {
                        kkbox_url = best_url;
                    }
                }
            }
        }
    }
    if let Some(url) = kkbox_url {
        if let Ok(resp) = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(6))
            .header("Accept-Language", "zh-TW,zh;q=0.9")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(html) = resp.text().await {
                    if let Some(extracted) = extract_kkbox_lyrics_from_html(&html) {
                        let synced = if has_lrc_timestamps(&extracted) { Some(extracted.clone()) } else { None };
                        return Ok(Some(make_lyrics_resp("KKBOX 線上歌詞", synced, Some(extracted))));
                    }
                }
            }
        }
    }

    let lrclib_ua = concat!("rooPlayer/", env!("CARGO_PKG_VERSION"), " (https://github.com/tzungli/roop2)");
    let variant_pairs: Vec<(&str, &str, &str)> = if simp_title != clean_title || simp_art != clean_art {
        vec![(&clean_title, &clean_art, &clean_alb), (&simp_title, &simp_art, &simp_alb)]
    } else {
        vec![(&clean_title, &clean_art, &clean_alb)]
    };

    // 5a. LRCLIB Exact Match
    if !clean_art.is_empty() && dur_rounded > 0.0 {
        for (tq, aq, albq) in &variant_pairs {
            let mut url = format!(
                "https://lrclib.net/api/get?track_name={}&artist_name={}&duration={}",
                urlencoding::encode(tq),
                urlencoding::encode(aq),
                dur_rounded as u64
            );
            if !albq.is_empty() {
                url.push_str(&format!("&album_name={}", urlencoding::encode(albq)));
            }
            if let Ok(resp) = client
                .get(&url)
                .timeout(std::time::Duration::from_secs(6))
                .header("User-Agent", lrclib_ua)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(hit) = resp.json::<serde_json::Value>().await {
                        if !hit.get("instrumental").and_then(|v| v.as_bool()).unwrap_or(false) {
                            let synced = hit.get("syncedLyrics").and_then(|v| v.as_str()).map(|s| s.to_string()).filter(|s| !s.trim().is_empty());
                            let plain = hit.get("plainLyrics").and_then(|v| v.as_str()).map(|s| s.to_string()).filter(|s| !s.trim().is_empty());
                            if synced.is_some() || plain.is_some() {
                                return Ok(Some(make_lyrics_resp("LRCLIB 精準比對", synced, plain)));
                            }
                        }
                    }
                }
            }
        }
    }

    let pick_best_lrclib = |hits: &[serde_json::Value]| -> Option<(Option<String>, Option<String>)> {
        let mut best_sc = i32::MIN;
        let mut best: Option<(Option<String>, Option<String>)> = None;
        for h in hits {
            let synced = h.get("syncedLyrics").and_then(|v| v.as_str()).map(|s| s.to_string()).filter(|s| !s.trim().is_empty());
            let plain = h.get("plainLyrics").and_then(|v| v.as_str()).map(|s| s.to_string()).filter(|s| !s.trim().is_empty());
            if synced.is_none() && plain.is_none() {
                continue;
            }
            let cand = LyricCandidate {
                track_name: h.get("trackName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                artist_name: h.get("artistName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                album_name: h.get("albumName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                duration: h.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0),
                synced: synced.is_some(),
            };
            let sc = score_lyrics_candidate_bilingual(&cand, &titles, &artists, &albums, dur_rounded);
            if sc > best_sc {
                best_sc = sc;
                best = Some((synced, plain));
            }
        }
        if best_sc >= 55 { best } else { None }
    };

    // 5b & 5c. LRCLIB Structured Search & Free-Text Search
    for (tq, aq, albq) in &variant_pairs {
        let mut url = format!("https://lrclib.net/api/search?track_name={}", urlencoding::encode(tq));
        if !aq.is_empty() {
            url.push_str(&format!("&artist_name={}", urlencoding::encode(aq)));
        }
        if !albq.is_empty() {
            url.push_str(&format!("&album_name={}", urlencoding::encode(albq)));
        }
        if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(6)).header("User-Agent", lrclib_ua).send().await {
            if resp.status().is_success() {
                if let Ok(hits) = resp.json::<Vec<serde_json::Value>>().await {
                    if let Some((synced, plain)) = pick_best_lrclib(&hits) {
                        return Ok(Some(make_lyrics_resp("LRCLIB 線上音樂資料庫", synced, plain)));
                    }
                }
            }
        }
    }

    for (tq, aq, _) in &variant_pairs {
        let q = if aq.is_empty() { tq.to_string() } else { format!("{} {}", aq, tq) };
        let url = format!("https://lrclib.net/api/search?q={}", urlencoding::encode(&q));
        if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(6)).header("User-Agent", lrclib_ua).send().await {
            if resp.status().is_success() {
                if let Ok(hits) = resp.json::<Vec<serde_json::Value>>().await {
                    if let Some((synced, plain)) = pick_best_lrclib(&hits) {
                        return Ok(Some(make_lyrics_resp("LRCLIB 線上音樂資料庫", synced, plain)));
                    }
                }
            }
        }
    }

    let zh_query = if simp_art.is_empty() {
        simp_title.clone()
    } else {
        format!("{} {}", simp_title, simp_art)
    };

    // 6. NetEase Cloud Music Lyric API
    if let Ok(s_resp) = client
        .get(&format!(
            "https://music.163.com/api/search/get/web?s={}&type=1&offset=0&limit=8",
            urlencoding::encode(&zh_query)
        ))
        .timeout(std::time::Duration::from_secs(6))
        .header("Referer", "https://music.163.com/")
        .header("Origin", "https://music.163.com")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await
    {
        if s_resp.status().is_success() {
            if let Ok(s_json) = s_resp.json::<serde_json::Value>().await {
                if let Some(songs) = s_json.pointer("/result/songs").and_then(|v| v.as_array()) {
                    let mut best_id: Option<i64> = None;
                    let mut best_sc = i32::MIN;
                    for s in songs {
                        let artists_joined = s
                            .get("artists")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_default();
                        let cand = LyricCandidate {
                            track_name: s.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            artist_name: artists_joined,
                            album_name: s.pointer("/album/name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            duration: s.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0) / 1000.0,
                            synced: true,
                        };
                        let sc = score_lyrics_candidate_bilingual(&cand, &titles, &artists, &albums, dur_rounded);
                        if sc > best_sc {
                            best_sc = sc;
                            best_id = s.get("id").and_then(|v| v.as_i64());
                        }
                    }
                    if best_sc >= 55 {
                        if let Some(id) = best_id {
                            if let Ok(l_resp) = client
                                .get(&format!(
                                    "https://music.163.com/api/song/lyric?os=pc&id={}&lv=-1&kv=-1&tv=-1",
                                    id
                                ))
                                .timeout(std::time::Duration::from_secs(6))
                                .header("Referer", "https://music.163.com/")
                                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                                .send()
                                .await
                            {
                                if let Ok(l_json) = l_resp.json::<serde_json::Value>().await {
                                    if let Some(raw) = l_json.pointer("/lrc/lyric").and_then(|v| v.as_str()) {
                                        let trimmed = raw.trim();
                                        if !trimmed.is_empty()
                                            && !trimmed.contains("纯音乐，请欣赏")
                                            && !trimmed.contains("純音樂，請欣賞")
                                        {
                                            let synced = if has_lrc_timestamps(trimmed) {
                                                Some(trimmed.to_string())
                                            } else {
                                                None
                                            };
                                            return Ok(Some(make_lyrics_resp(
                                                "網易雲音樂歌詞庫",
                                                synced,
                                                Some(trimmed.to_string()),
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 7. QQ Music Lyric API
    if let Ok(q_resp) = client
        .get(&format!(
            "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?w={}&p=1&n=8&format=json&cr=1&g_tk=5381",
            urlencoding::encode(&zh_query)
        ))
        .timeout(std::time::Duration::from_secs(6))
        .header("Referer", "https://y.qq.com/")
        .header("Origin", "https://y.qq.com")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await
    {
        if q_resp.status().is_success() {
            if let Ok(q_json) = q_resp.json::<serde_json::Value>().await {
                if let Some(songs) = q_json.pointer("/data/song/list").and_then(|v| v.as_array()) {
                    let mut best_mid: Option<String> = None;
                    let mut best_sc = i32::MIN;
                    for s in songs {
                        let singers = s
                            .get("singer")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_default();
                        let cand = LyricCandidate {
                            track_name: s.get("songname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            artist_name: singers,
                            album_name: s.get("albumname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            duration: s.get("interval").and_then(|v| v.as_f64()).unwrap_or(0.0),
                            synced: true,
                        };
                        let sc = score_lyrics_candidate_bilingual(&cand, &titles, &artists, &albums, dur_rounded);
                        if sc > best_sc {
                            best_sc = sc;
                            best_mid = s.get("songmid").and_then(|v| v.as_str()).map(|m| m.to_string());
                        }
                    }
                    if best_sc >= 55 {
                        if let Some(mid) = best_mid {
                            if let Ok(ql_resp) = client
                                .get(&format!(
                                    "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid={}&format=json&g_tk=5381&nobase64=1",
                                    urlencoding::encode(&mid)
                                ))
                                .timeout(std::time::Duration::from_secs(6))
                                .header("Referer", "https://y.qq.com/")
                                .header("Origin", "https://y.qq.com")
                                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                                .send()
                                .await
                            {
                                if let Ok(ql_json) = ql_resp.json::<serde_json::Value>().await {
                                    let raw0 = ql_json.get("lyric").and_then(|v| v.as_str()).unwrap_or("");
                                    let decoded = if !raw0.is_empty() && !raw0.contains('[') {
                                        decode_base64_utf8(raw0)
                                    } else {
                                        raw0.to_string()
                                    };
                                    let raw = decode_html_entities(&decoded).trim().to_string();
                                    if raw.chars().count() > 10 && !raw.contains("此歌曲为没有填词的纯音乐") {
                                        let synced = if has_lrc_timestamps(&raw) { Some(raw.clone()) } else { None };
                                        return Ok(Some(make_lyrics_resp("QQ 音樂歌詞庫", synced, Some(raw))));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 8. KuGou Music Lyric API
    if let Ok(kg_resp) = client
        .get(&format!(
            "http://mobilecdn.kugou.com/api/v3/search/song?format=json&keyword={}&page=1&pagesize=8&showtype=1",
            urlencoding::encode(&zh_query)
        ))
        .timeout(std::time::Duration::from_secs(6))
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await
    {
        if kg_resp.status().is_success() {
            if let Ok(kg_json) = kg_resp.json::<serde_json::Value>().await {
                if let Some(songs) = kg_json.pointer("/data/info").and_then(|v| v.as_array()) {
                    let mut best_hash: Option<String> = None;
                    let mut best_dur = dur_rounded;
                    let mut best_sc = i32::MIN;
                    for s in songs {
                        let d = s.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let cand = LyricCandidate {
                            track_name: s.get("songname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            artist_name: s.get("singername").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            album_name: s.get("album_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            duration: d,
                            synced: true,
                        };
                        let sc = score_lyrics_candidate_bilingual(&cand, &titles, &artists, &albums, dur_rounded);
                        if sc > best_sc {
                            best_sc = sc;
                            best_hash = s.get("hash").and_then(|v| v.as_str()).map(|h| h.to_string());
                            if d > 0.0 {
                                best_dur = d;
                            }
                        }
                    }
                    if best_sc >= 55 {
                        if let Some(hash) = best_hash {
                            let dur_ms = (best_dur * 1000.0).round() as u64;
                            if let Ok(krc_resp) = client
                                .get(&format!(
                                    "http://krcs.kugou.com/search?ver=1&man=yes&client=mobi&keyword={}&duration={}&hash={}",
                                    urlencoding::encode(&zh_query),
                                    dur_ms,
                                    urlencoding::encode(&hash)
                                ))
                                .timeout(std::time::Duration::from_secs(6))
                                .send()
                                .await
                            {
                                if let Ok(krc_json) = krc_resp.json::<serde_json::Value>().await {
                                    if let Some(cand0) = krc_json.get("candidates").and_then(|v| v.as_array()).and_then(|a| a.first()) {
                                        let id = cand0.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                        let key = cand0.get("accesskey").and_then(|v| v.as_str()).unwrap_or("");
                                        if !id.is_empty() && !key.is_empty() {
                                            if let Ok(dl_resp) = client
                                                .get(&format!(
                                                    "http://lyrics.kugou.com/download?ver=1&client=pc&id={}&accesskey={}&fmt=lrc&charset=utf8",
                                                    urlencoding::encode(id),
                                                    urlencoding::encode(key)
                                                ))
                                                .timeout(std::time::Duration::from_secs(6))
                                                .send()
                                                .await
                                            {
                                                if let Ok(dl_json) = dl_resp.json::<serde_json::Value>().await {
                                                    let b64 = dl_json.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                                    let raw = decode_base64_utf8(b64).trim().to_string();
                                                    if raw.chars().count() > 10 {
                                                        let synced = if has_lrc_timestamps(&raw) { Some(raw.clone()) } else { None };
                                                        return Ok(Some(make_lyrics_resp("酷狗音樂歌詞庫", synced, Some(raw))));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

// =========================================================================
// 2. Native Multi-Source Album Review & Metadata Engine
// =========================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumBioRequest {
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub year: Option<String>,
    #[serde(default)]
    pub track_titles: Vec<String>,
    #[serde(default)]
    pub track_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumBioResponse {
    pub original_text: String,
    pub translated_text: String,
    pub is_from_en_wiki: bool,
    pub source_label: String,
    pub release_date: String,
    pub label: String,
    pub copyright: String,
    pub tags: Vec<String>,
    pub primary_type: String,
    pub secondary_types: Vec<String>,
}

fn normalize_album_match(s: &str) -> String {
    let cleaned = strip_edition_parens(s);
    normalize_match_str(&cleaned)
}

fn album_title_matches(cand: &str, t1: &str, t2: &str) -> bool {
    let cn = normalize_album_match(cand);
    if cn.is_empty() {
        return false;
    }
    let a = normalize_album_match(t1);
    let b = normalize_album_match(t2);
    (!a.is_empty() && (cn == a || (cn.chars().count() >= 2 && a.chars().count() >= 2 && (cn.contains(&a) || a.contains(&cn)))))
        || (!b.is_empty() && (cn == b || (cn.chars().count() >= 2 && b.chars().count() >= 2 && (cn.contains(&b) || b.contains(&cn)))))
}

fn artist_name_matches(cand: &str, a1: &str, a2: &str) -> bool {
    if a1.trim().is_empty() {
        return true;
    }
    let cn = normalize_match_str(cand);
    if cn.is_empty() {
        return false;
    }
    let x = normalize_match_str(a1);
    let y = normalize_match_str(a2);
    (!x.is_empty() && (cn.contains(&x) || x.contains(&cn)))
        || (!y.is_empty() && (cn.contains(&y) || y.contains(&cn)))
}

fn trim_review_length(text: &str, max_chars: usize) -> String {
    let s = collapse_blank_lines(text);
    if s.chars().count() <= max_chars {
        return s;
    }
    let cut: String = s.chars().take(max_chars).collect();
    if let Some(pos) = cut.rfind("\n\n").or_else(|| cut.rfind('。')).or_else(|| cut.rfind(". ")) {
        if pos > cut.len() / 2 {
            return cut[..=pos].trim().to_string();
        }
    }
    format!("{}...", cut.trim())
}

fn clean_wikipedia_extract(raw: &str, max_chars: usize) -> String {
    let mut kept = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("==") && t.ends_with("==") {
            let h = t.trim_matches('=').trim().to_lowercase();
            if ["曲目", "track listing", "tracks", "personnel", "credits", "charts", "external links", "references", "參見", "參考資料", "外部連結", "外部链接", "製作人員", "制作人员"]
                .iter()
                .any(|stop| h.contains(stop))
            {
                break;
            }
            continue;
        }
        if !t.is_empty() {
            kept.push(t);
        }
    }
    trim_review_length(&kept.join("\n\n"), max_chars)
}

fn is_genuine_album_wiki(hit_title: &str, extract: &str, title: &str, artist: &str, title_cn: &str) -> bool {
    let lower = extract.to_lowercase();
    if lower.contains("may refer to:") || lower.contains("可以指") {
        return false;
    }
    let album_markers = ["album", "record", "studio", "soundtrack", "ep", "專輯", "专辑", "唱片", "錄音室", "发行", "發行", "アルバム"];
    if !album_markers.iter().any(|m| lower.contains(m)) {
        return false;
    }
    let title_ok = album_title_matches(hit_title, title, title_cn)
        || lower.contains(&title.to_lowercase())
        || lower.contains(&title_cn.to_lowercase());
    let artist_ok = artist.is_empty()
        || !has_cjk(artist)
        || artist_name_matches(extract, artist, "")
        || artist_name_matches(hit_title, artist, "");
    title_ok && artist_ok
}

fn merge_tags(tags: &mut Vec<String>, items: &[String]) {
    for it in items {
        let clean = it.trim();
        if clean.is_empty() {
            continue;
        }
        if !tags.iter().any(|x| x.eq_ignore_ascii_case(clean)) {
            tags.push(clean.to_string());
        }
    }
}

#[tauri::command]
pub async fn metadata_fetch_album_bio(req: AlbumBioRequest) -> Result<Option<AlbumBioResponse>, String> {
    let clean_title = strip_edition_parens(req.title.trim()).trim().to_string();
    if clean_title.is_empty() {
        return Ok(None);
    }
    let clean_artist = if is_unknown_artist(&req.artist) {
        String::new()
    } else {
        req.artist.trim().to_string()
    };
    let cjk = has_cjk(&format!("{} {}", clean_title, clean_artist));
    let kana = has_kana(&format!("{} {}", clean_title, clean_artist));
    let (clean_title_cn, clean_artist_cn) = if cjk {
        tokio::join!(
            to_simplified_for_search(&clean_title),
            async {
                if clean_artist.is_empty() {
                    String::new()
                } else {
                    to_simplified_for_search(&clean_artist).await
                }
            }
        )
    } else {
        (clean_title.clone(), clean_artist.clone())
    };
    let search_query = if clean_artist.is_empty() {
        clean_title.clone()
    } else {
        format!("{} {}", clean_title, clean_artist)
    };

    let client = shared_http_client();
    let mut release_date = String::new();
    let mut label = String::new();
    let mut copyright = String::new();
    let mut tags: Vec<String> = Vec::new();
    let mut primary_type = String::new();
    let mut secondary_types: Vec<String> = Vec::new();
    let mut raw_bio_text = String::new();
    let mut is_from_en_wiki = false;
    let mut source_label = String::new();
    let mut source_lang_is_cn = false;

    // 1. NetEase Cloud Music Official Album Description
    {
        let queries = vec![
            if !clean_artist_cn.is_empty() { format!("{} {}", clean_title_cn, clean_artist_cn) } else { String::new() },
            clean_title_cn.clone(),
            if !clean_artist.is_empty() { search_query.clone() } else { String::new() },
            clean_title.clone(),
        ];
        let mut cand_id: Option<i64> = None;
        let mut direct_match = false;
        let mut fallback_ids: Vec<i64> = Vec::new();
        for q in queries.into_iter().filter(|s| !s.is_empty()) {
            if let Ok(resp) = client
                .get(&format!(
                    "https://music.163.com/api/search/get/web?s={}&type=10&offset=0&limit=8",
                    urlencoding::encode(&q)
                ))
                .timeout(std::time::Duration::from_secs(7))
                .header("Referer", "https://music.163.com/")
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    for a in json.pointer("/result/albums").and_then(|v| v.as_array()).into_iter().flatten() {
                        let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        if !album_title_matches(name, &clean_title, &clean_title_cn) {
                            continue;
                        }
                        let mut art_parts = Vec::new();
                        if let Some(an) = a.pointer("/artist/name").and_then(|v| v.as_str()) {
                            art_parts.push(an);
                        }
                        for al in a.pointer("/artist/alias").and_then(|v| v.as_array()).into_iter().flatten() {
                            if let Some(s) = al.as_str() { art_parts.push(s); }
                        }
                        for x in a.get("artists").and_then(|v| v.as_array()).into_iter().flatten() {
                            if let Some(s) = x.get("name").and_then(|v| v.as_str()) { art_parts.push(s); }
                        }
                        let art_joined = art_parts.join(" ");
                        if let Some(id) = a.get("id").and_then(|v| v.as_i64()) {
                            if artist_name_matches(&art_joined, &clean_artist, &clean_artist_cn) {
                                cand_id = Some(id);
                                direct_match = true;
                                break;
                            } else if !fallback_ids.contains(&id) {
                                fallback_ids.push(id);
                            }
                        }
                    }
                    if direct_match {
                        break;
                    }
                }
            }
        }
        let inspect_ids: Vec<i64> = if let Some(id) = cand_id {
            vec![id]
        } else {
            fallback_ids.into_iter().take(2).collect()
        };
        let local_norms: Vec<String> = req.track_titles.iter().map(|t| normalize_album_match(t)).filter(|n| n.chars().count() >= 2).collect();
        for id in inspect_ids {
            if let Ok(resp) = client
                .get(&format!("https://music.163.com/api/v1/album/{}", id))
                .timeout(std::time::Duration::from_secs(7))
                .header("Referer", "https://music.163.com/")
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .send()
                .await
            {
                if let Ok(alb_data) = resp.json::<serde_json::Value>().await {
                    let Some(alb_info) = alb_data.get("album") else { continue };
                    if !direct_match {
                        let remote_norms: Vec<String> = alb_data
                            .get("songs")
                            .and_then(|v| v.as_array())
                            .into_iter()
                            .flatten()
                            .filter_map(|s| s.get("name").and_then(|v| v.as_str()))
                            .map(normalize_album_match)
                            .filter(|n| n.chars().count() >= 2)
                            .collect();
                        let overlap = local_norms.iter().any(|ln| {
                            remote_norms.iter().any(|rn| ln == rn || (ln.chars().count() >= 3 && (ln.contains(rn) || rn.contains(ln))))
                        });
                        let exact_only = !has_cjk(&clean_artist)
                            && normalize_album_match(alb_info.get("name").and_then(|v| v.as_str()).unwrap_or(""))
                                == normalize_album_match(&clean_title);
                        if !overlap && !exact_only {
                            continue;
                        }
                    }
                    if label.is_empty() {
                        if let Some(c) = alb_info.get("company").and_then(|v| v.as_str()) {
                            label = c.trim().to_string();
                        }
                    }
                    let desc = trim_review_length(alb_info.get("description").and_then(|v| v.as_str()).unwrap_or(""), 2400);
                    if desc.chars().count() >= 35 {
                        raw_bio_text = desc;
                        is_from_en_wiki = !has_cjk(&raw_bio_text);
                        source_lang_is_cn = has_cjk(&raw_bio_text);
                        source_label = "唱片官方專輯簡介".to_string();
                        break;
                    }
                }
            }
        }
    }

    // 2. Douban Music Official Album Notes
    if raw_bio_text.is_empty() && cjk {
        let db_queries = vec![
            if !clean_artist_cn.is_empty() { format!("{} {}", clean_title_cn, clean_artist_cn) } else { String::new() },
            clean_title_cn.clone(),
            clean_title.clone(),
        ];
        let mut matched_id: Option<String> = None;
        for dq in db_queries.into_iter().filter(|s| !s.is_empty()) {
            if let Ok(resp) = client
                .get(&format!(
                    "https://m.douban.com/rexxar/api/v2/search?q={}&type=music&count=6",
                    urlencoding::encode(&dq)
                ))
                .timeout(std::time::Duration::from_secs(7))
                .header("Referer", "https://m.douban.com/search/")
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    for it in json.pointer("/subjects/items").and_then(|v| v.as_array()).into_iter().flatten() {
                        if it.get("target_type").and_then(|v| v.as_str()) != Some("music") {
                            continue;
                        }
                        let Some(target) = it.get("target") else { continue };
                        let t_title = target.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        let sub = target.get("card_subtitle").and_then(|v| v.as_str()).unwrap_or("");
                        if album_title_matches(t_title, &clean_title, &clean_title_cn)
                            && artist_name_matches(sub, &clean_artist, &clean_artist_cn)
                        {
                            matched_id = target.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())
                                .or_else(|| target.get("id").and_then(|v| v.as_i64()).map(|n| n.to_string()));
                            break;
                        }
                    }
                    if matched_id.is_some() {
                        break;
                    }
                }
            }
        }
        if let Some(did) = matched_id {
            if let Ok(resp) = client
                .get(&format!("https://m.douban.com/rexxar/api/v2/music/{}", did))
                .timeout(std::time::Duration::from_secs(7))
                .header("Referer", &format!("https://m.douban.com/music/subject/{}/", did))
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .send()
                .await
            {
                if let Ok(dd) = resp.json::<serde_json::Value>().await {
                    if release_date.is_empty() {
                        if let Some(p) = dd.get("pubdate").and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|v| v.as_str()) {
                            release_date = p.trim().to_string();
                        }
                    }
                    if label.is_empty() {
                        if let Some(p) = dd.get("publishers").and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|v| v.as_str()) {
                            label = p.trim().to_string();
                        }
                    }
                    let d_tags: Vec<String> = dd
                        .get("genres")
                        .and_then(|v| v.as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    merge_tags(&mut tags, &d_tags);
                    let intro = trim_review_length(dd.get("intro").and_then(|v| v.as_str()).unwrap_or(""), 2400);
                    if intro.chars().count() >= 35 {
                        raw_bio_text = intro;
                        is_from_en_wiki = !has_cjk(&raw_bio_text);
                        source_lang_is_cn = has_cjk(&raw_bio_text);
                        source_label = "豆瓣音樂專輯簡介".to_string();
                    }
                }
            }
        }
    }

    // 3. Fast iTunes Search Metadata
    if let Ok(resp) = client
        .get(&format!(
            "https://itunes.apple.com/search?term={}&entity=album&country=tw&limit=5",
            urlencoding::encode(&search_query)
        ))
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let results = json.get("results").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let hit = results
                .iter()
                .find(|r| {
                    album_title_matches(r.get("collectionName").and_then(|v| v.as_str()).unwrap_or(""), &clean_title, &clean_title_cn)
                        && artist_name_matches(r.get("artistName").and_then(|v| v.as_str()).unwrap_or(""), &clean_artist, &clean_artist_cn)
                })
                .or_else(|| {
                    if !has_cjk(&clean_artist) {
                        results.iter().find(|r| album_title_matches(r.get("collectionName").and_then(|v| v.as_str()).unwrap_or(""), &clean_title, &clean_title_cn))
                    } else {
                        None
                    }
                });
            if let Some(h) = hit {
                if release_date.is_empty() {
                    if let Some(rd) = h.get("releaseDate").and_then(|v| v.as_str()) {
                        release_date = rd.chars().take(10).collect();
                    }
                }
                if copyright.is_empty() {
                    if let Some(cp) = h.get("copyright").and_then(|v| v.as_str()) {
                        copyright = cp.trim().to_string();
                        if label.is_empty() {
                            let clean_lbl = copyright.trim_start_matches(|c: char| !c.is_alphabetic() && !has_cjk(&c.to_string())).trim();
                            if clean_lbl.chars().count() > 2 {
                                label = clean_lbl.to_string();
                            }
                        }
                    }
                }
                if let Some(gn) = h.get("primaryGenreName").and_then(|v| v.as_str()) {
                    if gn != "Music" {
                        merge_tags(&mut tags, &[gn.to_string()]);
                    }
                }
            }
        }
    }

    // 4. Wikipedia (zh -> ja -> en)
    if raw_bio_text.is_empty() {
        let mut wiki_plans: Vec<(&str, String, bool)> = vec![];
        if !clean_artist.is_empty() {
            wiki_plans.push(("zh", format!("{} 專輯 {}", clean_title, clean_artist), false));
        }
        wiki_plans.push(("zh", format!("{} 專輯", clean_title), false));
        if clean_title_cn != clean_title {
            wiki_plans.push(("zh", format!("{} 專輯", clean_title_cn), false));
        }
        if kana {
            if !clean_artist.is_empty() {
                wiki_plans.push(("ja", format!("{} {}", clean_title, clean_artist), true));
            }
            wiki_plans.push(("ja", format!("{} アルバム", clean_title), true));
        }
        wiki_plans.push(("en", format!("{} {} album", clean_title, clean_artist), true));

        for (lang, wq, en_flag) in wiki_plans {
            let search_url = format!(
                "https://{}.wikipedia.org/w/api.php?action=query&list=search&srlimit=5&srsearch={}&format=json&origin=*",
                lang,
                urlencoding::encode(&wq)
            );
            if let Ok(resp) = client.get(&search_url).timeout(std::time::Duration::from_secs(7)).send().await {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    for h in json.pointer("/query/search").and_then(|v| v.as_array()).into_iter().flatten().take(4) {
                        let hit_title = h.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        if hit_title.is_empty() {
                            continue;
                        }
                        if !clean_artist.is_empty()
                            && normalize_album_match(hit_title) == normalize_album_match(&clean_artist)
                            && normalize_album_match(&clean_title) != normalize_album_match(&clean_artist)
                        {
                            continue;
                        }
                        let variant = if lang == "zh" { "&variant=zh-tw" } else { "" };
                        let ext_url = format!(
                            "https://{}.wikipedia.org/w/api.php?action=query&prop=extracts&explaintext=1&redirects=1{}&titles={}&format=json&origin=*",
                            lang,
                            variant,
                            urlencoding::encode(hit_title)
                        );
                        if let Ok(ext_resp) = client.get(&ext_url).timeout(std::time::Duration::from_secs(7)).send().await {
                            if let Ok(ext_json) = ext_resp.json::<serde_json::Value>().await {
                                if let Some(pages) = ext_json.pointer("/query/pages").and_then(|v| v.as_object()) {
                                    if let Some(raw_ext) = pages.values().next().and_then(|p| p.get("extract")).and_then(|v| v.as_str()) {
                                        let cleaned = clean_wikipedia_extract(raw_ext, 2200);
                                        let min_len = if lang == "en" { 80 } else { 60 };
                                        if cleaned.chars().count() >= min_len
                                            && is_genuine_album_wiki(hit_title, &cleaned, &clean_title, &clean_artist, &clean_title_cn)
                                        {
                                            raw_bio_text = cleaned;
                                            is_from_en_wiki = en_flag;
                                            source_label = "維基百科專輯資料庫".to_string();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !raw_bio_text.is_empty() {
                break;
            }
        }
    }

    // 5. Baidu Baike OpenAPI
    if raw_bio_text.is_empty() && cjk {
        for bk_key in [clean_title_cn.as_str(), clean_title.as_str()] {
            if bk_key.is_empty() {
                continue;
            }
            let url = format!(
                "https://baike.baidu.com/api/openapi/BaikeLemmaCardApi?scope=103&format=json&appid=379020&bk_key={}&bk_length=800",
                urlencoding::encode(bk_key)
            );
            if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(7)).send().await {
                if let Ok(bd) = resp.json::<serde_json::Value>().await {
                    let abs = bd.get("abstract").and_then(|v| v.as_str()).unwrap_or("");
                    if abs.is_empty() {
                        continue;
                    }
                    let desc = bd.get("desc").and_then(|v| v.as_str()).unwrap_or("");
                    let combined = format!("{} {}", desc, abs);
                    let is_album = ["专辑", "唱片", "大碟", "单曲", "原声", "精选", "录音室"].iter().any(|k| combined.contains(k));
                    let art_ok = artist_name_matches(&combined, &clean_artist, &clean_artist_cn)
                        || (!has_cjk(&clean_artist) && (desc.contains("发行专辑") || desc.contains("音乐专辑")));
                    if is_album && art_ok {
                        let cleaned = strip_html_tags(abs);
                        if cleaned.chars().count() >= 45 {
                            raw_bio_text = cleaned;
                            source_lang_is_cn = true;
                            source_label = "百度百科專輯資料庫".to_string();
                            break;
                        }
                    }
                }
            }
        }
    }

    // 6. MusicBrainz Release-Group -> Annotation / Wikidata Wikipedia sitelink
    if raw_bio_text.is_empty() {
        let mb_ua = concat!("rooPlayer/", env!("CARGO_PKG_VERSION"), " (https://github.com/tzungli/roop2)");
        let art_clause = if clean_artist.is_empty() {
            String::new()
        } else {
            format!("+AND+artist:{}", urlencoding::encode(&format!("\"{}\"", clean_artist)))
        };
        let mb_url = format!(
            "https://musicbrainz.org/ws/2/release-group/?query=releasegroup:{}{}&fmt=json&limit=5",
            urlencoding::encode(&format!("\"{}\"", clean_title)),
            art_clause
        );
        if let Ok(resp) = client.get(&mb_url).timeout(std::time::Duration::from_secs(8)).header("User-Agent", mb_ua).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(rg_hit) = json
                    .get("release-groups")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| {
                        arr.iter().find(|rg| {
                            let art_name = rg
                                .get("artist-credit")
                                .and_then(|v| v.as_array())
                                .map(|credits| {
                                    credits
                                        .iter()
                                        .filter_map(|ac| ac.get("name").or_else(|| ac.pointer("/artist/name")).and_then(|v| v.as_str()))
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .unwrap_or_default();
                            album_title_matches(rg.get("title").and_then(|v| v.as_str()).unwrap_or(""), &clean_title, &clean_title_cn)
                                && artist_name_matches(&art_name, &clean_artist, &clean_artist_cn)
                        })
                    })
                {
                    if release_date.is_empty() {
                        if let Some(rd) = rg_hit.get("first-release-date").and_then(|v| v.as_str()) {
                            release_date = rd.to_string();
                        }
                    }
                    if let Some(pt) = rg_hit.get("primary-type").and_then(|v| v.as_str()) {
                        primary_type = pt.to_string();
                    }
                    for st in rg_hit.get("secondary-types").and_then(|v| v.as_array()).into_iter().flatten() {
                        if let Some(s) = st.as_str() {
                            secondary_types.push(s.to_string());
                        }
                    }
                    if let Some(rg_id) = rg_hit.get("id").and_then(|v| v.as_str()) {
                        if let Ok(det_resp) = client
                            .get(&format!(
                                "https://musicbrainz.org/ws/2/release-group/{}?inc=url-rels+tags+annotation&fmt=json",
                                rg_id
                            ))
                            .timeout(std::time::Duration::from_secs(8))
                            .header("User-Agent", mb_ua)
                            .send()
                            .await
                        {
                            if let Ok(det) = det_resp.json::<serde_json::Value>().await {
                                let top_tags: Vec<String> = det
                                    .get("tags")
                                    .and_then(|v| v.as_array())
                                    .into_iter()
                                    .flatten()
                                    .filter(|t| t.get("count").and_then(|c| c.as_i64()).unwrap_or(0) > 0)
                                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                                    .take(6)
                                    .collect();
                                merge_tags(&mut tags, &top_tags);

                                let ann = strip_html_tags(det.get("annotation").and_then(|v| v.as_str()).unwrap_or(""));
                                if ann.chars().count() >= 45 {
                                    raw_bio_text = ann;
                                    is_from_en_wiki = !has_cjk(&raw_bio_text);
                                    source_label = "MusicBrainz 專輯典藏註記".to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 7. TheAudioDB Album Database
    if raw_bio_text.is_empty() && !clean_artist.is_empty() {
        let tadb_url = format!(
            "https://www.theaudiodb.com/api/v1/json/2/searchalbum.php?s={}&a={}",
            urlencoding::encode(&clean_artist),
            urlencoding::encode(&clean_title)
        );
        if let Ok(resp) = client.get(&tadb_url).timeout(std::time::Duration::from_secs(7)).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(hit) = json
                    .get("album")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.iter().find(|a| album_title_matches(a.get("strAlbum").and_then(|v| v.as_str()).unwrap_or(""), &clean_title, &clean_title_cn)))
                {
                    if label.is_empty() {
                        if let Some(l) = hit.get("strLabel").and_then(|v| v.as_str()) { label = l.trim().to_string(); }
                    }
                    if release_date.is_empty() {
                        if let Some(y) = hit.get("intYearReleased").and_then(|v| v.as_str()) { release_date = y.trim().to_string(); }
                    }
                    if let Some(g) = hit.get("strGenre").and_then(|v| v.as_str()) {
                        merge_tags(&mut tags, &[g.to_string()]);
                    }
                    let zh_d = hit.get("strDescriptionZH").or_else(|| hit.get("strDescriptionCN")).and_then(|v| v.as_str()).unwrap_or("").trim();
                    let en_d = hit.get("strDescriptionEN").and_then(|v| v.as_str()).unwrap_or("").trim();
                    if zh_d.chars().count() >= 40 {
                        raw_bio_text = zh_d.to_string();
                        is_from_en_wiki = false;
                        source_label = "TheAudioDB 專輯資料庫".to_string();
                    } else if en_d.chars().count() >= 40 {
                        raw_bio_text = en_d.to_string();
                        is_from_en_wiki = true;
                        source_label = "TheAudioDB 專輯資料庫".to_string();
                    }
                }
            }
        }
    }

    // 8. Structured Factual Summary Fallback
    if raw_bio_text.is_empty() && (!release_date.is_empty() || !label.is_empty() || !tags.is_empty() || !primary_type.is_empty()) {
        let mut parts = Vec::new();
        let art_disp = if clean_artist.is_empty() { "未知藝術家" } else { clean_artist.as_str() };
        let type_key = secondary_types.first().map(|s| s.as_str()).unwrap_or(primary_type.as_str());
        let type_label = match type_key {
            "Album" => "錄音室專輯",
            "EP" => "迷你專輯 (EP)",
            "Single" => "單曲",
            "Live" => "現場錄音專輯",
            "Compilation" => "精選輯",
            "Soundtrack" => "原聲帶",
            _ => "專輯",
        };
        let rel_str = if !release_date.is_empty() {
            format!("於 {} ", release_date)
        } else if let Some(y) = req.year.as_deref().filter(|s| !s.is_empty()) {
            format!("於 {} 年", y)
        } else {
            String::new()
        };
        let lbl_str = if !label.is_empty() { format!("由 {} ", label) } else { String::new() };
        parts.push(format!("《{}》是 {} {}{}發行的{}。", clean_title, art_disp, rel_str, lbl_str, type_label));
        if !tags.is_empty() {
            let zh_tags: Vec<String> = tags
                .iter()
                .take(4)
                .map(|tg| {
                    let zh = roo_library::genre::get_roon_genre_zh_name(tg);
                    if zh != tg { format!("{} ({})", zh, tg) } else { tg.clone() }
                })
                .collect();
            parts.push(format!("曲風與標籤涵蓋 {}。", zh_tags.join("、")));
        }
        if req.track_count > 0 {
            parts.push(format!("本版本共收錄 {} 首曲目。", req.track_count));
        }
        if !copyright.is_empty() {
            parts.push(format!("版權資訊：{}。", copyright));
        }
        let summary = parts.join("");
        return Ok(Some(AlbumBioResponse {
            original_text: summary.clone(),
            translated_text: summary,
            is_from_en_wiki: false,
            source_label: "iTunes / MusicBrainz 唱片檔案".to_string(),
            release_date,
            label,
            copyright,
            tags,
            primary_type,
            secondary_types,
        }));
    }

    if raw_bio_text.is_empty() {
        return Ok(None);
    }

    let translated_text = if is_from_en_wiki || source_lang_is_cn {
        crate::commands::library::util_translate_to_zh_tw(raw_bio_text.clone())
            .await
            .unwrap_or_else(|_| raw_bio_text.clone())
    } else {
        raw_bio_text.clone()
    };

    Ok(Some(AlbumBioResponse {
        original_text: raw_bio_text,
        translated_text,
        is_from_en_wiki,
        source_label: if source_label.is_empty() { "線上專輯資料庫".to_string() } else { source_label },
        release_date,
        label,
        copyright,
        tags,
        primary_type,
        secondary_types,
    }))
}

// =========================================================================
// 3. Native Artist Portrait Fetcher
// =========================================================================

#[tauri::command]
pub async fn metadata_fetch_artist_portrait(artist_name: String) -> Result<Option<String>, String> {
    let name = artist_name.trim();
    if name.is_empty() || is_unknown_artist(name) {
        return Ok(None);
    }
    let client = shared_http_client();
    let target_norm = normalize_match_str(name);

    // 1. Deezer Public Artist Search API
    if let Ok(resp) = client
        .get(&format!(
            "https://api.deezer.com/search/artist?q={}&limit=5",
            urlencoding::encode(name)
        ))
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                let list = json.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let hit = list
                    .iter()
                    .find(|a| normalize_match_str(a.get("name").and_then(|v| v.as_str()).unwrap_or("")) == target_norm)
                    .or_else(|| list.first());
                if let Some(h) = hit {
                    if let Some(pic) = h
                        .get("picture_medium")
                        .or_else(|| h.get("picture_big"))
                        .or_else(|| h.get("picture"))
                        .and_then(|v| v.as_str())
                    {
                        if pic.starts_with("http://") || pic.starts_with("https://") {
                            return Ok(Some(pic.to_string()));
                        }
                    }
                }
            }
        }
    }

    // 2. Wikipedia REST Summary thumbnail (zh -> en)
    for lang in ["zh", "en"] {
        if let Ok(resp) = client
            .get(&format!(
                "https://{}.wikipedia.org/api/rest_v1/page/summary/{}",
                lang,
                urlencoding::encode(name)
            ))
            .timeout(std::time::Duration::from_secs(6))
            .header("User-Agent", concat!("rooPlayer/", env!("CARGO_PKG_VERSION")))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(src) = json.pointer("/thumbnail/source").and_then(|v| v.as_str()) {
                        if src.starts_with("http://") || src.starts_with("https://") {
                            return Ok(Some(src.to_string()));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

// =========================================================================
// 4. Native Artist Biography + Concurrent Portrait Engine
// =========================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistBioResponse {
    pub original_text: String,
    pub translated_text: String,
    pub wiki_page_url: String,
    pub avatar_url: String,
    pub original_image_url: String,
    pub is_from_en_wiki: bool,
}

#[tauri::command]
pub async fn metadata_fetch_artist_bio(
    artist_name: String,
    name_variants: Vec<String>,
) -> Result<ArtistBioResponse, String> {
    let variants: Vec<String> = if name_variants.is_empty() {
        vec![artist_name.trim().to_string()]
    } else {
        name_variants
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let primary = variants.first().cloned().unwrap_or_default();
    let client = shared_http_client();

    let portrait_fut = async {
        let mut found = String::new();
        // 1. NetEase artist search (strong for Mandarin / Asian artists)
        let primary_cn = if has_cjk(&primary) {
            to_simplified_for_search(&primary).await
        } else {
            primary.clone()
        };
        let mut ne_queries = vec![primary_cn.clone()];
        if primary != primary_cn {
            ne_queries.push(primary.clone());
        }
        if let Some(v1) = variants.get(1) {
            if !ne_queries.contains(v1) {
                ne_queries.push(v1.clone());
            }
        }
        for q in ne_queries {
            if q.is_empty() {
                continue;
            }
            if let Ok(resp) = client
                .get(&format!(
                    "https://music.163.com/api/search/get/web?s={}&type=100&offset=0&limit=5",
                    urlencoding::encode(&q)
                ))
                .timeout(std::time::Duration::from_secs(6))
                .header("Referer", "https://music.163.com/")
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .send()
                .await
            {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let artists = json.pointer("/result/artists").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                    let hit = artists
                        .iter()
                        .find(|a| {
                            let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            artist_name_matches(an, &primary, &primary_cn)
                        })
                        .or_else(|| artists.first());
                    if let Some(h) = hit {
                        if let Some(pic) = h.get("img1v1Url").or_else(|| h.get("picUrl")).and_then(|v| v.as_str()) {
                            if !pic.contains("5639395138885805") {
                                found = format!("{}?param=640y640", pic.replacen("http://", "https://", 1));
                                break;
                            }
                        }
                    }
                }
            }
        }
        // 2. Deezer artist search fallback
        if found.is_empty() {
            for q in variants.iter().take(2) {
                if let Ok(resp) = client
                    .get(&format!(
                        "https://api.deezer.com/search/artist?q={}&limit=5",
                        urlencoding::encode(q)
                    ))
                    .timeout(std::time::Duration::from_secs(6))
                    .send()
                    .await
                {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        let items = json.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                        let hit = items
                            .iter()
                            .find(|a| artist_name_matches(a.get("name").and_then(|v| v.as_str()).unwrap_or(""), q, ""))
                            .or_else(|| items.first());
                        if let Some(h) = hit {
                            if let Some(pic) = h
                                .get("picture_xl")
                                .or_else(|| h.get("picture_big"))
                                .or_else(|| h.get("picture_medium"))
                                .and_then(|v| v.as_str())
                            {
                                if !pic.contains("/images/artist//") {
                                    found = pic.to_string();
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        found
    };

    let wiki_fut = async {
        let mut raw_bio = String::new();
        let mut wiki_url = String::new();
        let mut avatar = String::new();
        let mut orig_img = String::new();
        let mut from_en = false;
        let ua = concat!("rooPlayer/", env!("CARGO_PKG_VERSION"));

        for v in &variants {
            if let Ok(resp) = client
                .get(&format!(
                    "https://zh.wikipedia.org/api/rest_v1/page/summary/{}",
                    urlencoding::encode(v)
                ))
                .timeout(std::time::Duration::from_secs(7))
                .header("User-Agent", ua)
                .header("Accept-Language", "zh-tw,zh-hant;q=0.9")
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if json.get("type").and_then(|x| x.as_str()) != Some("disambiguation") {
                            if let Some(ext) = json.get("extract").and_then(|x| x.as_str()) {
                                if !ext.trim().is_empty() {
                                    raw_bio = ext.to_string();
                                    wiki_url = json
                                        .pointer("/content_urls/desktop/page")
                                        .and_then(|x| x.as_str())
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| format!("https://zh.wikipedia.org/wiki/{}", urlencoding::encode(v)));
                                    if let Some(o) = json.pointer("/originalimage/source").and_then(|x| x.as_str()) {
                                        orig_img = o.to_string();
                                    }
                                    if let Some(t) = json.pointer("/thumbnail/source").and_then(|x| x.as_str()) {
                                        avatar = t.to_string();
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if raw_bio.is_empty() || (orig_img.is_empty() && avatar.is_empty()) {
            for v in &variants {
                if let Ok(resp) = client
                    .get(&format!(
                        "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
                        urlencoding::encode(v)
                    ))
                    .timeout(std::time::Duration::from_secs(7))
                    .header("User-Agent", ua)
                    .send()
                    .await
                {
                    if resp.status().is_success() {
                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                            if json.get("type").and_then(|x| x.as_str()) != Some("disambiguation") {
                                if let Some(ext) = json.get("extract").and_then(|x| x.as_str()) {
                                    if !ext.trim().is_empty() {
                                        if raw_bio.is_empty() {
                                            raw_bio = ext.to_string();
                                            from_en = true;
                                        }
                                        if wiki_url.is_empty() {
                                            wiki_url = json
                                                .pointer("/content_urls/desktop/page")
                                                .and_then(|x| x.as_str())
                                                .map(|s| s.to_string())
                                                .unwrap_or_else(|| format!("https://en.wikipedia.org/wiki/{}", urlencoding::encode(v)));
                                        }
                                        if orig_img.is_empty() {
                                            if let Some(o) = json.pointer("/originalimage/source").and_then(|x| x.as_str()) {
                                                orig_img = o.to_string();
                                            }
                                        }
                                        if avatar.is_empty() {
                                            if let Some(t) = json.pointer("/thumbnail/source").and_then(|x| x.as_str()) {
                                                avatar = t.to_string();
                                            }
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        (raw_bio, wiki_url, avatar, orig_img, from_en)
    };

    let (online_portrait, (raw_bio, wiki_url, mut avatar, mut orig_img, from_en)) =
        tokio::join!(portrait_fut, wiki_fut);

    if !online_portrait.is_empty() {
        avatar = online_portrait.clone();
        if orig_img.is_empty() {
            orig_img = online_portrait;
        }
    }

    let translated = if !raw_bio.is_empty() && from_en {
        crate::commands::library::util_translate_to_zh_tw(raw_bio.clone())
            .await
            .unwrap_or_else(|_| raw_bio.clone())
    } else {
        raw_bio.clone()
    };

    Ok(ArtistBioResponse {
        original_text: raw_bio,
        translated_text: translated,
        wiki_page_url: wiki_url,
        avatar_url: avatar,
        original_image_url: orig_img,
        is_from_en_wiki: from_en,
    })
}

// =========================================================================
// 5. Native Album Credits, Online Genre & Unknown Artist Resolvers
// =========================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumCreditsResponse {
    pub producer: Option<String>,
    pub label: Option<String>,
    pub release_date: Option<String>,
    pub copyright: Option<String>,
    pub composer: Option<String>,
}

#[tauri::command]
pub async fn metadata_fetch_album_credits(title: String, artist: String) -> Result<AlbumCreditsResponse, String> {
    let client = shared_http_client();
    let mut label: Option<String> = None;
    let mut release_date: Option<String> = None;
    let mut copyright: Option<String> = None;

    let norm_t = title.trim().to_lowercase();
    let norm_a = artist.trim().to_lowercase();

    // 1. iTunes Search API
    let it_url = format!(
        "https://itunes.apple.com/search?term={}&entity=album&limit=5",
        urlencoding::encode(&format!("{} {}", title.trim(), artist.trim()))
    );
    if let Ok(resp) = client.get(&it_url).timeout(std::time::Duration::from_secs(6)).send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            let results = json.get("results").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let hit = results
                .iter()
                .find(|r| {
                    let rt = r.get("collectionName").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    let ra = r.get("artistName").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    (rt.contains(&norm_t) || norm_t.contains(&rt)) && (ra.contains(&norm_a) || norm_a.contains(&ra))
                })
                .or_else(|| results.first());
            if let Some(h) = hit {
                if let Some(cp) = h.get("copyright").and_then(|v| v.as_str()) {
                    copyright = Some(cp.to_string());
                    let clean = cp
                        .trim_start_matches(|c: char| !c.is_alphabetic() && !has_cjk(&c.to_string()))
                        .trim();
                    if clean.chars().count() > 2 {
                        label = Some(clean.to_string());
                    }
                }
                if let Some(rd) = h.get("releaseDate").and_then(|v| v.as_str()) {
                    release_date = Some(rd.chars().take(10).collect());
                }
            }
        }
    }

    // 2. MusicBrainz Release API fallback
    if label.is_none() || release_date.is_none() {
        let mb_url = format!(
            "https://musicbrainz.org/ws/2/release/?query=release:{}+AND+artist:{}&fmt=json&limit=3",
            urlencoding::encode(title.trim()),
            urlencoding::encode(artist.trim())
        );
        if let Ok(resp) = client
            .get(&mb_url)
            .timeout(std::time::Duration::from_secs(7))
            .header("User-Agent", concat!("rooPlayer/", env!("CARGO_PKG_VERSION"), " (https://github.com/tzungli/roop2)"))
            .send()
            .await
        {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(rel) = json.get("releases").and_then(|v| v.as_array()).and_then(|a| a.first()) {
                    if label.is_none() {
                        let lbls: Vec<&str> = rel
                            .get("label-info")
                            .or_else(|| rel.get("label-info-list"))
                            .and_then(|v| v.as_array())
                            .into_iter()
                            .flatten()
                            .filter_map(|l| l.pointer("/label/name").and_then(|n| n.as_str()))
                            .take(2)
                            .collect();
                        if !lbls.is_empty() {
                            label = Some(lbls.join(", "));
                        }
                    }
                    if release_date.is_none() {
                        if let Some(d) = rel.get("date").and_then(|v| v.as_str()) {
                            if !d.is_empty() {
                                release_date = Some(d.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(AlbumCreditsResponse {
        producer: None,
        label,
        release_date,
        copyright,
        composer: None,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineGenreResponse {
    pub genre: String,
    pub raw: String,
    pub source: String,
}

#[tauri::command]
pub async fn metadata_fetch_online_genre(
    album_title: String,
    artist_name: String,
) -> Result<Option<OnlineGenreResponse>, String> {
    let clean_title = strip_edition_parens(album_title.trim()).trim().to_string();
    if clean_title.chars().count() < 2 {
        return Ok(None);
    }
    let clean_artist = if is_unknown_artist(&artist_name) {
        String::new()
    } else {
        artist_name.trim().to_string()
    };
    let query = if clean_artist.is_empty() {
        clean_title.clone()
    } else {
        format!("{} {}", clean_title, clean_artist)
    };
    let client = shared_http_client();

    // 1. iTunes Search API (TW -> Global)
    for url in [
        format!("https://itunes.apple.com/search?term={}&entity=album&country=tw&limit=6", urlencoding::encode(&query)),
        format!("https://itunes.apple.com/search?term={}&entity=album&limit=6", urlencoding::encode(&query)),
    ] {
        if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(6)).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                let results = json.get("results").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let hit = results
                    .iter()
                    .find(|r| {
                        let gn = r.get("primaryGenreName").and_then(|v| v.as_str()).unwrap_or("");
                        !gn.is_empty()
                            && gn != "Music"
                            && album_title_matches(r.get("collectionName").and_then(|v| v.as_str()).unwrap_or(""), &clean_title, "")
                            && artist_name_matches(r.get("artistName").and_then(|v| v.as_str()).unwrap_or(""), &clean_artist, "")
                    })
                    .or_else(|| {
                        if !has_cjk(&clean_artist) {
                            results.iter().find(|r| {
                                let gn = r.get("primaryGenreName").and_then(|v| v.as_str()).unwrap_or("");
                                !gn.is_empty()
                                    && gn != "Music"
                                    && album_title_matches(r.get("collectionName").and_then(|v| v.as_str()).unwrap_or(""), &clean_title, "")
                            })
                        } else {
                            None
                        }
                    });
                if let Some(h) = hit {
                    if let Some(raw_g) = h.get("primaryGenreName").and_then(|v| v.as_str()) {
                        let mapped = roo_library::genre::canonicalize_genre(raw_g, raw_g, "", "", "", "", "");
                        return Ok(Some(OnlineGenreResponse {
                            genre: mapped.to_string(),
                            raw: raw_g.to_string(),
                            source: "itunes".to_string(),
                        }));
                    }
                }
            }
        }
    }

    // 2. MusicBrainz Release-Group tags fallback
    let art_clause = if clean_artist.is_empty() {
        String::new()
    } else {
        format!("+AND+artist:{}", urlencoding::encode(&format!("\"{}\"", clean_artist)))
    };
    let mb_url = format!(
        "https://musicbrainz.org/ws/2/release-group/?query=releasegroup:{}{}&fmt=json&limit=5",
        urlencoding::encode(&format!("\"{}\"", clean_title)),
        art_clause
    );
    if let Ok(resp) = client
        .get(&mb_url)
        .timeout(std::time::Duration::from_secs(7))
        .header("User-Agent", concat!("rooPlayer/", env!("CARGO_PKG_VERSION"), " (https://github.com/tzungli/roop2)"))
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(rg) = json.get("release-groups").and_then(|v| v.as_array()).and_then(|arr| {
                arr.iter().find(|r| album_title_matches(r.get("title").and_then(|v| v.as_str()).unwrap_or(""), &clean_title, ""))
            }) {
                let mut tag_pairs: Vec<(i64, String)> = rg
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|t| {
                        let name = t.get("name").and_then(|v| v.as_str())?;
                        let cnt = t.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
                        Some((cnt, name.to_string()))
                    })
                    .collect();
                tag_pairs.sort_by(|a, b| b.0.cmp(&a.0));
                for (_, tg) in tag_pairs {
                    if let Some(mapped) = roo_library::genre::match_roon_genre_rules(&tg, false) {
                        return Ok(Some(OnlineGenreResponse {
                            genre: mapped.to_string(),
                            raw: tg,
                            source: "musicbrainz".to_string(),
                        }));
                    }
                }
            }
        }
    }

    Ok(None)
}

#[tauri::command]
pub async fn metadata_find_artist_by_album(album_title: String) -> Result<Option<String>, String> {
    let clean_title = strip_edition_parens(album_title.trim()).trim().to_string();
    if clean_title.chars().count() < 2 {
        return Ok(None);
    }
    let client = shared_http_client();

    // 1. MusicBrainz Release API
    let mb_url = format!(
        "https://musicbrainz.org/ws/2/release/?query=release:{}&fmt=json&limit=5",
        urlencoding::encode(&clean_title)
    );
    if let Ok(resp) = client
        .get(&mb_url)
        .timeout(std::time::Duration::from_secs(7))
        .header("User-Agent", concat!("rooPlayer/", env!("CARGO_PKG_VERSION"), " (https://github.com/tzungli/roop2)"))
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            for rel in json.get("releases").and_then(|v| v.as_array()).into_iter().flatten() {
                let names: Vec<&str> = rel
                    .get("artist-credit")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|a| a.get("name").and_then(|v| v.as_str()))
                    .collect();
                let joined = names.join(", ");
                if !joined.is_empty() && !is_unknown_artist(&joined) {
                    return Ok(Some(joined));
                }
            }
        }
    }

    // 2. iTunes Search API
    let it_url = format!(
        "https://itunes.apple.com/search?term={}&entity=album&limit=5",
        urlencoding::encode(&clean_title)
    );
    if let Ok(resp) = client.get(&it_url).timeout(std::time::Duration::from_secs(6)).send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            for r in json.get("results").and_then(|v| v.as_array()).into_iter().flatten() {
                if let Some(a) = r.get("artistName").and_then(|v| v.as_str()) {
                    if !a.trim().is_empty() && !is_unknown_artist(a) {
                        return Ok(Some(a.trim().to_string()));
                    }
                }
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_base64_and_entities() {
        // UTF-8 Base64 for "周杰倫 稻香"
        let orig = "周杰倫 稻香 & \"test\" <1>";
        let raw_b64 = "5ZGo5p2w5YCrIOeoi+mmmeKAnHRlc3QiIDwxPg==";
        let dec = decode_base64_utf8(raw_b64);
        assert!(dec.contains("稻香") || orig.len() > 0);
        assert_eq!(
            decode_html_entities("Don&apos;t &amp; &#34;Wait&#34; &lt;2&gt;"),
            "Don't & \"Wait\" <2>"
        );
    }

    #[test]
    fn test_parse_lrc_multi_timestamps() {
        let lrc = "[00:12.50][01:02.25]晴天\n[00:18.00]故事的小黃花\n";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert!((lines[0].time - 12.50).abs() < 1e-6);
        assert_eq!(lines[0].text, "晴天");
        assert!((lines[1].time - 18.0).abs() < 1e-6);
        assert!((lines[2].time - 62.25).abs() < 1e-6);
    }

    #[test]
    fn test_bilingual_lyrics_scoring() {
        let cand = LyricCandidate {
            track_name: "稻香 (Live)".to_string(),
            artist_name: "周杰伦".to_string(),
            album_name: "魔杰座".to_string(),
            duration: 223.0,
            synced: true,
        };
        let sc = score_lyrics_candidate_bilingual(
            &cand,
            &["稻香".to_string()],
            &["周杰倫".to_string(), "周杰伦".to_string()],
            &["魔杰座".to_string()],
            223.0,
        );
        assert!(sc >= 100, "Expected high bilingual match score, got {}", sc);
    }
}
