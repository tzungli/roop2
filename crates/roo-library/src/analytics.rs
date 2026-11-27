use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::db::TrackRecord;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongMatchQuery {
    pub title: String,
    #[serde(default)]
    pub artist: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongMatchHit {
    pub query_index: usize,
    pub kind: String, // "local" or "tidal"
    pub track_id: String,
    pub album: String,
    pub artist: String,
    pub title: String,
    pub file_path: String,
    pub tidal_album_id: Option<String>,
}

pub fn normalize_song_key(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();
    // Strip common parenthetical suffixes
    for pat in ["(remaster", "(deluxe", "(official", "(single", "(radio", "(live", "（live", "(feat", "(ft."] {
        if let Some(idx) = s.find(pat) {
            s.truncate(idx);
        }
    }
    if let Some(idx) = s.find(" - ") {
        let tail = &s[idx + 3..];
        if tail.contains("remaster") || tail.contains("single") || tail.contains("edit") || tail.contains("live") {
            s.truncate(idx);
        }
    }
    s.chars()
        .filter(|c| c.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(c) || ('\u{3040}'..='\u{30ff}').contains(c))
        .collect()
}

pub fn artist_keys_match(a: &str, b: &str) -> bool {
    let split_parts = |s: &str| -> Vec<String> {
        s.split([',', ';', '&', '/', '+', '、'])
            .map(|p| {
                p.to_lowercase()
                    .chars()
                    .filter(|c| c.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(c))
                    .collect::<String>()
            })
            .filter(|p| !p.is_empty())
            .collect()
    };
    let pa = split_parts(a);
    let pb = split_parts(b);
    if pa.is_empty() || pb.is_empty() {
        return false;
    }
    pa.iter().any(|x| pb.iter().any(|y| x == y))
}

/// Fast O(N + Q) HashMap batch matcher for matching KKBOX/external chart songs against the local + TIDAL SQLite library.
pub fn match_songs_against_library(queries: &[SongMatchQuery], tracks: &[TrackRecord]) -> Vec<SongMatchHit> {
    let mut index: HashMap<String, Vec<&TrackRecord>> = HashMap::with_capacity(tracks.len());
    for t in tracks {
        let key = normalize_song_key(&t.title);
        if !key.is_empty() {
            index.entry(key).or_default().push(t);
        }
    }

    let mut hits = Vec::new();
    for (qi, q) in queries.iter().enumerate() {
        let qk = normalize_song_key(&q.title);
        if qk.is_empty() {
            continue;
        }
        let Some(cands) = index.get(&qk) else { continue };
        let mut has_local = false;
        let mut has_tidal = false;
        for tr in cands {
            if !q.artist.trim().is_empty() && !artist_keys_match(&q.artist, &tr.artist) && !artist_keys_match(&q.artist, &tr.album_artist) {
                continue;
            }
            let is_tidal = tr.is_tidal || tr.path.starts_with("tidal://");
            if is_tidal && has_tidal {
                continue;
            }
            if !is_tidal && has_local {
                continue;
            }
            if is_tidal { has_tidal = true; } else { has_local = true; }
            hits.push(SongMatchHit {
                query_index: qi,
                kind: if is_tidal { "tidal".to_string() } else { "local".to_string() },
                track_id: tr.id.clone(),
                album: tr.album.clone(),
                artist: tr.artist.clone(),
                title: tr.title.clone(),
                file_path: tr.path.clone(),
                tidal_album_id: tr.tidal_album_id.clone(),
            });
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_song_and_artist_matching() {
        assert_eq!(normalize_song_key("Take Five (Single Edit)"), normalize_song_key("Take Five"));
        assert_eq!(normalize_song_key("Legacy Song (Remastered 2011)"), normalize_song_key("Legacy Song - 2011 Remastered"));
        assert!(artist_keys_match("張學友", "張學友 / 高勝美"));
        assert!(artist_keys_match("Queen + David Bowie", "Queen & David Bowie"));
        assert!(!artist_keys_match("Queen", "Queen Worship Ensemble"));
    }
}
