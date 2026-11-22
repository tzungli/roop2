use serde::{Deserialize, Serialize};
use std::sync::Arc;
use roo_dsp::{
    build_dsf_header, DsdModulator, DitherType,
    PeqBand, PeqChain, REGISTRY,
};
use roo_decoder::read_pcm_samples;
use anyhow::{bail, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DspStreamOptions {
    pub file_path: String,
    #[serde(default)]
    pub start_sec: f64,
    #[serde(default = "default_rate")]
    pub target_rate: u32,
    pub dsd_mode: Option<String>,
    #[serde(default = "default_order")]
    pub dsd_order: u8,
    #[serde(default = "default_phase")]
    pub filter_phase: String,
    pub peq_json: Option<String>,
    pub conv_json: Option<String>,
    #[serde(default)]
    pub ultrasonic_filter: Option<String>,
    #[serde(default)]
    pub dop: Option<bool>,
}

impl DspStreamOptions {
    pub fn cache_key(&self) -> String {
        let peq = self.peq_json.as_deref().unwrap_or("").trim();
        let conv = self.conv_json.as_deref().unwrap_or("").trim();
        let dsd = self.dsd_mode.as_deref().unwrap_or("").trim();
        let ultra = self.ultrasonic_filter.as_deref().unwrap_or("").trim();
        let dop = self.dop.unwrap_or(true);

        format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            self.file_path,
            (self.start_sec * 10.0).round() as i64,
            self.target_rate,
            dsd,
            self.dsd_order,
            self.filter_phase,
            ultra,
            peq,
            conv,
            dop
        )
    }
}

fn default_rate() -> u32 { 0 }
fn default_order() -> u8 { 5 }
fn default_phase() -> String { "linear".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeqConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub pre_gain_db: f64,
    #[serde(default)]
    pub bands: Vec<PeqBand>,
}

pub struct GeneratedStream {
    pub mime_type: &'static str,
    pub total_bytes: u64,
    pub data: Arc<Vec<u8>>,
}

pub fn generate_audio_stream(opts: &DspStreamOptions) -> Result<GeneratedStream> {
    // 1. Read input audio PCM samples
    let (src_rate, mut left, mut right) = read_pcm_samples(&opts.file_path, opts.start_sec, 3600.0)?;
    if left.is_empty() {
        bail!("No audio samples read from file");
    }

    // 2. Apply PEQ if configured
    if let Some(peq_str) = &opts.peq_json {
        if !peq_str.trim().is_empty() && peq_str != "null" {
            if let Ok(peq_conf) = serde_json::from_str::<PeqConfig>(peq_str) {
                if peq_conf.enabled && !peq_conf.bands.is_empty() {
                    let chain = PeqChain::new(src_rate as f64, peq_conf.pre_gain_db, &peq_conf.bands);
                    let mut out_l = vec![0.0; left.len()];
                    let mut out_r = vec![0.0; right.len()];
                    chain.apply_arrays(&left, &right, &mut out_l, &mut out_r);
                    left = out_l;
                    right = out_r;
                }
            }
        }
    }

    // 3. Check if DSD modulation requested
    let is_dsd = opts.dsd_mode.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false);

    if is_dsd {
        // Apply ultrasonic noise protection filter if configured (e.g. 20k, 30k, 40k, 50k)
        if let Some(mode) = &opts.ultrasonic_filter {
            if let Some(coeffs) = roo_dsp::build_ultrasonic_filter_coeffs(src_rate as f64, mode) {
                roo_dsp::apply_ultrasonic_filter(&coeffs, &mut left, &mut right);
            }
        }

        let dsd_mode_str = opts.dsd_mode.as_ref().unwrap();
        let use_dop = opts.dop.unwrap_or(true);

        let dsd_multiplier: u32 = match dsd_mode_str.to_lowercase().as_str() {
            "dsd128" | "dsd256" | "dsd512" if use_dop => 128, // Clamp DLNA DoP to DSD128 (352.8k/384k)
            "dsd128" => 128,
            "dsd256" => 256,
            "dsd512" => 512,
            _ => 64,
        };
        let base_rate: u32 = if src_rate % 48000 == 0 { 48000 } else { 44100 };
        let dsd_rate: u32 = base_rate * dsd_multiplier;

        let ratio = dsd_rate as f64 / src_rate as f64;
        let inv_ratio = 1.0 / ratio;

        let gf = REGISTRY.build_filter(dsd_rate as f64, src_rate as f64, &opts.filter_phase);
        let mut modulator = DsdModulator::new(opts.dsd_order, &opts.filter_phase, DitherType::Gauss1, gf);

        let total_dsd_samples = (left.len() as f64 * ratio).round() as usize;
        let samples_per_block = 32768usize;
        let num_blocks = (total_dsd_samples + samples_per_block - 1) / samples_per_block;

        let data_bytes = num_blocks * 4096 * 2;
        let mut block_data = vec![0u8; data_bytes];
        modulator.run_blocks(&left, &right, left.len(), 0, num_blocks, inv_ratio, &mut block_data);

        if use_dop {
            // Pack DSD stream into DoP (DSD over PCM Standard v1.1) 24-bit stereo WAV container
            let carrier_rate: u32 = dsd_rate / 16;
            let num_frames = (total_dsd_samples + 15) / 16;
            let pcm_data_size = num_frames as u64 * 6;
            let total_file_bytes = 44 + pcm_data_size;

            let mut out_buf = Vec::with_capacity(total_file_bytes as usize);
            // RIFF header
            out_buf.extend_from_slice(b"RIFF");
            out_buf.extend_from_slice(&((36 + pcm_data_size) as u32).to_le_bytes());
            out_buf.extend_from_slice(b"WAVE");
            // fmt chunk
            out_buf.extend_from_slice(b"fmt ");
            out_buf.extend_from_slice(&16u32.to_le_bytes()); // Chunk size 16
            out_buf.extend_from_slice(&1u16.to_le_bytes()); // Format 1 = PCM
            out_buf.extend_from_slice(&2u16.to_le_bytes()); // 2 channels
            out_buf.extend_from_slice(&carrier_rate.to_le_bytes()); // Sample rate (176400 or 352800)
            out_buf.extend_from_slice(&(carrier_rate * 6).to_le_bytes()); // Byte rate
            out_buf.extend_from_slice(&6u16.to_le_bytes()); // Block align = 6
            out_buf.extend_from_slice(&24u16.to_le_bytes()); // 24 bits per sample
            // data chunk
            out_buf.extend_from_slice(b"data");
            out_buf.extend_from_slice(&(pcm_data_size as u32).to_le_bytes());

            let mut frame_count = 0usize;
            'dop_loop: for b in 0..num_blocks {
                let left_off = b * 8192;
                let right_off = left_off + 4096;

                for k in 0..2048 {
                    if frame_count >= num_frames {
                        break 'dop_loop;
                    }

                    let marker = if frame_count % 2 == 0 { 0x05u8 } else { 0xFAu8 };

                    // Left channel (3 bytes: Byte 0 = LSB, Byte 1 = MID, Byte 2 = MSB/marker)
                    let l0 = block_data[left_off + k * 2];
                    let l1 = block_data[left_off + k * 2 + 1];
                    out_buf.push(l1.reverse_bits());
                    out_buf.push(l0.reverse_bits());
                    out_buf.push(marker);

                    // Right channel (3 bytes: Byte 0 = LSB, Byte 1 = MID, Byte 2 = MSB/marker)
                    let r0 = block_data[right_off + k * 2];
                    let r1 = block_data[right_off + k * 2 + 1];
                    out_buf.push(r1.reverse_bits());
                    out_buf.push(r0.reverse_bits());
                    out_buf.push(marker);

                    frame_count += 1;
                }
            }

            Ok(GeneratedStream {
                mime_type: "audio/wav",
                total_bytes: total_file_bytes,
                data: Arc::new(out_buf),
            })
        } else {
            // Raw DSF format
            let total_file_bytes = 92 + data_bytes as u64;
            let mut out_buf = Vec::with_capacity(total_file_bytes as usize);
            let dsf_header = build_dsf_header(total_file_bytes, dsd_rate, (num_blocks * samples_per_block) as u64, 4096);
            out_buf.extend_from_slice(&dsf_header);
            out_buf.extend_from_slice(&block_data);

            Ok(GeneratedStream {
                mime_type: "audio/x-dsf",
                total_bytes: total_file_bytes,
                data: Arc::new(out_buf),
            })
        }
    } else {
        // PCM path: if target_rate is 0 or matches source, keep original rate (bit-perfect pass-through)
        let target_rate = if opts.target_rate == 0 { src_rate } else { opts.target_rate };
        let (mut up_l, mut up_r) = if target_rate == src_rate {
            (left, right)
        } else {
            let ratio = target_rate as f64 / src_rate as f64;
            let out_frames = (left.len() as f64 * ratio).round() as usize;

            let gf = REGISTRY.build_filter(target_rate as f64, src_rate as f64, &opts.filter_phase);
            if let Some(filter) = &gf {
                let mut l_vec = Vec::with_capacity(out_frames);
                let mut r_vec = Vec::with_capacity(out_frames);
                for i in 0..out_frames {
                    l_vec.push(roo_dsp::fir_sample(&left, left.len(), i, ratio, filter));
                    r_vec.push(roo_dsp::fir_sample(&right, right.len(), i, ratio, filter));
                }
                (l_vec, r_vec)
            } else {
                // Polynomial Hermite interpolator
                let mut l_vec = Vec::with_capacity(out_frames);
                let mut r_vec = Vec::with_capacity(out_frames);
                let inv_ratio = 1.0 / ratio;
                for i in 0..out_frames {
                    let t_src = i as f64 * inv_ratio;
                    let idx = t_src.floor() as usize;
                    let frac = t_src - idx as f64;

                    let p1_l = if idx < left.len() { left[idx] } else { 0.0 };
                    let p2_l = if idx + 1 < left.len() { left[idx + 1] } else { 0.0 };
                    let p0_l = if idx > 0 && idx - 1 < left.len() { left[idx - 1] } else { p1_l };
                    let p3_l = if idx + 2 < left.len() { left[idx + 2] } else { p2_l };

                    let p1_r = if idx < right.len() { right[idx] } else { 0.0 };
                    let p2_r = if idx + 1 < right.len() { right[idx + 1] } else { 0.0 };
                    let p0_r = if idx > 0 && idx - 1 < right.len() { right[idx - 1] } else { p1_r };
                    let p3_r = if idx + 2 < right.len() { right[idx + 2] } else { p2_r };

                    l_vec.push(roo_dsp::interpolate_linear(p0_l, p1_l, p2_l, p3_l, frac));
                    r_vec.push(roo_dsp::interpolate_linear(p0_r, p1_r, p2_r, p3_r, frac));
                }
                (l_vec, r_vec)
            }
        };

        // Apply ultrasonic noise protection filter on PCM upsampled stream (e.g. 20k, 30k, 40k, 50k)
        if let Some(mode) = &opts.ultrasonic_filter {
            if let Some(coeffs) = roo_dsp::build_ultrasonic_filter_coeffs(target_rate as f64, mode) {
                roo_dsp::apply_ultrasonic_filter(&coeffs, &mut up_l, &mut up_r);
            }
        }

        let out_frames = up_l.len();
        // Write 24-bit stereo WAV
        let data_size = out_frames as u64 * 6; // 3 bytes L + 3 bytes R
        let total_file_bytes = 44 + data_size;
        let mut out_buf = Vec::with_capacity(total_file_bytes as usize);

        // WAV Header
        out_buf.extend_from_slice(b"RIFF");
        out_buf.extend_from_slice(&((36 + data_size) as u32).to_le_bytes());
        out_buf.extend_from_slice(b"WAVE");
        out_buf.extend_from_slice(b"fmt ");
        out_buf.extend_from_slice(&16u32.to_le_bytes());
        out_buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out_buf.extend_from_slice(&2u16.to_le_bytes()); // 2 channels
        out_buf.extend_from_slice(&target_rate.to_le_bytes());
        out_buf.extend_from_slice(&(target_rate * 6).to_le_bytes()); // Byte rate
        out_buf.extend_from_slice(&6u16.to_le_bytes()); // Block align = 6
        out_buf.extend_from_slice(&24u16.to_le_bytes()); // 24-bit
        out_buf.extend_from_slice(b"data");
        out_buf.extend_from_slice(&(data_size as u32).to_le_bytes());

        // 24-bit PCM samples
        let max_val = 8388607.0f64;
        for i in 0..out_frames {
            let sl = (up_l[i].clamp(-1.0, 1.0) * max_val).round() as i32;
            let sr = (up_r[i].clamp(-1.0, 1.0) * max_val).round() as i32;

            out_buf.push((sl & 0xFF) as u8);
            out_buf.push(((sl >> 8) & 0xFF) as u8);
            out_buf.push(((sl >> 16) & 0xFF) as u8);

            out_buf.push((sr & 0xFF) as u8);
            out_buf.push(((sr >> 8) & 0xFF) as u8);
            out_buf.push(((sr >> 16) & 0xFF) as u8);
        }

        Ok(GeneratedStream {
            mime_type: "audio/wav",
            total_bytes: total_file_bytes,
            data: Arc::new(out_buf),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dsd_stream_generation() {
        // Create a 1-second 44.1kHz stereo WAV file in temp
        let temp_wav = std::env::temp_dir().join("test_dsd_stream.wav");
        let num_samples = 44100usize;
        let mut wav_data = Vec::new();
        wav_data.extend_from_slice(b"RIFF");
        let data_size = (num_samples * 4) as u32;
        wav_data.extend_from_slice(&((36 + data_size) as u32).to_le_bytes());
        wav_data.extend_from_slice(b"WAVE");
        wav_data.extend_from_slice(b"fmt ");
        wav_data.extend_from_slice(&16u32.to_le_bytes());
        wav_data.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav_data.extend_from_slice(&2u16.to_le_bytes()); // 2 channels
        wav_data.extend_from_slice(&44100u32.to_le_bytes());
        wav_data.extend_from_slice(&(44100u32 * 4).to_le_bytes());
        wav_data.extend_from_slice(&4u16.to_le_bytes());
        wav_data.extend_from_slice(&16u16.to_le_bytes()); // 16-bit
        wav_data.extend_from_slice(b"data");
        wav_data.extend_from_slice(&data_size.to_le_bytes());
        for i in 0..num_samples {
            let val = ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / 44100.0).sin() * 10000.0) as i16;
            wav_data.extend_from_slice(&val.to_le_bytes());
            wav_data.extend_from_slice(&val.to_le_bytes());
        }
        std::fs::write(&temp_wav, &wav_data).unwrap();

        // 1. Test DoP WAV generation (default for DLNA streaming)
        let opts_dop = DspStreamOptions {
            file_path: temp_wav.to_string_lossy().to_string(),
            start_sec: 0.0,
            target_rate: 176400,
            dsd_mode: Some("dsd64".to_string()),
            dsd_order: 5,
            filter_phase: "linear".to_string(),
            peq_json: None,
            conv_json: None,
            ultrasonic_filter: Some("30k".to_string()),
            dop: Some(true),
        };

        let res_dop = generate_audio_stream(&opts_dop);
        assert!(res_dop.is_ok());
        let stream_dop = res_dop.unwrap();
        assert_eq!(stream_dop.mime_type, "audio/wav");
        assert!(stream_dop.total_bytes > 44);
        assert_eq!(&stream_dop.data[0..4], b"RIFF");
        assert_eq!(&stream_dop.data[8..12], b"WAVE");
        // Verify DoP marker in the first frame (byte offset 44 + 2 for Left MSB, offset 44 + 5 for Right MSB)
        assert_eq!(stream_dop.data[44 + 2], 0x05);
        assert_eq!(stream_dop.data[44 + 5], 0x05);
        // Verify DoP marker in the second frame (byte offset 44 + 6 + 2 = 52, 44 + 6 + 5 = 55)
        assert_eq!(stream_dop.data[44 + 6 + 2], 0xFA);
        assert_eq!(stream_dop.data[44 + 6 + 5], 0xFA);

        // 2. Test raw DSF generation
        let opts_dsf = DspStreamOptions {
            file_path: temp_wav.to_string_lossy().to_string(),
            start_sec: 0.0,
            target_rate: 0,
            dsd_mode: Some("dsd64".to_string()),
            dsd_order: 5,
            filter_phase: "linear".to_string(),
            peq_json: None,
            conv_json: None,
            ultrasonic_filter: Some("30k".to_string()),
            dop: Some(false),
        };

        let res_dsf = generate_audio_stream(&opts_dsf);
        let _ = std::fs::remove_file(temp_wav);
        assert!(res_dsf.is_ok());
        let stream_dsf = res_dsf.unwrap();
        assert_eq!(stream_dsf.mime_type, "audio/x-dsf");
        assert!(stream_dsf.total_bytes > 0);
        assert_eq!(&stream_dsf.data[0..4], b"DSD ");

        // 3. Test canonical cache_key equivalence (None vs Some(""))
        let mut opts1 = opts_dop.clone();
        opts1.peq_json = None;
        opts1.conv_json = None;

        let mut opts2 = opts_dop.clone();
        opts2.peq_json = Some("".to_string());
        opts2.conv_json = Some("  ".to_string());

        assert_eq!(opts1.cache_key(), opts2.cache_key());
    }
}

