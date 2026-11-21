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

        for step in 0..k {
            let start = step * CONV_BLOCK_SIZE;
            let len = (taps - start).min(CONV_BLOCK_SIZE);
            let mut buf = vec![Complex64::new(0.0, 0.0); CONV_FFT_SIZE];
            for i in 0..len {
                buf[i].re = ir[start + i];
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

        for i in 0..CONV_FFT_SIZE {
            y_buf[i] = Complex64::new(0.0, 0.0);
        }

        for k in 0..k_total {
            let x_idx = (ch.ring_pos + 2 * k_total - 1 - k) % k_total;
            let x = &ch.ring[x_idx];
            let h = &ch.parts[k].h;
            for i in 0..CONV_FFT_SIZE {
                y_buf[i] += x[i] * h[i];
            }
        }

        fft_inv.process(y_buf);
        let inv_n = 1.0 / CONV_FFT_SIZE as f64;

        // Overlap-save: valid linear convolution is the second half
        for i in 0..CONV_BLOCK_SIZE {
            ch.out[i] = y_buf[CONV_BLOCK_SIZE + i].re * inv_n;
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
}
