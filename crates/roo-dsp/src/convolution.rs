use rustfft::{FftPlanner, num_complex::Complex64};
use std::sync::Arc;

pub const CONV_FFT_SIZE: usize = 4096;
pub const CONV_BLOCK_SIZE: usize = CONV_FFT_SIZE / 2; // 2048
pub const CONV_MAX_IR_TAPS: usize = 262144;

pub fn next_pow2(n: usize) -> usize {
    if n <= 1 { return 1; }
    let mut p = 1;
    while p < n {
        p <<= 1;
    }
    p
}

pub fn ir_response_peak(ir: &[f64]) -> f64 {
    let taps = ir.len().min(CONV_MAX_IR_TAPS);
    if taps == 0 { return 0.0; }
    let n = next_pow2(2.max(taps));

    let mut buf = vec![Complex64::new(0.0, 0.0); n];
    for i in 0..taps {
        buf[i].re = ir[i];
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    fft.process(&mut buf);

    let mut peak = 0.0f64;
    for i in 0..=(n / 2) {
        let m = buf[i].norm();
        if m > peak {
            peak = m;
        }
    }
    peak
}

pub struct ConvPartition {
    pub h: Vec<Complex64>,
}

pub struct ChannelConv {
    pub parts: Vec<ConvPartition>,
    pub ring: Vec<Vec<Complex64>>,
    pub ring_pos: usize,
    pub window: Vec<f64>, // [prev B | cur B]
    pub fill_count: usize,
    pub out: Vec<f64>,
    pub out_pos: usize,
    pub out_len: usize,
    pub x_buf: Vec<Complex64>,
}

impl ChannelConv {
    pub fn new(parts: Vec<ConvPartition>, k: usize) -> Self {
        let mut padded = parts;
        while padded.len() < k {
            padded.push(ConvPartition { h: vec![Complex64::new(0.0, 0.0); CONV_FFT_SIZE] });
        }
        let ring = vec![vec![Complex64::new(0.0, 0.0); CONV_FFT_SIZE]; k];
        Self {
            parts: padded,
            ring,
            ring_pos: 0,
            window: vec![0.0; CONV_FFT_SIZE],
            fill_count: 0,
            out: vec![0.0; CONV_BLOCK_SIZE],
            out_pos: 0,
            out_len: 0,
            x_buf: vec![Complex64::new(0.0, 0.0); CONV_FFT_SIZE],
        }
    }
}

pub struct ConvolutionEngine {
    pub fs: f64,
    pub k: usize,
    pub left: ChannelConv,
    pub right: ChannelConv,
    fft_fwd: Arc<dyn rustfft::Fft<f64>>,
    fft_inv: Arc<dyn rustfft::Fft<f64>>,
    y_buf: Vec<Complex64>,
}

impl ConvolutionEngine {
    pub fn new(fs: f64, ir_l: &[f64], ir_r: &[f64]) -> Option<Self> {
        if fs <= 0.0 || ir_l.is_empty() || ir_r.is_empty() {
            return None;
        }
        let mut planner = FftPlanner::new();
        let fft_fwd = planner.plan_fft_forward(CONV_FFT_SIZE);
        let fft_inv = planner.plan_fft_inverse(CONV_FFT_SIZE);

        let parts_l = Self::build_partitions(ir_l, &fft_fwd)?;
        let parts_r = Self::build_partitions(ir_r, &fft_fwd)?;
        let k = parts_l.len().max(parts_r.len());

        let left = ChannelConv::new(parts_l, k);
        let right = ChannelConv::new(parts_r, k);

        Some(Self {
            fs,
            k,
            left,
            right,
            fft_fwd,
            fft_inv,
            y_buf: vec![Complex64::new(0.0, 0.0); CONV_FFT_SIZE],
        })
    }

    fn build_partitions(ir: &[f64], fft: &Arc<dyn rustfft::Fft<f64>>) -> Option<Vec<ConvPartition>> {
        let taps = ir.len().min(CONV_MAX_IR_TAPS);
        if taps == 0 { return None; }
        let k = (taps + CONV_BLOCK_SIZE - 1) / CONV_BLOCK_SIZE;
        let mut parts = Vec::with_capacity(k);
        // Fold 1/N IFFT normalization directly into IR spectra at init time
        let inv_n = 1.0 / CONV_FFT_SIZE as f64;

        for step in 0..k {
            let start = step * CONV_BLOCK_SIZE;
            let len = (taps - start).min(CONV_BLOCK_SIZE);
            let mut buf = vec![Complex64::new(0.0, 0.0); CONV_FFT_SIZE];
            for i in 0..len {
                buf[i].re = ir[start + i] * inv_n;
            }
            fft.process(&mut buf);
            parts.push(ConvPartition { h: buf });
        }

        Some(parts)
    }

    fn process_channel_block(
        ch: &mut ChannelConv,
        k_total: usize,
        fft_fwd: &Arc<dyn rustfft::Fft<f64>>,
        fft_inv: &Arc<dyn rustfft::Fft<f64>>,
        y_buf: &mut [Complex64],
    ) {
        for i in 0..CONV_FFT_SIZE {
            ch.x_buf[i] = Complex64::new(ch.window[i], 0.0);
        }
        fft_fwd.process(&mut ch.x_buf);

        ch.ring[ch.ring_pos].copy_from_slice(&ch.x_buf);
        ch.ring_pos = (ch.ring_pos + 1) % k_total;

        let half = CONV_FFT_SIZE / 2;
        for i in 0..=half {
            y_buf[i] = Complex64::new(0.0, 0.0);
        }

        // Real-input conjugate symmetry: accumulate only DC..Nyquist (N/2 + 1 bins),
        // cutting partition multiply-add work in half with 4-way loop unrolling.
        let bins = half + 1;
        for k in 0..k_total {
            let x_idx = (ch.ring_pos + 2 * k_total - 1 - k) % k_total;
            let x = &ch.ring[x_idx][..bins];
            let h = &ch.parts[k].h[..bins];
            let mut i = 0usize;
            while i + 4 <= bins {
                y_buf[i] += x[i] * h[i];
                y_buf[i + 1] += x[i + 1] * h[i + 1];
                y_buf[i + 2] += x[i + 2] * h[i + 2];
                y_buf[i + 3] += x[i + 3] * h[i + 3];
                i += 4;
            }
            while i < bins {
                y_buf[i] += x[i] * h[i];
                i += 1;
            }
        }

        // Mirror conjugate-symmetric upper half once before inverse FFT
        for i in 1..half {
            y_buf[CONV_FFT_SIZE - i] = y_buf[i].conj();
        }

        fft_inv.process(y_buf);

        // Overlap-save: valid linear convolution is the second half (1/N already folded into H)
        for i in 0..CONV_BLOCK_SIZE {
            ch.out[i] = y_buf[CONV_BLOCK_SIZE + i].re;
        }
        ch.out_pos = 0;
        ch.out_len = CONV_BLOCK_SIZE;

        // Slide window: copy cur B to prev B
        ch.window.copy_within(CONV_BLOCK_SIZE..CONV_FFT_SIZE, 0);
    }

    pub fn process_pair(&mut self, x_l: f64, x_r: f64) -> (f64, f64) {
        let b = CONV_BLOCK_SIZE;
        self.left.window[b + self.left.fill_count] = x_l;
        self.right.window[b + self.right.fill_count] = x_r;
        self.left.fill_count += 1;
        self.right.fill_count += 1;

        if self.left.fill_count == b {
            Self::process_channel_block(&mut self.left, self.k, &self.fft_fwd, &self.fft_inv, &mut self.y_buf);
            Self::process_channel_block(&mut self.right, self.k, &self.fft_fwd, &self.fft_inv, &mut self.y_buf);
            self.left.fill_count = 0;
            self.right.fill_count = 0;
        }

        let mut o_l = 0.0;
        let mut o_r = 0.0;
        if self.left.out_pos < self.left.out_len {
            o_l = self.left.out[self.left.out_pos];
            self.left.out_pos += 1;
        }
        if self.right.out_pos < self.right.out_len {
            o_r = self.right.out[self.right.out_pos];
            self.right.out_pos += 1;
        }

        (o_l, o_r)
    }

    pub fn apply_arrays(&mut self, in_l: &[f64], in_r: &[f64], out_l: &mut [f64], out_r: &mut [f64]) {
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        for i in 0..n {
            let (ol, or) = self.process_pair(in_l[i], in_r[i]);
            out_l[i] = ol;
            out_r[i] = or;
        }
    }

    /// Convolve a full stereo buffer with exact latency compensation:
    /// flushes the 1-block (CONV_BLOCK_SIZE - 1) overlap-save pipeline delay
    /// so output sample `i` corresponds directly to linear convolution `y[i]`.
    pub fn apply_stereo_aligned(&mut self, in_l: &[f64], in_r: &[f64]) -> (Vec<f64>, Vec<f64>) {
        let n = in_l.len().min(in_r.len());
        let latency = CONV_BLOCK_SIZE - 1;
        let mut out_l = vec![0.0f64; n];
        let mut out_r = vec![0.0f64; n];

        for i in 0..n {
            let (ol, or) = self.process_pair(in_l[i], in_r[i]);
            if i >= latency {
                out_l[i - latency] = ol;
                out_r[i - latency] = or;
            }
        }
        for j in 0..latency {
            let (ol, or) = self.process_pair(0.0, 0.0);
            let idx = n + j;
            if idx >= latency && (idx - latency) < n {
                out_l[idx - latency] = ol;
                out_r[idx - latency] = or;
            }
        }
        (out_l, out_r)
    }
}

/// Resample an impulse response from `src_fs` to `dst_fs` using a Kaiser-windowed sinc
/// interpolator, scaling amplitude by `src_fs / dst_fs` so frequency-domain gain stays invariant.
pub fn resample_ir_to_rate(ir: &[f64], src_fs: f64, dst_fs: f64) -> Vec<f64> {
    if ir.is_empty() || src_fs <= 0.0 || dst_fs <= 0.0 {
        return Vec::new();
    }
    if (src_fs - dst_fs).abs() < 1e-3 {
        return ir.iter().copied().take(CONV_MAX_IR_TAPS).collect();
    }

    let ratio = dst_fs / src_fs;
    let out_len = ((ir.len() as f64 * ratio).round() as usize).clamp(1, CONV_MAX_IR_TAPS);
    let cutoff = ratio.min(1.0);
    let half_win: isize = 24;
    let inv_ratio = src_fs / dst_fs;
    let gain_scale = inv_ratio * cutoff; // preserves unity passband gain
    let mut out = vec![0.0f64; out_len];

    for n in 0..out_len {
        let pos = n as f64 * inv_ratio;
        let center = pos.floor() as isize;
        let mut acc = 0.0f64;
        for m in (center - half_win + 1)..=(center + half_win) {
            if m < 0 || (m as usize) >= ir.len() {
                continue;
            }
            let x = pos - m as f64;
            let sinc_arg = std::f64::consts::PI * cutoff * x;
            let sinc = if sinc_arg.abs() < 1e-12 {
                1.0
            } else {
                sinc_arg.sin() / sinc_arg
            };
            // Hann window over [-half_win, half_win]
            let u = x / (half_win as f64);
            let win = if u.abs() >= 1.0 {
                0.0
            } else {
                0.5 * (1.0 + (std::f64::consts::PI * u).cos())
            };
            acc += ir[m as usize] * sinc * win;
        }
        out[n] = acc * gain_scale;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_stereo_aligned_zero_latency() {
        let mut ir_l = vec![0.0; 2600];
        ir_l[5] = 0.75;
        ir_l[2500] = -0.25;
        let mut ir_r = vec![0.0; 2600];
        ir_r[0] = 1.0;

        let mut eng = ConvolutionEngine::new(48000.0, &ir_l, &ir_r).unwrap();
        let mut in_l = vec![0.0; 3000];
        let mut in_r = vec![0.0; 3000];
        in_l[0] = 1.0;
        in_r[10] = 0.5;

        let (out_l, out_r) = eng.apply_stereo_aligned(&in_l, &in_r);
        assert!((out_l[5] - 0.75).abs() < 1e-12, "L tap 5: {}", out_l[5]);
        assert!((out_l[2500] - (-0.25)).abs() < 1e-12, "L tap 2500: {}", out_l[2500]);
        assert!((out_r[10] - 0.5).abs() < 1e-12, "R tap 10: {}", out_r[10]);
    }

    #[test]
    fn test_resample_ir_preserves_dc_gain() {
        // Dirac delta at center of 48kHz IR
        let mut ir = vec![0.0; 129];
        ir[64] = 1.0;
        let ir_441 = resample_ir_to_rate(&ir, 48000.0, 44100.0);
        let sum_441: f64 = ir_441.iter().sum();
        assert!((sum_441 - 1.0).abs() < 1e-3, "DC gain at 44.1kHz = {}", sum_441);

        let ir_96 = resample_ir_to_rate(&ir, 48000.0, 96000.0);
        let sum_96: f64 = ir_96.iter().sum();
        assert!((sum_96 - 1.0).abs() < 1e-3, "DC gain at 96kHz = {}", sum_96);
    }
}
