use std::f64::consts::PI;
use std::sync::{Arc, Mutex};
use rustfft::{num_complex::Complex, Fft, FftPlanner};

use crate::fir::{bessel_i0, build_precise_linear_filter, fir_sample, FirFilter};

/// Half-length of each polyphase branch in input-rate samples.
/// Total taps per phase = 2 * 32767 + 1 = 65,535.
/// At 16x oversampling (e.g. 44.1k -> 705.6k), total prototype length = 1,048,545 (~1 Million taps).
pub const SINC_M_HALF_IN: i32 = 32767;
pub const SINC_M_TAPS_PER_PHASE: usize = (2 * SINC_M_HALF_IN + 1) as usize;
pub const SINC_M_BLOCK_IN: usize = 65536;
pub const SINC_M_FFT_SIZE: usize = 131072; // 2 * SINC_M_BLOCK_IN > (T - 1) + B
const SINC_M_AS_DB: f64 = 175.0;
const SINC_M_FC_RATIO: f64 = 0.4994; // 22,023.5 Hz at 44.1 kHz (flat to 22.00 kHz, < -175 dB at 22.05 kHz)

struct CachedFftBlock {
    block_idx: usize,
    src_ptr: usize,
    src_len: usize,
    out_l: Vec<f64>,
    out_r: Vec<f64>,
}

/// Exact integer-ratio Partitioned FFT Overlap-Save Polyphase Sinc-M Resampler.
/// Packs stereo L/R into a single complex FFT (`L + i*R`) for 2x throughput.
pub struct SincMIntegerStage {
    p: usize,
    phase_spectra: Vec<Vec<Complex<f64>>>,
    fft_fwd: Arc<dyn Fft<f64>>,
    fft_inv: Arc<dyn Fft<f64>>,
    cache: Mutex<Option<CachedFftBlock>>,
}

impl SincMIntegerStage {
    pub fn new(p: usize) -> Self {
        assert!(p >= 2 && p <= 32, "SincMIntegerStage requires integer ratio 2..=32, got {}", p);

        let mut planner = FftPlanner::<f64>::new();
        let fft_fwd = planner.plan_fft_forward(SINC_M_FFT_SIZE);
        let fft_inv = planner.plan_fft_inverse(SINC_M_FFT_SIZE);

        let beta = 0.1102 * (SINC_M_AS_DB - 8.7);
        let i0_beta = bessel_i0(beta);
        let l = SINC_M_HALF_IN as f64 * p as f64;
        let inv_fft_n = 1.0 / SINC_M_FFT_SIZE as f64;

        let num_workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, p);
        let chunk_size = (p + num_workers - 1) / num_workers;
        let scratch_len = fft_fwd.get_inplace_scratch_len();

        let mut phase_spectra: Vec<Vec<Complex<f64>>> = vec![Vec::new(); p];
        std::thread::scope(|s| {
            let mut handles = Vec::with_capacity(num_workers);
            for w_idx in 0..num_workers {
                let phi_start = w_idx * chunk_size;
                if phi_start >= p {
                    break;
                }
                let phi_end = (phi_start + chunk_size).min(p);
                let fft_fwd_ref = &fft_fwd;
                handles.push(s.spawn(move || {
                    let mut taps = vec![0.0f64; SINC_M_TAPS_PER_PHASE];
                    let mut scratch = vec![Complex::new(0.0, 0.0); scratch_len];
                    let mut local = Vec::with_capacity(phi_end - phi_start);
                    for phi in phi_start..phi_end {
                        let mut sum = 0.0f64;
                        for r in 0..SINC_M_TAPS_PER_PHASE {
                            let k = r as i32 - SINC_M_HALF_IN;
                            let t = (k * p as i32 + phi as i32) as f64;
                            let u = t / l;
                            if u.abs() < 1.0 {
                                let x = 2.0 * SINC_M_FC_RATIO * t / p as f64;
                                let sinc = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
                                let w = bessel_i0(beta * (1.0 - u * u).max(0.0).sqrt()) / i0_beta;
                                let v = (2.0 * SINC_M_FC_RATIO) * sinc * w;
                                taps[r] = v;
                                sum += v;
                            } else {
                                taps[r] = 0.0;
                            }
                        }

                        // Normalize each phase to exact unity DC gain and fold 1/N IFFT scaling into H_phi
                        let scale = if sum.abs() > 1e-12 { inv_fft_n / sum } else { inv_fft_n };
                        let mut spec = vec![Complex::new(0.0, 0.0); SINC_M_FFT_SIZE];
                        for r in 0..SINC_M_TAPS_PER_PHASE {
                            spec[r].re = taps[r] * scale;
                        }
                        fft_fwd_ref.process_with_scratch(&mut spec, &mut scratch);
                        local.push((phi, spec));
                    }
                    local
                }));
            }
            for h in handles {
                for (phi, spec) in h.join().unwrap() {
                    phase_spectra[phi] = spec;
                }
            }
        });

        Self {
            p,
            phase_spectra,
            fft_fwd,
            fft_inv,
            cache: Mutex::new(None),
        }
    }

    pub fn ratio(&self) -> usize {
        self.p
    }

    pub fn total_taps(&self) -> usize {
        2 * (SINC_M_HALF_IN as usize * self.p) + 1
    }

    fn compute_block(&self, left: &[f64], right: &[f64], block_idx: usize) -> (Vec<f64>, Vec<f64>) {
        let in_len = left.len().min(right.len());
        let n0 = block_idx * SINC_M_BLOCK_IN;

        // Pack L + i*R into a single complex input buffer for 2x FFT efficiency
        let mut x_buf = vec![Complex::new(0.0, 0.0); SINC_M_FFT_SIZE];
        for j in 0..SINC_M_FFT_SIZE {
            let src_i = n0 as isize - SINC_M_HALF_IN as isize + j as isize;
            if src_i >= 0 && (src_i as usize) < in_len {
                let idx = src_i as usize;
                x_buf[j] = Complex::new(left[idx], right[idx]);
            }
        }
        self.fft_fwd.process(&mut x_buf);

        let p = self.p;
        let valid_offset = 2 * SINC_M_HALF_IN as usize;
        let num_workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, p);
        let chunk_size = (p + num_workers - 1) / num_workers;
        let inv_scratch_len = self.fft_inv.get_inplace_scratch_len();

        // Compute all polyphase IFFTs in parallel across CPU cores (zero false sharing)
        let mut phase_outputs: Vec<Vec<Complex<f64>>> = vec![Vec::new(); p];
        std::thread::scope(|s| {
            let x_ref = &x_buf;
            let spectra_ref = &self.phase_spectra;
            let fft_inv_ref = &self.fft_inv;
            let mut handles = Vec::with_capacity(num_workers);

            for w_idx in 0..num_workers {
                let phi_start = w_idx * chunk_size;
                if phi_start >= p {
                    break;
                }
                let phi_end = (phi_start + chunk_size).min(p);
                handles.push(s.spawn(move || {
                    let mut work = vec![Complex::new(0.0, 0.0); SINC_M_FFT_SIZE];
                    let mut scratch = vec![Complex::new(0.0, 0.0); inv_scratch_len];
                    let mut local = Vec::with_capacity(phi_end - phi_start);
                    for phi in phi_start..phi_end {
                        let h_spec = &spectra_ref[phi];
                        for i in 0..SINC_M_FFT_SIZE {
                            work[i] = x_ref[i] * h_spec[i];
                        }
                        fft_inv_ref.process_with_scratch(&mut work, &mut scratch);
                        local.push((phi, work[valid_offset..valid_offset + SINC_M_BLOCK_IN].to_vec()));
                    }
                    local
                }));
            }

            for h in handles {
                for (phi, buf) in h.join().unwrap() {
                    phase_outputs[phi] = buf;
                }
            }
        });

        // Cache-friendly sequential interleave into stereo output arrays
        let out_block_len = SINC_M_BLOCK_IN * p;
        let mut out_l = vec![0.0f64; out_block_len];
        let mut out_r = vec![0.0f64; out_block_len];
        for d in 0..SINC_M_BLOCK_IN {
            let base = d * p;
            for phi in 0..p {
                let c = phase_outputs[phi][d];
                out_l[base + phi] = c.re;
                out_r[base + phi] = c.im;
            }
        }

        (out_l, out_r)
    }

    /// Upsample stereo `[start_out..end_out]` with exact zero-latency linear-phase alignment.
    pub fn upsample_range_stereo(
        &self,
        left: &[f64],
        right: &[f64],
        start_out: usize,
        end_out: usize,
    ) -> (Vec<f64>, Vec<f64>) {
        let count = end_out.saturating_sub(start_out);
        if count == 0 {
            return (Vec::new(), Vec::new());
        }

        let mut res_l = vec![0.0f64; count];
        let mut res_r = vec![0.0f64; count];
        let block_out_len = SINC_M_BLOCK_IN * self.p;
        let src_ptr = left.as_ptr() as usize;
        let src_len = left.len().min(right.len());

        let first_block = start_out / block_out_len;
        let last_block = (end_out - 1) / block_out_len;

        for blk in first_block..=last_block {
            let blk_start_out = blk * block_out_len;
            let blk_end_out = blk_start_out + block_out_len;
            let copy_start = start_out.max(blk_start_out);
            let copy_end = end_out.min(blk_end_out);
            let copy_len = copy_end - copy_start;
            let src_off = copy_start - blk_start_out;
            let dst_off = copy_start - start_out;

            // Check 1-block cache first so small progressive chunks never recompute FFTs
            let mut hit = false;
            if let Ok(guard) = self.cache.lock() {
                if let Some(cached) = guard.as_ref() {
                    if cached.block_idx == blk && cached.src_ptr == src_ptr && cached.src_len == src_len {
                        res_l[dst_off..dst_off + copy_len].copy_from_slice(&cached.out_l[src_off..src_off + copy_len]);
                        res_r[dst_off..dst_off + copy_len].copy_from_slice(&cached.out_r[src_off..src_off + copy_len]);
                        hit = true;
                    }
                }
            }

            if !hit {
                let (blk_l, blk_r) = self.compute_block(left, right, blk);
                res_l[dst_off..dst_off + copy_len].copy_from_slice(&blk_l[src_off..src_off + copy_len]);
                res_r[dst_off..dst_off + copy_len].copy_from_slice(&blk_r[src_off..src_off + copy_len]);
                if let Ok(mut guard) = self.cache.lock() {
                    *guard = Some(CachedFftBlock {
                        block_idx: blk,
                        src_ptr,
                        src_len,
                        out_l: blk_l,
                        out_r: blk_r,
                    });
                }
            }
        }

        (res_l, res_r)
    }
}

/// Complete `sinc-M` Partitioned FFT Polyphase Resampler.
/// - For integer upsampling ratios `2..=32` (44.1k->88.2/176.4/352.8/705.6k, 48k->96/192/384/768k, etc.),
///   runs single-stage exact FFT polyphase convolution (up to 1,048,545 taps at 16x).
/// - For non-integer upsampling ratios (e.g. 48k->352.8k DSD or 44.1k->192k),
///   runs 4x FFT `sinc-M` brickwall Stage 1 followed by a high-precision Stage 2 fractional FIR.
pub struct SincMFftResampler {
    pub fs_out: f64,
    pub src_rate: f64,
    pub ratio: f64,
    stage1: SincMIntegerStage,
    stage2_fir: Option<FirFilter>,
    stage2_ratio: f64,
}

impl SincMFftResampler {
    /// Returns `Some(SincMFftResampler)` for upsampling (`fs_out > src_rate`), or `None` for 1:1 / downsampling.
    pub fn new(fs_out: f64, src_rate: f64) -> Option<Self> {
        if fs_out <= src_rate * 1.0001 {
            return None;
        }
        let ratio = fs_out / src_rate;
        let rounded = ratio.round();
        let is_integer = (ratio - rounded).abs() < 1e-6 && rounded >= 2.0 && rounded <= 32.0;

        if is_integer {
            let p = rounded as usize;
            Some(Self {
                fs_out,
                src_rate,
                ratio,
                stage1: SincMIntegerStage::new(p),
                stage2_fir: None,
                stage2_ratio: 1.0,
            })
        } else {
            // Non-integer upsampling: 4x sinc-M brickwall stage1 + precise fractional stage2
            let p1 = 4usize;
            let s1_rate = src_rate * p1 as f64;
            let stage2_fir = build_precise_linear_filter(fs_out, s1_rate);
            Some(Self {
                fs_out,
                src_rate,
                ratio,
                stage1: SincMIntegerStage::new(p1),
                stage2_fir: Some(stage2_fir),
                stage2_ratio: fs_out / s1_rate,
            })
        }
    }

    pub fn total_taps(&self) -> usize {
        self.stage1.total_taps()
    }

    pub fn is_single_stage_integer(&self) -> bool {
        self.stage2_fir.is_none()
    }

    /// Upsample stereo output frames `[start_out..end_out]`.
    pub fn upsample_range_stereo(
        &self,
        left: &[f64],
        right: &[f64],
        start_out: usize,
        end_out: usize,
    ) -> (Vec<f64>, Vec<f64>) {
        if let Some(fir2) = &self.stage2_fir {
            let count = end_out.saturating_sub(start_out);
            if count == 0 {
                return (Vec::new(), Vec::new());
            }
            let r2 = self.stage2_ratio;
            let margin = ((fir2.hi.max(-fir2.lo) as f64) / r2).ceil() as usize + 16;
            let s1_first = ((start_out as f64) / r2).floor() as usize;
            let s1_last = ((end_out as f64) / r2).ceil() as usize + 1;
            let s1_start = s1_first.saturating_sub(margin);
            let s1_end = s1_last + margin;

            let (s1_l, s1_r) = self.stage1.upsample_range_stereo(left, right, s1_start, s1_end);
            let s1_len = s1_l.len();

            // Shift output index into local s1 slice coordinates:
            // global s1 position = out_idx / r2; local s1 position = out_idx / r2 - s1_start.
            // We evaluate fir_sample using exact global-to-local shift via fractional index:
            let mut out_l = vec![0.0f64; count];
            let mut out_r = vec![0.0f64; count];
            let s1_start_out_offset = (s1_start as f64 * r2).round() as usize;
            // For exact fractional phase alignment, evaluate directly around t_local = out_idx / r2 - s1_start
            let _ = s1_start_out_offset;
            for (dst_i, out_idx) in (start_out..end_out).enumerate() {
                let t_s1_local = (out_idx as f64) / r2 - s1_start as f64;
                out_l[dst_i] = fir_sample_at_pos(&s1_l, s1_len, t_s1_local, r2, fir2);
                out_r[dst_i] = fir_sample_at_pos(&s1_r, s1_len, t_s1_local, r2, fir2);
            }
            (out_l, out_r)
        } else {
            self.stage1.upsample_range_stereo(left, right, start_out, end_out)
        }
    }
}

/// Helper to evaluate a `FirFilter` at an arbitrary local fractional input position `t_in`.
#[inline]
fn fir_sample_at_pos(pcm: &[f64], in_len: usize, t_in: f64, ratio: f64, gf: &FirFilter) -> f64 {
    if t_in < 0.0 {
        return fir_sample(pcm, in_len, 0, ratio, gf);
    }
    let h = &gf.h;
    let s = gf.s as f64;
    let half = (gf.hi - gf.lo) as f64 * 0.5;
    let lo_f = gf.lo as f64;
    let hi_f = gf.hi as f64;
    let k_min = (t_in - hi_f / ratio).ceil() as i32;
    let k_max = (t_in - lo_f / ratio).floor() as i32;
    let mut acc = 0.0;
    let h_len = h.len();
    for k in k_min..=k_max {
        if k < 0 || k as usize >= in_len {
            continue;
        }
        let d = (t_in - k as f64) * ratio;
        let idx_f = (d + half) * s;
        if idx_f < 0.0 {
            continue;
        }
        let idx = idx_f as usize;
        if idx + 1 >= h_len {
            continue;
        }
        let frac = idx_f - idx as f64;
        // 4-point Catmull-Rom cubic interpolation of the oversampled FIR table (O(1/s^4) accuracy, < -130 dB)
        let w = if idx >= 1 && idx + 2 < h_len {
            let y0 = h[idx - 1];
            let y1 = h[idx];
            let y2 = h[idx + 1];
            let y3 = h[idx + 2];
            let a0 = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
            let a1 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let a2 = -0.5 * y0 + 0.5 * y2;
            let a3 = y1;
            ((a0 * frac + a1) * frac + a2) * frac + a3
        } else {
            h[idx] + frac * (h[idx + 1] - h[idx])
        };
        acc += pcm[k as usize] * w;
    }
    acc
}

/// Check whether a filter phase ID selects the Partitioned FFT `sinc-M` engine.
#[inline]
pub fn is_sinc_m_phase(phase_id: &str) -> bool {
    matches!(phase_id, "sinc-m" | "sinc-M" | "hqp-sinc-m")
}
