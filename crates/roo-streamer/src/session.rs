use serde::{Deserialize, Serialize};
use std::sync::Arc;
use roo_dsp::{
    build_dsf_header, DsdModulator, DitherType,
    get_dsd_rate, PeqBand, PeqChain, REGISTRY,
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
}

fn default_rate() -> u32 { 192000 }
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
        let dsd_mode_str = opts.dsd_mode.as_ref().unwrap();
        let dsd_rate = get_dsd_rate(dsd_mode_str);
        let ratio = dsd_rate as f64 / src_rate as f64;
        let inv_ratio = 1.0 / ratio;

        let gf = REGISTRY.build_filter(dsd_rate as f64, src_rate as f64, &opts.filter_phase);
        let mut modulator = DsdModulator::new(opts.dsd_order, &opts.filter_phase, DitherType::Gauss1, gf);

        let total_dsd_samples = (left.len() as f64 * ratio).round() as usize;
        let samples_per_block = 32768usize;
        let num_blocks = (total_dsd_samples + samples_per_block - 1) / samples_per_block;

        let data_bytes = num_blocks * 4096 * 2;
        let total_file_bytes = 92 + data_bytes as u64;

        let mut out_buf = Vec::with_capacity(total_file_bytes as usize);
        let dsf_header = build_dsf_header(total_file_bytes, dsd_rate, (num_blocks * samples_per_block) as u64, 4096);
        out_buf.extend_from_slice(&dsf_header);

        let mut block_data = vec![0u8; data_bytes];
        modulator.run_blocks(&left, &right, left.len(), 0, num_blocks, inv_ratio, &mut block_data);
        out_buf.extend_from_slice(&block_data);

        Ok(GeneratedStream {
            mime_type: "audio/x-dsf",
            total_bytes: total_file_bytes,
            data: Arc::new(out_buf),
        })
    } else {
        // PCM Upsampling path (e.g. up to 192kHz/384kHz 24-bit WAV)
        let target_rate = opts.target_rate;
        let ratio = target_rate as f64 / src_rate as f64;
        let out_frames = (left.len() as f64 * ratio).round() as usize;

        let gf = REGISTRY.build_filter(target_rate as f64, src_rate as f64, &opts.filter_phase);
        let (up_l, up_r) = if let Some(filter) = &gf {
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
        };

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
