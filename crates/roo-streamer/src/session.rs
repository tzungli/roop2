use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::LazyLock;
use roo_dsp::{
    build_dsf_header, ConvolutionEngine, DsdModulator, DitherType,
    PeqBand, PeqChain, REGISTRY, ir_response_peak, resample_ir_to_rate,
};
use roo_decoder::{read_pcm_samples, read_wav_pcm};
use anyhow::{bail, Result};

pub struct ProgressiveSession {
    pub token: String,
    pub temp_file_path: PathBuf,
    pub mime_type: &'static str,
    pub total_bytes: u64,
    pub bytes_written: Arc<AtomicU64>,
    pub is_completed: Arc<AtomicBool>,
    pub is_cancelled: Arc<AtomicBool>,
    pub notify: Arc<tokio::sync::Notify>,
    pub is_temp_file: bool,
}

impl Drop for ProgressiveSession {
    fn drop(&mut self) {
        self.is_cancelled.store(true, Ordering::Relaxed);
        if self.is_temp_file {
            let _ = std::fs::remove_file(&self.temp_file_path);
        }
    }
}

static PROGRESSIVE_SESSIONS: LazyLock<RwLock<HashMap<String, Arc<ProgressiveSession>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn get_progressive_session(token: &str) -> Option<Arc<ProgressiveSession>> {
    PROGRESSIVE_SESSIONS.read().unwrap().get(token).cloned()
}

pub fn cancel_all_progressive_streams() {
    let mut write = PROGRESSIVE_SESSIONS.write().unwrap();
    for session in write.values() {
        session.is_cancelled.store(true, Ordering::Relaxed);
        session.notify.notify_waiters();
    }
    write.clear();
}

pub fn cancel_progressive_stream(token: &str) {
    let mut write = PROGRESSIVE_SESSIONS.write().unwrap();
    if let Some(session) = write.remove(token) {
        session.is_cancelled.store(true, Ordering::Relaxed);
        session.notify.notify_waiters();
    }
}

pub fn register_direct_file_stream(
    token: &str,
    file_path: &std::path::Path,
    mime_type: &'static str,
) -> Result<Arc<ProgressiveSession>> {
    cancel_progressive_stream(token);
    let meta = std::fs::metadata(file_path)?;
    let total_bytes = meta.len();
    let session = Arc::new(ProgressiveSession {
        token: token.to_string(),
        temp_file_path: file_path.to_path_buf(),
        mime_type,
        total_bytes,
        bytes_written: Arc::new(AtomicU64::new(total_bytes)),
        is_completed: Arc::new(AtomicBool::new(true)),
        is_cancelled: Arc::new(AtomicBool::new(false)),
        notify: Arc::new(tokio::sync::Notify::new()),
        is_temp_file: false,
    });
    PROGRESSIVE_SESSIONS.write().unwrap().insert(token.to_string(), session.clone());
    Ok(session)
}

/// Stage-1 Polyphase FIR upsampler for Two-Stage DSD architecture (src_rate -> 352.8 kHz).
/// Eliminates all ultrasonic images between 22.05 kHz and 330.75 kHz by >130 dB before
/// Stage-2 Hermite interpolation + Delta-Sigma modulation, using minimal RAM per batch.
fn upsample_stage1_channel(
    pcm: &[f64],
    gf: &roo_dsp::FirFilter,
    ratio: f64,
    start_idx: usize,
    end_idx: usize,
) -> Vec<f64> {
    let in_len = pcm.len();
    let count = end_idx.saturating_sub(start_idx);
    let mut out = vec![0.0f64; count];
    if let Some(phases) = &gf.poly_phases {
        let p = phases.p;
        let q = phases.q as i32;
        for (dst_i, out_idx) in (start_idx..end_idx).enumerate() {
            let phi = out_idx % p;
            let entry = &phases.entries[phi];
            let k_base = entry.k0 + (out_idx / p) as i32 * q;
            let t = entry.t;
            if k_base >= 0 && (k_base as usize + t) <= in_len {
                let start = k_base as usize;
                out[dst_i] = roo_dsp::polyphase_dot_product(&pcm[start..start + t], &entry.g[..t]);
            } else {
                out[dst_i] = roo_dsp::fir_sample(pcm, in_len, out_idx, ratio, gf);
            }
        }
    } else {
        for (dst_i, out_idx) in (start_idx..end_idx).enumerate() {
            out[dst_i] = roo_dsp::fir_sample(pcm, in_len, out_idx, ratio, gf);
        }
    }
    out
}

fn upsample_stage1_stereo(
    left: &[f64],
    right: &[f64],
    gf: &roo_dsp::FirFilter,
    ratio: f64,
    start_idx: usize,
    end_idx: usize,
) -> (Vec<f64>, Vec<f64>) {
    std::thread::scope(|s| {
        let h_l = s.spawn(|| upsample_stage1_channel(left, gf, ratio, start_idx, end_idx));
        let r_out = upsample_stage1_channel(right, gf, ratio, start_idx, end_idx);
        let l_out = h_l.join().unwrap();
        (l_out, r_out)
    })
}

enum StageResampler {
    Fir(roo_dsp::FirFilter),
    SincM(roo_dsp::SincMFftResampler),
}

impl StageResampler {
    fn build(fs_out: f64, src_rate: f64, phase_id: &str) -> Option<Self> {
        if let Some(preset) = roo_dsp::sinc_long_preset_for_phase(phase_id) {
            if let Some(fft_resampler) = roo_dsp::SincMFftResampler::new_with_preset(fs_out, src_rate, preset) {
                return Some(Self::SincM(fft_resampler));
            }
        }
        REGISTRY.build_filter(fs_out, src_rate, phase_id).map(Self::Fir)
    }

    fn upsample_range_stereo(
        &self,
        left: &[f64],
        right: &[f64],
        ratio: f64,
        start_idx: usize,
        end_idx: usize,
    ) -> (Vec<f64>, Vec<f64>) {
        match self {
            Self::Fir(gf) => upsample_stage1_stereo(left, right, gf, ratio, start_idx, end_idx),
            Self::SincM(resampler) => resampler.upsample_range_stereo(left, right, start_idx, end_idx),
        }
    }
}

fn render_pcm_range_helper(
    left: &[f64],
    right: &[f64],
    resampler: Option<&StageResampler>,
    target_rate: u32,
    src_rate: u32,
    ratio: f64,
    start_f: usize,
    end_f: usize,
    dither_type: roo_dsp::DitherType,
    ditherer: &mut roo_dsp::StereoPcmDitherer,
    buf: &mut [u8],
) {
    let mut off = 0;

    if target_rate != src_rate {
        if let Some(res) = resampler {
            let (up_l, up_r) = res.upsample_range_stereo(left, right, ratio, start_f, end_f);
            for (sl_f, sr_f) in up_l.into_iter().zip(up_r.into_iter()) {
                let (i_l, i_r) = ditherer.quantize_24bit(sl_f, sr_f, dither_type);
                let bytes_l = i_l.to_le_bytes();
                buf[off] = bytes_l[0];
                buf[off + 1] = bytes_l[1];
                buf[off + 2] = bytes_l[2];

                let bytes_r = i_r.to_le_bytes();
                buf[off + 3] = bytes_r[0];
                buf[off + 4] = bytes_r[1];
                buf[off + 5] = bytes_r[2];
                off += 6;
            }
            return;
        }
    }

    for i in start_f..end_f {
        let idx = i.min(left.len().saturating_sub(1));
        let (sl_f, sr_f) = (left[idx], right[idx]);

        let (i_l, i_r) = ditherer.quantize_24bit(sl_f, sr_f, dither_type);
        let bytes_l = i_l.to_le_bytes();
        buf[off] = bytes_l[0];
        buf[off + 1] = bytes_l[1];
        buf[off + 2] = bytes_l[2];

        let bytes_r = i_r.to_le_bytes();
        buf[off + 3] = bytes_r[0];
        buf[off + 4] = bytes_r[1];
        buf[off + 5] = bytes_r[2];
        off += 6;
    }
}

pub fn start_progressive_stream(opts: &DspStreamOptions, token: &str) -> Result<Arc<ProgressiveSession>> {
    cancel_progressive_stream(token);

    let cache_dir = std::env::temp_dir().join("rooplayer_dsp_cache");
    let _ = std::fs::create_dir_all(&cache_dir);

    // Clean up older cache files (> 1 hour)
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(elapsed) = meta.modified().and_then(|m| m.elapsed().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))) {
                    if elapsed.as_secs() > 3600 {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }

    // 1. Read input audio PCM samples
    let (src_rate, mut left, mut right) = read_pcm_samples(&opts.file_path, opts.start_sec, 3600.0)?;
    if left.is_empty() {
        bail!("No audio samples read from file: {}", opts.file_path);
    }

    // 2. Resync lead-in silence padding for DLNA streaming from start (start_sec == 0)
    // Prevents hardware DAC PLL clock lock and unmute delay from eating the first 0.5s of audio!
    let lead_in_sec = if opts.start_sec == 0.0 { 0.6 } else { 0.0 };
    let lead_in_frames = (lead_in_sec * src_rate as f64).round() as usize;
    if lead_in_frames > 0 {
        let mut padded_l = vec![0.0f64; left.len() + lead_in_frames];
        let mut padded_r = vec![0.0f64; right.len() + lead_in_frames];
        padded_l[lead_in_frames..].copy_from_slice(&left);
        padded_r[lead_in_frames..].copy_from_slice(&right);
        left = padded_l;
        right = padded_r;
    }

    // 3. Apply PEQ if configured
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

    // 3.5 Apply Convolution (IR) if configured
    apply_convolution_if_configured(&opts.conv_json, src_rate, &mut left, &mut right);

    // 4. Apply ultrasonic noise protection filter if configured (e.g. 20k, 30k, 40k, 50k)
    if let Some(mode) = &opts.ultrasonic_filter {
        if let Some(coeffs) = roo_dsp::build_ultrasonic_filter_coeffs(src_rate as f64, mode) {
            roo_dsp::apply_ultrasonic_filter(&coeffs, &mut left, &mut right);
        }
    }

    let is_dsd = opts.dsd_mode.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false);
    let left = Arc::new(left);
    let right = Arc::new(right);

    if is_dsd {
        let dsd_mode_str = opts.dsd_mode.as_ref().unwrap();
        let dsd_multiplier: u32 = match dsd_mode_str.to_lowercase().as_str() {
            "dsd128" => 128,
            "dsd256" => 256,
            "dsd512" => 512,
            _ => 64,
        };
        let dsd_rate: u32 = 44100 * dsd_multiplier;
        let ratio = dsd_rate as f64 / src_rate as f64;
        let inv_ratio = 1.0 / ratio;

        let total_dsd_samples = (left.len() as f64 * ratio).floor() as usize;
        let samples_per_block = 32768usize;
        let num_blocks = (total_dsd_samples + samples_per_block - 1) / samples_per_block;
        let total_file_bytes = 92 + (num_blocks * 8192) as u64;

        let temp_file_path = cache_dir.join(format!(
            "stream_{}_{}.dsf",
            token,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()
        ));

        let header = build_dsf_header(total_file_bytes, dsd_rate, (num_blocks * samples_per_block) as u64, 4096);
        {
            use std::io::{Seek, SeekFrom, Write};
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(&temp_file_path)?;
            file.set_len(total_file_bytes)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&header)?;
            file.flush()?;
        }

        let dither_type = opts.dither.as_deref().and_then(DitherType::from_str_opt).unwrap_or(DitherType::Gauss);
        // Two-stage audiophile DSD upsampling:
        // Stage 1: Polyphase FIR anti-imaging filter (176.4 kHz for DSD64, 352.8 kHz for DSD128/256/512)
        // Stage 2: Forward-difference 64-bit Hermite + parallel L/R AVX2+FMA CLANS modulator.
        let stage1_rate: u32 = if dsd_multiplier == 64 { 176400 } else { 352800 };
        let use_stage1 = src_rate < stage1_rate;
        let stage1_gf = if use_stage1 {
            StageResampler::build(stage1_rate as f64, src_rate as f64, &opts.filter_phase).map(Arc::new)
        } else {
            None
        };
        let stage1_ratio = stage1_rate as f64 / src_rate as f64;
        let stage2_ratio: usize = (dsd_rate / stage1_rate) as usize; // exact integer: 8, 16, 32, or 64
        let stage2_inv_ratio = 1.0 / stage2_ratio as f64;

        let mut modulator = DsdModulator::new(opts.dsd_order, &opts.filter_phase, dither_type, None);
        modulator.set_dsd_rate(dsd_rate);

        let run_dsd_batch = |m: &mut DsdModulator,
                             src_l: &[f64],
                             src_r: &[f64],
                             s1_gf: Option<&StageResampler>,
                             start_block: usize,
                             blocks_to_run: usize,
                             out_slice: &mut [u8]| {
            if let Some(gf) = s1_gf {
                let start_bit = start_block * samples_per_block;
                let num_bits = blocks_to_run * samples_per_block;
                let start_s1 = start_bit / stage2_ratio;
                let num_s1 = num_bits / stage2_ratio;
                let pad_start = start_s1.saturating_sub(1);
                let lead_pad = start_s1 - pad_start;
                let pad_end = start_s1 + num_s1 + 3;
                let (s1_l, s1_r) = gf.upsample_range_stereo(src_l, src_r, stage1_ratio, pad_start, pad_end);
                m.run_blocks(&s1_l, &s1_r, s1_l.len(), lead_pad * stage2_ratio, blocks_to_run, stage2_inv_ratio, out_slice);
            } else {
                m.run_blocks(src_l, src_r, src_l.len(), start_block * samples_per_block, blocks_to_run, inv_ratio, out_slice);
            }
        };

        let target_initial_blocks = ((3.5 * dsd_rate as f64) / samples_per_block as f64).ceil() as usize;
        let initial_blocks = num_blocks.min(target_initial_blocks.max(32));
        let mut initial_data = vec![0u8; initial_blocks * 8192];
        if initial_blocks > 0 {
            run_dsd_batch(&mut modulator, &left, &right, stage1_gf.as_deref(), 0, initial_blocks, &mut initial_data);
            use std::io::{Seek, SeekFrom, Write};
            let mut file = std::fs::OpenOptions::new().read(true).write(true).open(&temp_file_path)?;
            file.seek(SeekFrom::Start(92))?;
            file.write_all(&initial_data)?;
            file.flush()?;
        }

        let bytes_written = Arc::new(AtomicU64::new(92 + (initial_blocks * 8192) as u64));
        let is_completed = Arc::new(AtomicBool::new(initial_blocks >= num_blocks));
        let is_cancelled = Arc::new(AtomicBool::new(false));
        let notify = Arc::new(tokio::sync::Notify::new());

        let session = Arc::new(ProgressiveSession {
            token: token.to_string(),
            temp_file_path: temp_file_path.clone(),
            mime_type: "audio/x-dsf",
            total_bytes: total_file_bytes,
            bytes_written: bytes_written.clone(),
            is_completed: is_completed.clone(),
            is_cancelled: is_cancelled.clone(),
            notify: notify.clone(),
            is_temp_file: true,
        });

        PROGRESSIVE_SESSIONS.write().unwrap().insert(token.to_string(), session.clone());

        if initial_blocks < num_blocks {
            let bg_file_path = temp_file_path.clone();
            let bg_bytes_written = bytes_written;
            let bg_is_completed = is_completed;
            let bg_is_cancelled = is_cancelled;
            let bg_notify = notify;
            let bg_left = left.clone();
            let bg_right = right.clone();
            let bg_stage1_gf = stage1_gf.clone();
            let mut bg_modulator = modulator;

            std::thread::spawn(move || {
                use std::io::{Seek, SeekFrom, Write};
                let Ok(mut file) = std::fs::OpenOptions::new().read(true).write(true).open(&bg_file_path) else {
                    return;
                };
                let start_offset = 92 + (initial_blocks * 8192) as u64;
                if file.seek(SeekFrom::Start(start_offset)).is_err() {
                    return;
                }

                // Scale batch size with DSD multiplier (~0.37s per batch) to minimize OS thread-spawn overhead
                let batch_blocks = ((dsd_multiplier as usize) / 2).max(32);
                let mut curr_block = initial_blocks;
                let mut batch_buf = vec![0u8; batch_blocks * 8192];
                let bg_start = std::time::Instant::now();

                while curr_block < num_blocks {
                    if bg_is_cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    let blocks_to_run = (num_blocks - curr_block).min(batch_blocks);
                    let slice = &mut batch_buf[..blocks_to_run * 8192];
                    if let Some(gf) = bg_stage1_gf.as_deref() {
                        let start_bit = curr_block * samples_per_block;
                        let num_bits = blocks_to_run * samples_per_block;
                        let start_s1 = start_bit / stage2_ratio;
                        let num_s1 = num_bits / stage2_ratio;
                        let pad_start = start_s1.saturating_sub(1);
                        let lead_pad = start_s1 - pad_start;
                        let pad_end = start_s1 + num_s1 + 3;
                        let (s1_l, s1_r) = gf.upsample_range_stereo(&bg_left, &bg_right, stage1_ratio, pad_start, pad_end);
                        bg_modulator.run_blocks(&s1_l, &s1_r, s1_l.len(), lead_pad * stage2_ratio, blocks_to_run, stage2_inv_ratio, slice);
                    } else {
                        bg_modulator.run_blocks(&bg_left, &bg_right, bg_left.len(), curr_block * samples_per_block, blocks_to_run, inv_ratio, slice);
                    }
                    if file.write_all(slice).is_err() {
                        return;
                    }
                    let _ = file.flush();
                    curr_block += blocks_to_run;
                    bg_bytes_written.store(92 + (curr_block * 8192) as u64, Ordering::Release);
                    bg_notify.notify_waiters();

                    // Gentle CPU pacing once we have a >20s safety cushion ahead of playback time.
                    // Keeps steady-state CPU usage ultra-low (DSD64 <1%, DSD128 ~1-2%, DSD256 ~2-3%, DSD512 ~4-6%).
                    let written_secs = (curr_block * samples_per_block) as f64 / dsd_rate as f64;
                    let ahead_secs = written_secs - bg_start.elapsed().as_secs_f64();
                    if ahead_secs > 20.0 && curr_block < num_blocks {
                        let sleep_ticks = if dsd_multiplier >= 512 { 8 } else { 11 };
                        for _ in 0..sleep_ticks {
                            if bg_is_cancelled.load(Ordering::Relaxed) {
                                return;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }
                    }
                }
                bg_is_completed.store(true, Ordering::Release);
                bg_notify.notify_waiters();
            });
        }

        Ok(session)
    } else {
        let target_rate = if opts.target_rate == 0 { src_rate } else { opts.target_rate };
        let ratio = target_rate as f64 / src_rate as f64;
        let out_frames = (left.len() as f64 * ratio).floor() as usize;

        let data_size = out_frames as u64 * 6;
        let total_file_bytes = 44 + data_size;

        let temp_file_path = cache_dir.join(format!(
            "stream_{}_{}.wav",
            token,
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()
        ));

        let mut header = Vec::with_capacity(44);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&((36 + data_size) as u32).to_le_bytes());
        header.extend_from_slice(b"WAVE");
        header.extend_from_slice(b"fmt ");
        header.extend_from_slice(&16u32.to_le_bytes());
        header.extend_from_slice(&1u16.to_le_bytes());
        header.extend_from_slice(&2u16.to_le_bytes());
        header.extend_from_slice(&target_rate.to_le_bytes());
        header.extend_from_slice(&(target_rate * 6).to_le_bytes());
        header.extend_from_slice(&6u16.to_le_bytes());
        header.extend_from_slice(&24u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&(data_size as u32).to_le_bytes());

        {
            use std::io::{Seek, SeekFrom, Write};
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(&temp_file_path)?;
            file.set_len(total_file_bytes)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&header)?;
            file.flush()?;
        }

        let gf = if target_rate != src_rate {
            StageResampler::build(target_rate as f64, src_rate as f64, &opts.filter_phase)
        } else {
            None
        };
        let gf = Arc::new(gf);

        let dither_type = opts.dither.as_deref().and_then(DitherType::from_str_opt).unwrap_or(DitherType::Tpdf);
        let mut ditherer = roo_dsp::StereoPcmDitherer::with_sample_rate(target_rate as f64);
        let initial_frames = out_frames.min(target_rate as usize * 3);
        let mut initial_bytes = vec![0u8; initial_frames * 6];
        if initial_frames > 0 {
            render_pcm_range_helper(&left, &right, gf.as_ref().as_ref(), target_rate, src_rate, ratio, 0, initial_frames, dither_type, &mut ditherer, &mut initial_bytes);
            use std::io::{Seek, SeekFrom, Write};
            let mut file = std::fs::OpenOptions::new().read(true).write(true).open(&temp_file_path)?;
            file.seek(SeekFrom::Start(44))?;
            file.write_all(&initial_bytes)?;
            file.flush()?;
        }

        let bytes_written = Arc::new(AtomicU64::new(44 + initial_bytes.len() as u64));
        let is_completed = Arc::new(AtomicBool::new(initial_frames >= out_frames));
        let is_cancelled = Arc::new(AtomicBool::new(false));
        let notify = Arc::new(tokio::sync::Notify::new());

        let session = Arc::new(ProgressiveSession {
            token: token.to_string(),
            temp_file_path: temp_file_path.clone(),
            mime_type: "audio/wav",
            total_bytes: total_file_bytes,
            bytes_written: bytes_written.clone(),
            is_completed: is_completed.clone(),
            is_cancelled: is_cancelled.clone(),
            notify: notify.clone(),
            is_temp_file: true,
        });

        PROGRESSIVE_SESSIONS.write().unwrap().insert(token.to_string(), session.clone());

        if initial_frames < out_frames {
            let bg_file_path = temp_file_path.clone();
            let bg_bytes_written = bytes_written;
            let bg_is_completed = is_completed;
            let bg_is_cancelled = is_cancelled;
            let bg_notify = notify;
            let bg_left = left.clone();
            let bg_right = right.clone();
            let bg_gf = gf.clone();

            std::thread::spawn(move || {
                use std::io::{Seek, SeekFrom, Write};
                let Ok(mut file) = std::fs::OpenOptions::new().read(true).write(true).open(&bg_file_path) else {
                    return;
                };
                let start_offset = 44 + (initial_frames * 6) as u64;
                if file.seek(SeekFrom::Start(start_offset)).is_err() {
                    return;
                }

                let batch_frames = 16384usize;
                let mut curr_frame = initial_frames;
                let mut batch_buf = vec![0u8; batch_frames * 6];

                while curr_frame < out_frames {
                    if bg_is_cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    let frames_to_run = (out_frames - curr_frame).min(batch_frames);
                    let slice = &mut batch_buf[..frames_to_run * 6];
                    render_pcm_range_helper(&bg_left, &bg_right, bg_gf.as_ref().as_ref(), target_rate, src_rate, ratio, curr_frame, curr_frame + frames_to_run, dither_type, &mut ditherer, slice);
                    if file.write_all(slice).is_err() {
                        return;
                    }
                    let _ = file.flush();
                    curr_frame += frames_to_run;
                    bg_bytes_written.store(44 + (curr_frame * 6) as u64, Ordering::Release);
                    bg_notify.notify_waiters();
                }
                bg_is_completed.store(true, Ordering::Release);
                bg_notify.notify_waiters();
            });
        }

        Ok(session)
    }
}

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
    #[serde(default)]
    pub dither: Option<String>,
}

impl DspStreamOptions {
    pub fn cache_key(&self) -> String {
        let peq = self.peq_json.as_deref().unwrap_or("").trim();
        let conv = self.conv_json.as_deref().unwrap_or("").trim();
        let dsd = self.dsd_mode.as_deref().unwrap_or("").trim();
        let ultra = self.ultrasonic_filter.as_deref().unwrap_or("").trim();
        let dop = self.dop.unwrap_or(false);
        let dither = self.dither.as_deref().unwrap_or("").trim();

        format!(
            "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            self.file_path,
            (self.start_sec * 10.0).round() as i64,
            self.target_rate,
            dsd,
            self.dsd_order,
            self.filter_phase,
            ultra,
            peq,
            conv,
            dop,
            dither
        )
    }
}

fn default_rate() -> u32 { 0 }
fn default_order() -> u8 { 5 }
fn default_phase() -> String { "precise-linear".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeqConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub pre_gain_db: f64,
    #[serde(default)]
    pub bands: Vec<PeqBand>,
}

#[derive(Debug, Deserialize)]
pub struct ConvConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(rename = "irPath", alias = "ir_path", default)]
    pub ir_path: Option<String>,
}

fn apply_convolution_if_configured(
    conv_json: &Option<String>,
    src_rate: u32,
    left: &mut Vec<f64>,
    right: &mut Vec<f64>,
) {
    let conv_str = match conv_json {
        Some(s) if !s.trim().is_empty() && s.trim() != "null" => s.trim(),
        _ => return,
    };
    let conv_conf: ConvConfig = match serde_json::from_str(conv_str) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[Convolution] Failed to parse conv_json: {}", e);
            return;
        }
    };
    if !conv_conf.enabled {
        return;
    }
    let ir_path = match conv_conf.ir_path {
        Some(p) if !p.trim().is_empty() => p,
        _ => return,
    };

    match read_wav_pcm(&ir_path, 0.0, 30.0) {
        Ok((ir_sr, mut ir_l, mut ir_r)) => {
            if ir_l.is_empty() || ir_r.is_empty() {
                tracing::warn!("[Convolution] IR file has 0 samples: {}", ir_path);
                return;
            }
            if ir_sr != src_rate {
                tracing::info!(
                    "[Convolution] Resampling IR '{}' from {} Hz -> {} Hz",
                    ir_path, ir_sr, src_rate
                );
                ir_l = resample_ir_to_rate(&ir_l, ir_sr as f64, src_rate as f64);
                ir_r = resample_ir_to_rate(&ir_r, ir_sr as f64, src_rate as f64);
            }
            let peak = ir_response_peak(&ir_l).max(ir_response_peak(&ir_r));
            if peak > 1.0 {
                let scale = 1.0 / peak;
                for x in ir_l.iter_mut() { *x *= scale; }
                for x in ir_r.iter_mut() { *x *= scale; }
                tracing::info!(
                    "[Convolution] Auto-attenuated IR by {:.2} dB (peak={:.3}) to prevent clipping",
                    -20.0 * peak.log10(),
                    peak
                );
            }
            if let Some(mut engine) = ConvolutionEngine::new(src_rate as f64, &ir_l, &ir_r) {
                let (out_l, out_r) = engine.apply_stereo_aligned(left, right);
                *left = out_l;
                *right = out_r;
                tracing::info!(
                    "[Convolution] Applied IR '{}' ({} taps @ {} Hz) to {} stereo frames",
                    ir_path, ir_l.len(), src_rate, left.len()
                );
            }
        }
        Err(e) => {
            tracing::warn!("[Convolution] Failed to read IR '{}', bypassing: {}", ir_path, e);
        }
    }
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

    // 2.5 Apply Convolution (IR) if configured
    apply_convolution_if_configured(&opts.conv_json, src_rate, &mut left, &mut right);

    // Apply ultrasonic noise protection filter on input if configured (e.g. 20k, 30k, 40k, 50k)
    if let Some(mode) = &opts.ultrasonic_filter {
        if let Some(coeffs) = roo_dsp::build_ultrasonic_filter_coeffs(src_rate as f64, mode) {
            roo_dsp::apply_ultrasonic_filter(&coeffs, &mut left, &mut right);
        }
    }

    // 3. Check if DSD modulation requested
    let is_dsd = opts.dsd_mode.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false);

    if is_dsd {

        let dsd_mode_str = opts.dsd_mode.as_ref().unwrap();
        let use_dop = opts.dop.unwrap_or(false);

        let dsd_multiplier: u32 = match dsd_mode_str.to_lowercase().as_str() {
            "dsd128" | "dsd256" | "dsd512" if use_dop => 128, // Clamp DLNA DoP to DSD128 (352.8k/384k)
            "dsd128" => 128,
            "dsd256" => 256,
            "dsd512" => 512,
            _ => 64,
        };
        let dsd_rate: u32 = 44100 * dsd_multiplier;

        let ratio = dsd_rate as f64 / src_rate as f64;
        let inv_ratio = 1.0 / ratio;

        let dither_type = opts.dither.as_deref().and_then(DitherType::from_str_opt).unwrap_or(DitherType::Gauss);
        let stage1_rate: u32 = 352800;
        let stage1_gf = if src_rate < stage1_rate {
            StageResampler::build(stage1_rate as f64, src_rate as f64, &opts.filter_phase)
        } else {
            None
        };
        let stage1_ratio = stage1_rate as f64 / src_rate as f64;
        let stage2_ratio: usize = (dsd_rate / stage1_rate) as usize;
        let stage2_inv_ratio = 1.0 / stage2_ratio as f64;

        let mut modulator = DsdModulator::new(opts.dsd_order, &opts.filter_phase, dither_type, None);
        modulator.set_dsd_rate(dsd_rate);

        let total_dsd_samples = (left.len() as f64 * ratio).round() as usize;
        let samples_per_block = 32768usize;
        let num_blocks = (total_dsd_samples + samples_per_block - 1) / samples_per_block;

        let data_bytes = num_blocks * 4096 * 2;
        let mut block_data = vec![0u8; data_bytes];
        if let Some(gf) = stage1_gf.as_ref() {
            let batch_blocks = 32usize;
            let mut curr_block = 0usize;
            while curr_block < num_blocks {
                let blocks_to_run = (num_blocks - curr_block).min(batch_blocks);
                let start_bit = curr_block * samples_per_block;
                let num_bits = blocks_to_run * samples_per_block;
                let start_s1 = start_bit / stage2_ratio;
                let num_s1 = num_bits / stage2_ratio;
                let pad_start = start_s1.saturating_sub(1);
                let lead_pad = start_s1 - pad_start;
                let pad_end = start_s1 + num_s1 + 3;
                let (s1_l, s1_r) = gf.upsample_range_stereo(&left, &right, stage1_ratio, pad_start, pad_end);
                let slice = &mut block_data[curr_block * 8192..(curr_block + blocks_to_run) * 8192];
                modulator.run_blocks(&s1_l, &s1_r, s1_l.len(), lead_pad * stage2_ratio, blocks_to_run, stage2_inv_ratio, slice);
                curr_block += blocks_to_run;
            }
        } else {
            modulator.run_blocks(&left, &right, left.len(), 0, num_blocks, inv_ratio, &mut block_data);
        }

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
            let out_frames = (left.len() as f64 * ratio).floor() as usize;

            let resampler = StageResampler::build(target_rate as f64, src_rate as f64, &opts.filter_phase);
            if let Some(res) = resampler {
                res.upsample_range_stereo(&left, &right, ratio, 0, out_frames)
            } else {
                let mut l_vec = Vec::with_capacity(out_frames);
                let mut r_vec = Vec::with_capacity(out_frames);
                for i in 0..out_frames {
                    l_vec.push(left[i.min(left.len() - 1)]);
                    r_vec.push(right[i.min(right.len() - 1)]);
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

        // 24-bit PCM samples with dither
        let dither_type = opts.dither.as_deref().and_then(DitherType::from_str_opt).unwrap_or(DitherType::Tpdf);
        let mut ditherer = roo_dsp::StereoPcmDitherer::with_sample_rate(target_rate as f64);

        for i in 0..out_frames {
            let (i_l, i_r) = ditherer.quantize_24bit(up_l[i], up_r[i], dither_type);
            let bytes_l = i_l.to_le_bytes();
            out_buf.push(bytes_l[0]);
            out_buf.push(bytes_l[1]);
            out_buf.push(bytes_l[2]);

            let bytes_r = i_r.to_le_bytes();
            out_buf.push(bytes_r[0]);
            out_buf.push(bytes_r[1]);
            out_buf.push(bytes_r[2]);
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
            filter_phase: "precise-linear".to_string(),
            peq_json: None,
            conv_json: None,
            ultrasonic_filter: Some("30k".to_string()),
            dop: Some(true),
            dither: None,
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
            filter_phase: "precise-linear".to_string(),
            peq_json: None,
            conv_json: None,
            ultrasonic_filter: Some("30k".to_string()),
            dop: Some(false),
            dither: None,
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

    #[tokio::test]
    async fn test_progressive_dsd_stream() {
        let temp_wav = std::env::temp_dir().join("test_prog_dsd.wav");
        let num_samples = 44100usize * 3; // 3 seconds
        let mut wav_data = Vec::new();
        wav_data.extend_from_slice(b"RIFF");
        let data_size = (num_samples * 4) as u32;
        wav_data.extend_from_slice(&((36 + data_size) as u32).to_le_bytes());
        wav_data.extend_from_slice(b"WAVE");
        wav_data.extend_from_slice(b"fmt ");
        wav_data.extend_from_slice(&16u32.to_le_bytes());
        wav_data.extend_from_slice(&1u16.to_le_bytes());
        wav_data.extend_from_slice(&2u16.to_le_bytes());
        wav_data.extend_from_slice(&44100u32.to_le_bytes());
        wav_data.extend_from_slice(&(44100u32 * 4).to_le_bytes());
        wav_data.extend_from_slice(&4u16.to_le_bytes());
        wav_data.extend_from_slice(&16u16.to_le_bytes());
        wav_data.extend_from_slice(b"data");
        wav_data.extend_from_slice(&data_size.to_le_bytes());
        for i in 0..num_samples {
            let val = ((i as f64 * 440.0 * 2.0 * std::f64::consts::PI / 44100.0).sin() * 8000.0) as i16;
            wav_data.extend_from_slice(&val.to_le_bytes());
            wav_data.extend_from_slice(&val.to_le_bytes());
        }
        std::fs::write(&temp_wav, &wav_data).unwrap();

        let opts = DspStreamOptions {
            file_path: temp_wav.to_string_lossy().to_string(),
            start_sec: 0.0,
            target_rate: 0,
            dsd_mode: Some("dsd64".to_string()),
            dsd_order: 5,
            filter_phase: "precise-linear".to_string(),
            peq_json: None,
            conv_json: None,
            ultrasonic_filter: Some("30k".to_string()),
            dop: Some(false),
            dither: None,
        };

        let t0 = std::time::Instant::now();
        let session = start_progressive_stream(&opts, "test_token_123").expect("Failed to start progressive stream");
        let init_time = t0.elapsed();
        println!("start_progressive_stream took: {:?}", init_time);
        let max_allowed = if cfg!(debug_assertions) { 4000 } else { 500 };
        assert!(init_time.as_millis() < max_allowed, "Progressive stream init took too long: {:?}", init_time);
        assert_eq!(session.mime_type, "audio/x-dsf");
        assert!(session.bytes_written.load(Ordering::Acquire) >= 92);

        // Verify session is retrievable
        let fetched = get_progressive_session("test_token_123");
        assert!(fetched.is_some());

        // Wait brief moment for background writer
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(session.bytes_written.load(Ordering::Acquire) >= 92);

        // Clean up
        cancel_progressive_stream("test_token_123");
        let _ = std::fs::remove_file(temp_wav);
    }
}

