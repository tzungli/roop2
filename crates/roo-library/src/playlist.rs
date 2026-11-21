use std::fs;
use std::path::Path;
use anyhow::Result;

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
