//! Multi-Zone AirPlay device manager.
//! Decodes any audio format (FLAC/WAV/ALAC/MP3/DSF), applies PEQ & Convolution (IR),
//! resamples to AirPlay's native 44.1 kHz / 16-bit lossless ALAC with TPDF dither,
//! and manages independent per-device RAOP streaming sessions.

use crate::alac::RAOP_SAMPLE_RATE;
use crate::mdns::{discover_airplay_devices, AirPlayDevice};
use crate::raop::{start_raop_stream, RaopControlHandle};
use anyhow::{bail, Result};
use rand::Rng;
use roo_decoder::{read_pcm_samples, read_wav_pcm};
use roo_dsp::{
    ir_response_peak, resample_ir_to_rate, ConvolutionEngine, PeqBand, PeqChain,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};

#[derive(Debug, Deserialize)]
struct PeqConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(rename = "preGainDb", default)]
    pre_gain_db: f64,
    #[serde(default)]
    bands: Vec<PeqBand>,
}

#[derive(Debug, Deserialize)]
struct ConvConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(rename = "irPath", alias = "ir_path", default)]
    ir_path: Option<String>,
}

#[derive(Clone)]
pub struct AirPlayManager {
    devices: Arc<RwLock<Vec<AirPlayDevice>>>,
    sessions: Arc<RwLock<HashMap<String, Arc<RaopControlHandle>>>>,
    volumes: Arc<RwLock<HashMap<String, u32>>>,
}

impl Default for AirPlayManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AirPlayManager {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(Vec::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            volumes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn scan(&self, timeout_ms: u64) -> Vec<AirPlayDevice> {
        let found = discover_airplay_devices(timeout_ms);
        *self.devices.write().unwrap() = found.clone();
        found
    }

    pub fn set_devices(&self, list: Vec<AirPlayDevice>) {
        *self.devices.write().unwrap() = list;
    }

    pub fn get_devices(&self) -> Vec<AirPlayDevice> {
        self.devices.read().unwrap().clone()
    }

    pub fn get_device(&self, device_id: &str) -> Option<AirPlayDevice> {
        self.devices
            .read()
            .unwrap()
            .iter()
            .find(|d| d.id == device_id)
            .cloned()
    }

    pub fn stop(&self, device_id: &str) {
        if let Some(handle) = self.sessions.write().unwrap().remove(device_id) {
            handle.is_stopped.store(true, Ordering::Release);
        }
    }

    pub fn stop_all(&self) {
        let handles: Vec<Arc<RaopControlHandle>> = {
            let mut map = self.sessions.write().unwrap();
            map.drain().map(|(_, h)| h).collect()
        };
        for h in handles {
            h.is_stopped.store(true, Ordering::Release);
        }
    }

    pub fn pause(&self, device_id: &str) -> Result<()> {
        if let Some(handle) = self.sessions.read().unwrap().get(device_id).cloned() {
            handle.is_paused.store(true, Ordering::Release);
            let seq = handle.current_seq.load(Ordering::Relaxed) as u16;
            let rtp = handle.current_rtptime.load(Ordering::Relaxed);
            if let Ok(mut rtsp) = handle.rtsp.lock() {
                let _ = rtsp.flush(seq, rtp);
            }
        }
        Ok(())
    }

    pub fn resume(&self, device_id: &str) -> Result<()> {
        if let Some(handle) = self.sessions.read().unwrap().get(device_id).cloned() {
            handle.is_paused.store(false, Ordering::Release);
        }
        Ok(())
    }

    pub fn seek(&self, device_id: &str, seconds: f64) -> Result<()> {
        if let Some(handle) = self.sessions.read().unwrap().get(device_id).cloned() {
            let target_frame = (seconds.max(0.0) * RAOP_SAMPLE_RATE as f64).round() as u64;
            handle.seek_target_frame.store(target_frame, Ordering::Release);
            handle.seek_requested.store(true, Ordering::Release);
        }
        Ok(())
    }

    pub fn set_volume(&self, device_id: &str, volume_pct: u32) -> Result<()> {
        let clamped = volume_pct.min(100);
        self.volumes
            .write()
            .unwrap()
            .insert(device_id.to_string(), clamped);
        if let Some(handle) = self.sessions.read().unwrap().get(device_id).cloned() {
            if let Ok(mut rtsp) = handle.rtsp.lock() {
                let _ = rtsp.set_volume(clamped);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn play(
        &self,
        device_id: &str,
        file_path: &str,
        start_sec: f64,
        title: &str,
        artist: &str,
        album: &str,
        peq_json: Option<&str>,
        conv_json: Option<&str>,
        on_progress: Arc<dyn Fn(f64, f64, &str) + Send + Sync>,
        on_ended: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<()> {
        let dev = match self.get_device(device_id) {
            Some(d) => d,
            None => bail!("AirPlay device not found: {}", device_id),
        };

        // Stop only THIS AirPlay device's prior session (other zones stay active!)
        self.stop(device_id);

        // 1. Decode track to 64-bit float stereo PCM
        let (src_rate, mut left, mut right) = read_pcm_samples(file_path, 0.0, 3600.0)?;
        if left.is_empty() {
            bail!("No audio samples decoded from {}", file_path);
        }

        // 2. Apply PEQ if active
        if let Some(peq_str) = peq_json {
            if !peq_str.trim().is_empty() && peq_str.trim() != "null" {
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

        // 3. Apply Convolution (IR) if active
        if let Some(conv_str) = conv_json {
            if !conv_str.trim().is_empty() && conv_str.trim() != "null" {
                if let Ok(conv_conf) = serde_json::from_str::<ConvConfig>(conv_str) {
                    if conv_conf.enabled {
                        if let Some(ir_path) = conv_conf.ir_path.filter(|p| !p.trim().is_empty()) {
                            if let Ok((ir_sr, mut ir_l, mut ir_r)) = read_wav_pcm(&ir_path, 0.0, 30.0) {
                                if !ir_l.is_empty() && !ir_r.is_empty() {
                                    if ir_sr != src_rate {
                                        ir_l = resample_ir_to_rate(&ir_l, ir_sr as f64, src_rate as f64);
                                        ir_r = resample_ir_to_rate(&ir_r, ir_sr as f64, src_rate as f64);
                                    }
                                    let peak = ir_response_peak(&ir_l).max(ir_response_peak(&ir_r));
                                    if peak > 1.0 {
                                        let s = 1.0 / peak;
                                        for x in ir_l.iter_mut() { *x *= s; }
                                        for x in ir_r.iter_mut() { *x *= s; }
                                    }
                                    if let Some(mut eng) = ConvolutionEngine::new(src_rate as f64, &ir_l, &ir_r) {
                                        let (ol, or) = eng.apply_stereo_aligned(&left, &right);
                                        left = ol;
                                        right = or;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. Anti-aliased windowed-sinc resample to 44.1 kHz if source sample rate differs
        if src_rate != RAOP_SAMPLE_RATE {
            let ratio = RAOP_SAMPLE_RATE as f64 / src_rate as f64;
            let inv_ratio = src_rate as f64 / RAOP_SAMPLE_RATE as f64;
            let out_len = ((left.len() as f64) * ratio).round() as usize;
            let cutoff = ratio.min(1.0);
            let half_win: isize = 16;
            let mut rl = Vec::with_capacity(out_len);
            let mut rr = Vec::with_capacity(out_len);
            let in_len = left.len();
            for n in 0..out_len {
                let pos = n as f64 * inv_ratio;
                let center = pos.floor() as isize;
                let mut acc_l = 0.0f64;
                let mut acc_r = 0.0f64;
                let mut w_sum = 0.0f64;
                for m in (center - half_win + 1)..=(center + half_win) {
                    if m < 0 || (m as usize) >= in_len {
                        continue;
                    }
                    let x = pos - m as f64;
                    let arg = std::f64::consts::PI * cutoff * x;
                    let sinc = if arg.abs() < 1e-12 { 1.0 } else { arg.sin() / arg };
                    let u = x / (half_win as f64);
                    let win = if u.abs() >= 1.0 {
                        0.0
                    } else {
                        0.5 * (1.0 + (std::f64::consts::PI * u).cos())
                    };
                    let w = sinc * win;
                    acc_l += left[m as usize] * w;
                    acc_r += right[m as usize] * w;
                    w_sum += w;
                }
                let norm = if w_sum.abs() > 1e-12 { 1.0 / w_sum } else { 1.0 };
                rl.push(acc_l * norm);
                rr.push(acc_r * norm);
            }
            left = rl;
            right = rr;
        }

        // 5. Quantize to 16-bit signed PCM with TPDF dither
        let n = left.len().min(right.len());
        let mut left_i16 = Vec::with_capacity(n);
        let mut right_i16 = Vec::with_capacity(n);
        let mut rng = rand::thread_rng();
        let lsb = 1.0 / 32768.0;
        for i in 0..n {
            let d1: f64 = (rng.gen::<f64>() - rng.gen::<f64>()) * lsb;
            let d2: f64 = (rng.gen::<f64>() - rng.gen::<f64>()) * lsb;
            let sl = ((left[i] + d1).clamp(-1.0, 0.999969482421875) * 32768.0).round() as i16;
            let sr = ((right[i] + d2).clamp(-1.0, 0.999969482421875) * 32768.0).round() as i16;
            left_i16.push(sl);
            right_i16.push(sr);
        }

        let init_vol = self.volumes.read().unwrap().get(device_id).copied().unwrap_or(100);
        let handle = start_raop_stream(
            dev.ip,
            dev.port,
            Arc::new(left_i16),
            Arc::new(right_i16),
            start_sec,
            init_vol,
            title.to_string(),
            artist.to_string(),
            album.to_string(),
            on_progress,
            on_ended,
        )?;

        self.sessions
            .write()
            .unwrap()
            .insert(device_id.to_string(), handle);

        Ok(())
    }
}
