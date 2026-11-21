pub mod wav;
pub mod aiff;
pub mod flac;
pub mod dsd_file;
pub mod isobmff;

pub use wav::*;
pub use aiff::*;
pub use flac::*;
pub use dsd_file::*;
pub use isobmff::*;

use std::fs;
use std::path::Path;
use anyhow::Result;

pub fn read_pcm_samples(path: &str, start_sec: f64, max_duration_sec: f64) -> Result<(u32, Vec<f64>, Vec<f64>)> {
    let p = Path::new(path);
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

    match ext.as_str() {
        "wav" => read_wav_pcm(path, start_sec, max_duration_sec),
        "aiff" | "aif" | "aifc" => read_aiff_pcm(path, start_sec, max_duration_sec),
        "dsf" => decimate_dsf_to_pcm(path, start_sec, max_duration_sec),
        "flac" => {
            // Check if it's TIDAL CMAF ISO-BMFF encapsulated
            if let Ok(data) = fs::read(path) {
                if is_isobmff(&data) {
                    if let Some(raw_flac) = extract_flac_from_isobmff(&data) {
                        let temp_path = std::env::temp_dir().join("roo_temp_demuxed.flac");
                        fs::write(&temp_path, raw_flac)?;
                        let res = read_flac_pcm(&temp_path.to_string_lossy(), start_sec, max_duration_sec);
                        let _ = fs::remove_file(temp_path);
                        return res;
                    }
                }
            }
            read_flac_pcm(path, start_sec, max_duration_sec)
        }
        _ => read_flac_pcm(path, start_sec, max_duration_sec),
    }
}
