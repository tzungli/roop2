use std::fs;
use std::path::Path;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedM3uEntry {
    pub file_path: String,
    pub title: String,
    pub artist: String,
    pub duration: f64,
}

pub fn parse_m3u_text(text: &str) -> Vec<ParsedM3uEntry> {
    let mut items = Vec::new();
    let mut pending_title = String::new();
    let mut pending_artist = String::new();
    let mut pending_dur = 0.0f64;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() >= 7 && line[..7].eq_ignore_ascii_case("#EXTINF") {
            if let Some((meta_part, title_part)) = line.split_once(',') {
                if let Some((_, dur_s)) = meta_part.split_once(':') {
                    if let Ok(d) = dur_s.trim().parse::<f64>() {
                        if d > 0.0 {
                            pending_dur = d;
                        }
                    }
                }
                let raw_title = title_part.trim();
                if let Some((a, t)) = raw_title.split_once(" - ") {
                    pending_artist = a.trim().to_string();
                    pending_title = t.trim().to_string();
                } else {
                    pending_artist.clear();
                    pending_title = raw_title.to_string();
                }
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let fallback_title = Path::new(line)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(line)
            .to_string();
        items.push(ParsedM3uEntry {
            file_path: line.to_string(),
            title: if pending_title.is_empty() { fallback_title } else { pending_title.clone() },
            artist: pending_artist.clone(),
            duration: pending_dur,
        });
        pending_title.clear();
        pending_artist.clear();
        pending_dur = 0.0;
    }

    items
}

pub fn read_playlist_file(path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)?;
    let mut tracks = Vec::new();

    let parent = path.parent().unwrap_or(Path::new(""));

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let p = Path::new(trimmed);
        if p.is_absolute() {
            tracks.push(trimmed.to_string());
        } else {
            let abs = parent.join(p);
            tracks.push(abs.to_string_lossy().to_string());
        }
    }

    Ok(tracks)
}

pub fn write_playlist_file(path: &Path, tracks: &[String]) -> Result<()> {
    let mut out = String::from("#EXTM3U\n");
    for t in tracks {
        out.push_str(t);
        out.push('\n');
    }
    fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_m3u_text_extinf_and_paths() {
        let m3u = "#EXTM3U\n#EXTINF:245,Miles Davis - So What\nD:/Music/KindOfBlue/01_So_What.flac\n#EXTINF:180,Take Five\nD:/Music/TimeOut/03_Take_Five.wav\n";
        let parsed = parse_m3u_text(m3u);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].artist, "Miles Davis");
        assert_eq!(parsed[0].title, "So What");
        assert!((parsed[0].duration - 245.0).abs() < 1e-6);
        assert_eq!(parsed[1].artist, "");
        assert_eq!(parsed[1].title, "Take Five");
    }
}
