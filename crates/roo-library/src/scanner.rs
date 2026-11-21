use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use serde::{Deserialize, Serialize};
use crate::db::TrackRecord;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub file: String,
}

pub fn scan_directory<F>(
    dir_path: &Path,
    covers_dir: &Path,
    on_progress: F,
) -> Result<Vec<TrackRecord>>
where
    F: Fn(ScanProgress) + Send + Sync,
{
    let mut files = Vec::new();
    collect_audio_files(dir_path, &mut files);

    let total = files.len();
    let mut tracks = Vec::with_capacity(total);

    for (idx, file_path) in files.into_iter().enumerate() {
        on_progress(ScanProgress {
            current: idx + 1,
            total,
            file: file_path.to_string_lossy().to_string(),
        });

        if let Ok(track) = read_file_metadata(&file_path, covers_dir) {
            tracks.push(track);
        }
    }

    Ok(tracks)
}

fn collect_audio_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_audio_files(&p, files);
            } else if is_supported_audio(&p) {
                files.push(p);
            }
        }
    }
}

fn is_supported_audio(p: &Path) -> bool {
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    matches!(
        ext.as_str(),
        "flac" | "wav" | "aiff" | "aif" | "aifc" | "dsf" | "dff" | "mp3" | "m4a" | "aac" | "ogg"
    )
}

pub fn read_file_metadata(file_path: &Path, covers_dir: &Path) -> Result<TrackRecord> {
    let tagged_file = Probe::open(file_path)?.read()?;
    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());
    let properties = tagged_file.properties();

    let title = tag.and_then(|t| t.title().map(|s| s.to_string()))
        .unwrap_or_else(|| file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("Unknown").to_string());

    let artist = tag.and_then(|t| t.artist().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown Artist".to_string());

    let album_artist = tag.and_then(|t| t.get_string(&lofty::tag::ItemKey::AlbumArtist).map(|s| s.to_string()))
        .unwrap_or_else(|| artist.clone());

    let album = tag.and_then(|t| t.album().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown Album".to_string());

    let year = tag.and_then(|t| t.year());
    let genre = tag.and_then(|t| t.genre().map(|s| s.to_string()))
        .unwrap_or_else(|| "Unknown".to_string());

    let track_number = tag.and_then(|t| t.track());
    let disk_number = tag.and_then(|t| t.disk());

    let duration = properties.duration().as_secs_f64();
    let sample_rate = properties.sample_rate().unwrap_or(44100);
    let bits_per_sample = properties.bit_depth().unwrap_or(16) as u32;

    let format = file_path.extension().and_then(|s| s.to_str()).unwrap_or("unknown").to_uppercase();

    // Check embedded cover art
    let mut cover_path = None;
    if let Some(t) = tag {
        let pictures = t.pictures();
        if let Some(pic) = pictures.first() {
            let hash = format!("{:x}", md5_hash(&pic.data()));
            let cover_file = covers_dir.join(format!("{}.jpg", hash));
            if !cover_file.exists() {
                let _ = fs::create_dir_all(covers_dir);
                let _ = fs::write(&cover_file, pic.data());
            }
            cover_path = Some(cover_file.to_string_lossy().to_string());
        }
    }

    let id = format!("{:x}", md5_hash(file_path.to_string_lossy().as_bytes()));
    let date_added = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();

    Ok(TrackRecord {
        id,
        path: file_path.to_string_lossy().to_string(),
        title,
        artist,
        album_artist,
        album,
        year,
        genre,
        track_number,
        disk_number,
        duration,
        sample_rate,
        bits_per_sample,
        format,
        cover_path,
        date_added,
    })
}

fn md5_hash(data: &[u8]) -> u128 {
    // Simple 128-bit FNV-1a hash
    let mut hash = 0x6c62272e07bb014262b821756295c58du128;
    for &b in data {
        hash ^= b as u128;
        hash = hash.wrapping_mul(0x1000000000000000000013b);
    }
    hash
}
