use std::f64::consts::PI;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use crate::fir::{fir_sample, get_hermite_tab, interpolate_linear, interpolate_minimum, interpolate_intermediate, FirFilter, PolyphaseBank};

pub static GAUSS_DITHER_TABLE: std::sync::LazyLock<[f64; 65536]> = std::sync::LazyLock::new(|| {
    let mut table = [0.0f64; 65536];
    let mut rng = rand::rngs::StdRng::seed_from_u64(0x123456789abcdef0);
    for i in (0..65536).step_by(2) {
        let u1: f64 = rng.gen::<f64>().max(1e-12);
        let u2: f64 = rng.gen::<f64>();
        let r = (-2.0 * u1.ln()).sqrt() * 1e-5;
        let theta = 2.0 * PI * u2;
        table[i] = r * theta.cos();
        table[i + 1] = r * theta.sin();
    }
    table
});

pub static TPDF_DITHER_TABLE: std::sync::LazyLock<[f64; 65536]> = std::sync::LazyLock::new(|| {
    let mut table = [0.0f64; 65536];
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xabcdef0123456789);
    for i in 0..65536 {
        table[i] = (rng.gen::<f64>() - rng.gen::<f64>()) * 1e-5;
    }
    table
});

pub const DSD_BLOCK_SIZE: usize = 4096;
pub const DSD_SAMPLES_PER_BLOCK: usize = DSD_BLOCK_SIZE * 8; // 32768 samples per channel block

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DsdMode {
    Dsd64,
    Dsd128,
    Dsd256,
    Dsd512,
}

impl DsdMode {
    pub fn carrier_rate(&self, base_rate: u32) -> u32 {
        let multiplier = match self {
            DsdMode::Dsd64 => 64,
            DsdMode::Dsd128 => 128,
            DsdMode::Dsd256 => 256,
            DsdMode::Dsd512 => 512,
        };
        let base = if base_rate % 48000 == 0 { 48000 } else { 44100 };
        base * multiplier
    }

    pub fn rate_hz(&self) -> u32 {
        match self {
            DsdMode::Dsd64 => 2822400,
            DsdMode::Dsd128 => 5644800,
            DsdMode::Dsd256 => 11289600,
            DsdMode::Dsd512 => 22579200,
        }
    }
}

pub fn get_dsd_rate(mode: &str) -> u32 {
    match mode.to_lowercase().as_str() {
        "dsd128" => 5644800,
        "dsd256" => 11289600,
        "dsd512" => 22579200,
        _ => 2822400,
    }
}

pub const CIFFR_5_C: [f64; 5] = [0.5, 0.5, 0.4, 0.3, 0.15];
pub const CIFFR_5_SCALE: f64 = 0.50;
pub const CIFFR_5_CLAMP: f64 = 6.0;

#[derive(Clone, Copy)]
pub struct Ciff5Coeffs {
    pub g1: f64,
    pub g2: f64,
    pub a: [f64; 5],
}

pub fn get_ciff5_params(inv_ratio: f64) -> Ciff5Coeffs {
    let ratio = 1.0 / inv_ratio;
    if ratio >= 384.0 {
        // dsd512
        Ciff5Coeffs {
            g1: 2.6753e-5,
            g2: 1.8678e-4,
            a: [1.167550, 0.882446, 0.571927, 0.330361, 0.211115],
        }
    } else if ratio >= 192.0 {
        // dsd256
        Ciff5Coeffs {
            g1: 1.0701e-4,
            g2: 7.4710e-4,
            a: [1.167598, 0.882338, 0.571526, 0.329978, 0.210111],
        }
    } else if ratio >= 96.0 {
        // dsd128
        Ciff5Coeffs {
            g1: 4.2805e-4,
            g2: 2.9883e-3,
            a: [1.167792, 0.881907, 0.569920, 0.328447, 0.206106],
        }
    } else {
        // dsd64
        Ciff5Coeffs {
            g1: 1.7121e-3,
            g2: 1.1952e-2,
            a: [1.168565, 0.880190, 0.563485, 0.322320, 0.190215],
        }
    }
}

pub const CIFFR_7_C: [f64; 7] = [0.4, 0.4, 0.3, 0.3, 0.2, 0.2, 0.1];
pub const CIFFR_7_SCALE: f64 = 0.48;
pub const CIFFR_7_CLAMP: f64 = 8.0;

#[derive(Clone, Copy)]
pub struct Ciff7Coeffs {
    pub g1: f64,
    pub g2: f64,
    pub g3: f64,
    pub a: [f64; 7],
}

pub fn get_ciff7_params(inv_ratio: f64) -> Ciff7Coeffs {
    let ratio = 1.0 / inv_ratio;
    if ratio >= 384.0 {
        // dsd512
        Ciff7Coeffs {
            g1: 1.9437e-5,
            g2: 9.4667e-5,
            g3: 2.9441e-4,
            a: [1.388919, 1.223895, 1.003589, 0.595969, 0.380464, 0.162117, 0.072051],
        }
    } else if ratio >= 192.0 {
        // dsd256
        Ciff7Coeffs {
            g1: 7.7748e-5,
            g2: 3.7867e-4,
            g3: 1.1776e-3,
            a: [1.389009, 1.223628, 1.002543, 0.594685, 0.378470, 0.161060, 0.070423],
        }
    } else if ratio >= 96.0 {
        // dsd128
        Ciff7Coeffs {
            g1: 3.1099e-4,
            g2: 1.5146e-3,
            g3: 4.7103e-3,
            a: [1.389371, 1.222558, 0.998359, 0.589548, 0.370519, 0.156853, 0.064007],
        }
    } else {
        // dsd64
        Ciff7Coeffs {
            g1: 1.2439e-3,
            g2: 6.0581e-3,
            g3: 1.8839e-2,
            a: [1.390815, 1.218295, 0.981571, 0.568986, 0.339084, 0.140386, 0.039812],
        }
    }
}

pub fn build_dsf_header(total_file_bytes: u64, dsd_rate: u32, total_samples_per_channel: u64, block_size: u32) -> Vec<u8> {
    let mut header = vec![0u8; 92];

    // 1. 'DSD ' Chunk (28 bytes)
    header[0..4].copy_from_slice(b"DSD ");
    header[4..12].copy_from_slice(&28u64.to_le_bytes());
    header[12..20].copy_from_slice(&total_file_bytes.to_le_bytes());
    header[20..28].copy_from_slice(&0u64.to_le_bytes()); // metadata offset

    // 2. 'fmt ' Chunk (52 bytes)
    header[28..32].copy_from_slice(b"fmt ");
    header[32..40].copy_from_slice(&52u64.to_le_bytes());
    header[40..44].copy_from_slice(&1u32.to_le_bytes()); // format version = 1
    header[44..48].copy_from_slice(&0u32.to_le_bytes()); // format id = 0 (raw DSD)
    header[48..52].copy_from_slice(&2u32.to_le_bytes()); // channel type = 2 (stereo)
    header[52..56].copy_from_slice(&2u32.to_le_bytes()); // num channels = 2
    header[56..60].copy_from_slice(&dsd_rate.to_le_bytes()); // sample rate
    header[60..64].copy_from_slice(&1u32.to_le_bytes()); // bits per sample = 1
    header[64..72].copy_from_slice(&total_samples_per_channel.to_le_bytes());
    header[72..76].copy_from_slice(&block_size.to_le_bytes()); // block size = 4096
    header[76..80].copy_from_slice(&0u32.to_le_bytes()); // reserved

    // 3. 'data' Chunk (12 bytes)
    header[80..84].copy_from_slice(b"data");
    let data_chunk_size = 12u64 + (total_file_bytes.saturating_sub(92));
    header[84..92].copy_from_slice(&data_chunk_size.to_le_bytes());

    header
}

#[derive(Debug, Clone)]
pub struct UltrasonicFilterCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

pub fn build_ultrasonic_filter_coeffs(sample_rate: f64, mode: &str) -> Option<UltrasonicFilterCoeffs> {
    let fc = match mode.to_lowercase().as_str() {
        "20k" => 20000.0,
        "30k" => 30000.0,
        "40k" => 40000.0,
        "50k" => 50000.0,
        _ => return None,
    };
    if fc >= 0.45 * sample_rate {
        return None;
    }

    let q = 0.7071067811865475;
    let w0 = 2.0 * PI * fc / sample_rate;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q);

    let b0 = (1.0 - cos_w0) / 2.0;
    let b1 = 1.0 - cos_w0;
    let b2 = (1.0 - cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    Some(UltrasonicFilterCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    })
}

pub fn apply_ultrasonic_filter(coeffs: &UltrasonicFilterCoeffs, left: &mut [f64], right: &mut [f64]) {
    let b0 = coeffs.b0;
    let b1 = coeffs.b1;
    let b2 = coeffs.b2;
    let a1 = coeffs.a1;
    let a2 = coeffs.a2;

    let mut x1_l = 0.0; let mut x2_l = 0.0; let mut y1_l = 0.0; let mut y2_l = 0.0;
    let mut x1_r = 0.0; let mut x2_r = 0.0; let mut y1_r = 0.0; let mut y2_r = 0.0;

    let len = left.len().min(right.len());
    for i in 0..len {
        let xl = left[i];
        let yl = b0 * xl + b1 * x1_l + b2 * x2_l - a1 * y1_l - a2 * y2_l;
        x2_l = x1_l; x1_l = xl; y2_l = y1_l; y1_l = yl;
        left[i] = yl;

        let xr = right[i];
        let yr = b0 * xr + b1 * x1_r + b2 * x2_r - a1 * y1_r - a2 * y2_r;
        x2_r = x1_r; x1_r = xr; y2_r = y1_r; y1_r = yr;
        right[i] = yr;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DitherType {
    None,
    Tpdf,
    Gauss1,
}

pub struct DsdModulator {
    pub order: u8, // 5 or 7
    pub dither_type: DitherType,
    pub gf: Option<FirFilter>,
    pub poly_phases: Option<PolyphaseBank>,
    pub phase_name: String,

    // CIFF5 state
    pub s1_l: f64, pub s2_l: f64, pub s3_l: f64, pub s4_l: f64, pub s5_l: f64, pub y_l: f64,
    pub s1_r: f64, pub s2_r: f64, pub s3_r: f64, pub s4_r: f64, pub s5_r: f64, pub y_r: f64,

    // CIFF7 extra state
    pub s6_l: f64, pub s7_l: f64,
    pub s6_r: f64, pub s7_r: f64,
}

impl DsdModulator {
    pub fn new(order: u8, phase_name: &str, dither: DitherType, gf: Option<FirFilter>) -> Self {
        let poly_phases = gf.as_ref().and_then(|f| crate::fir::build_fir_phases(f));
        Self {
            order: if order == 7 { 7 } else { 5 },
            dither_type: dither,
            gf,
            poly_phases,
            phase_name: phase_name.to_string(),
            s1_l: 0.0, s2_l: 0.0, s3_l: 0.0, s4_l: 0.0, s5_l: 0.0, y_l: 0.0,
            s1_r: 0.0, s2_r: 0.0, s3_r: 0.0, s4_r: 0.0, s5_r: 0.0, y_r: 0.0,
            s6_l: 0.0, s7_l: 0.0,
            s6_r: 0.0, s7_r: 0.0,
        }
    }

    pub fn reset_state(&mut self) {
        self.s1_l = 0.0; self.s2_l = 0.0; self.s3_l = 0.0; self.s4_l = 0.0; self.s5_l = 0.0; self.y_l = 0.0;
        self.s1_r = 0.0; self.s2_r = 0.0; self.s3_r = 0.0; self.s4_r = 0.0; self.s5_r = 0.0; self.y_r = 0.0;
        self.s6_l = 0.0; self.s7_l = 0.0;
        self.s6_r = 0.0; self.s7_r = 0.0;
    }

    /// Process input PCM stereo arrays (length `in_len`), modulating `num_blocks` DSF channel blocks (4096 bytes each).
    /// Output buffer format: interleaved block pairs (4096 bytes Left, 4096 bytes Right) for each block.
    pub fn run_blocks(
        &mut self,
        left: &[f64],
        right: &[f64],
        in_len: usize,
        start_bit: usize,
        num_blocks: usize,
        inv_ratio: f64,
        out: &mut [u8],
    ) {
        if self.order == 7 {
            self.run_blocks7(left, right, in_len, start_bit, num_blocks, inv_ratio, out);
        } else {
            self.run_blocks5(left, right, in_len, start_bit, num_blocks, inv_ratio, out);
        }
    }

    fn run_blocks5(
        &mut self,
        left: &[f64],
        right: &[f64],
        in_len: usize,
        start_bit: usize,
        num_blocks: usize,
        inv_ratio: f64,
        out: &mut [u8],
    ) {
        let p = get_ciff5_params(inv_ratio);
        let c1 = CIFFR_5_C[0]; let c2 = CIFFR_5_C[1]; let c3 = CIFFR_5_C[2]; let c4 = CIFFR_5_C[3]; let c5 = CIFFR_5_C[4];
        let g1 = p.g1; let g2 = p.g2;
        let a1 = p.a[0]; let a2 = p.a[1]; let a3 = p.a[2]; let a4 = p.a[3]; let a5 = p.a[4];
        let scale = CIFFR_5_SCALE;
        let clamp = CIFFR_5_CLAMP;

        let mut s1_l = self.s1_l; let mut s2_l = self.s2_l; let mut s3_l = self.s3_l; let mut s4_l = self.s4_l; let mut s5_l = self.s5_l; let mut y_l = self.y_l;
        let mut s1_r = self.s1_r; let mut s2_r = self.s2_r; let mut s3_r = self.s3_r; let mut s4_r = self.s4_r; let mut s5_r = self.s5_r; let mut y_r = self.y_r;

        let ratio = 1.0 / inv_ratio;
        let is_exact_frac = (ratio - ratio.round()).abs() < 1e-9;
        let w_period = if is_exact_frac && ratio >= 1.0 && ratio <= 1024.0 { ratio.round() as usize } else { 0 };
        let w_tab = if w_period > 0 { Some(get_hermite_tab(w_period)) } else { None };

        let use_fir = self.gf.is_some();
        let dither_mode = self.dither_type;
        let gauss_table = if dither_mode == DitherType::Gauss1 { Some(&*GAUSS_DITHER_TABLE) } else { None };
        let tpdf_table = if dither_mode == DitherType::Tpdf { Some(&*TPDF_DITHER_TABLE) } else { None };
        let mut dither_idx: usize = (start_bit * 137) & 0xffff;

        let mut cur_idx = 0;
        let mut cur_phi = 0;
        if w_period > 0 {
            cur_idx = start_bit / w_period;
            cur_phi = start_bit - cur_idx * w_period;
        }

        let mut p1_l = if cur_idx < in_len { left[cur_idx] } else { 0.0 };
        let mut p2_l = if cur_idx + 1 < in_len { left[cur_idx + 1] } else { 0.0 };
        let mut p0_l = if cur_idx > 0 && cur_idx - 1 < in_len { left[cur_idx - 1] } else { p1_l };
        let mut p3_l = if cur_idx + 2 < in_len { left[cur_idx + 2] } else { p2_l };
        let mut m1_l = 0.5 * (p2_l - p0_l);
        let mut m2_l = 0.5 * (p3_l - p1_l);

        let mut p1_r = if cur_idx < in_len { right[cur_idx] } else { 0.0 };
        let mut p2_r = if cur_idx + 1 < in_len { right[cur_idx + 1] } else { 0.0 };
        let mut p0_r = if cur_idx > 0 && cur_idx - 1 < in_len { right[cur_idx - 1] } else { p1_r };
        let mut p3_r = if cur_idx + 2 < in_len { right[cur_idx + 2] } else { p2_r };
        let mut m1_r = 0.5 * (p2_r - p0_r);
        let mut m2_r = 0.5 * (p3_r - p1_r);

        for b in 0..num_blocks {
            let left_off = b * 2 * DSD_BLOCK_SIZE;
            let right_off = left_off + DSD_BLOCK_SIZE;
            let start_sample_idx = start_bit + b * DSD_SAMPLES_PER_BLOCK;

            for byte_idx in 0..DSD_BLOCK_SIZE {
                let mut byte_val_l = 0u8;
                let mut byte_val_r = 0u8;
                let base_sample = start_sample_idx + (byte_idx << 3);

                for bit in 0..8 {
                    let out_idx = base_sample + bit;
                    let (raw_l, raw_r) = if use_fir {
                        if let (Some(gf), Some(phases)) = (&self.gf, &self.poly_phases) {
                            let phi = out_idx % phases.p;
                            let entry = &phases.entries[phi];
                            let k_base = entry.k0 + (out_idx / phases.p) as i32 * phases.q as i32;
                            let t = entry.t;
                            if k_base >= 0 && (k_base as usize + t) <= in_len {
                                let mut acc_l = 0.0;
                                let mut acc_r = 0.0;
                                for j in 0..t {
                                    let k = (k_base as usize) + j;
                                    acc_l += left[k] * entry.g[j];
                                    acc_r += right[k] * entry.g[j];
                                }
                                (acc_l, acc_r)
                            } else {
                                (fir_sample(left, in_len, out_idx, ratio, gf), fir_sample(right, in_len, out_idx, ratio, gf))
                            }
                        } else if let Some(gf) = &self.gf {
                            (fir_sample(left, in_len, out_idx, ratio, gf), fir_sample(right, in_len, out_idx, ratio, gf))
                        } else {
                            (0.0, 0.0)
                        }
                    } else if let Some(tab) = &w_tab {
                        let o = cur_phi * 4;
                        cur_phi += 1;
                        if cur_phi == w_period {
                            cur_phi = 0;
                            cur_idx += 1;
                            p1_l = if cur_idx < in_len { left[cur_idx] } else { 0.0 };
                            p2_l = if cur_idx + 1 < in_len { left[cur_idx + 1] } else { 0.0 };
                            p0_l = if cur_idx > 0 && cur_idx - 1 < in_len { left[cur_idx - 1] } else { p1_l };
                            p3_l = if cur_idx + 2 < in_len { left[cur_idx + 2] } else { p2_l };
                            m1_l = 0.5 * (p2_l - p0_l);
                            m2_l = 0.5 * (p3_l - p1_l);

                            p1_r = if cur_idx < in_len { right[cur_idx] } else { 0.0 };
                            p2_r = if cur_idx + 1 < in_len { right[cur_idx + 1] } else { 0.0 };
                            p0_r = if cur_idx > 0 && cur_idx - 1 < in_len { right[cur_idx - 1] } else { p1_r };
                            p3_r = if cur_idx + 2 < in_len { right[cur_idx + 2] } else { p2_r };
                            m1_r = 0.5 * (p2_r - p0_r);
                            m2_r = 0.5 * (p3_r - p1_r);
                        }

                        let y_l_interp = tab[o] * p1_l + tab[o + 1] * m1_l + tab[o + 2] * p2_l + tab[o + 3] * m2_l;
                        let y_r_interp = tab[o] * p1_r + tab[o + 1] * m1_r + tab[o + 2] * p2_r + tab[o + 3] * m2_r;

                        (y_l_interp.clamp(-1.0, 1.0), y_r_interp.clamp(-1.0, 1.0))
                    } else {
                        let t_src = out_idx as f64 * inv_ratio;
                        let idx = t_src.floor() as usize;
                        let frac = t_src - idx as f64;

                        let p1_l = if idx < in_len { left[idx] } else { 0.0 };
                        let p2_l = if idx + 1 < in_len { left[idx + 1] } else { 0.0 };
                        let p0_l = if idx > 0 && idx - 1 < in_len { left[idx - 1] } else { p1_l };
                        let p3_l = if idx + 2 < in_len { left[idx + 2] } else { p2_l };

                        let p1_r = if idx < in_len { right[idx] } else { 0.0 };
                        let p2_r = if idx + 1 < in_len { right[idx + 1] } else { 0.0 };
                        let p0_r = if idx > 0 && idx - 1 < in_len { right[idx - 1] } else { p1_r };
                        let p3_r = if idx + 2 < in_len { right[idx + 2] } else { p2_r };

                        match self.phase_name.as_str() {
                            "minimum" => (interpolate_minimum(p0_l, p1_l, p2_l, p3_l, frac), interpolate_minimum(p0_r, p1_r, p2_r, p3_r, frac)),
                            "intermediate" => (interpolate_intermediate(p0_l, p1_l, p2_l, p3_l, frac), interpolate_intermediate(p0_r, p1_r, p2_r, p3_r, frac)),
                            _ => (interpolate_linear(p0_l, p1_l, p2_l, p3_l, frac), interpolate_linear(p0_r, p1_r, p2_r, p3_r, frac)),
                        }
                    };

                    // CIFF5 modulator equations with anti-leak coefficient 0.999999
                    let u_l = raw_l * scale;
                    s1_l = (s1_l * 0.999999 + c1 * (u_l - y_l)).clamp(-clamp, clamp);
                    s2_l = (s2_l * 0.999999 + c2 * s1_l - g1 * s3_l).clamp(-clamp, clamp);
                    s3_l = (s3_l * 0.999999 + c3 * s2_l).clamp(-clamp, clamp);
                    s4_l = (s4_l * 0.999999 + c4 * s3_l - g2 * s5_l).clamp(-clamp, clamp);
                    s5_l = (s5_l * 0.999999 + c5 * s4_l).clamp(-clamp, clamp);

                    let (dither_l, dither_r) = if let Some(gtab) = gauss_table {
                        let dl = gtab[dither_idx];
                        let dr = gtab[(dither_idx + 1) & 0xffff];
                        dither_idx = (dither_idx + 2) & 0xffff;
                        (dl, dr)
                    } else if let Some(ttab) = tpdf_table {
                        let dl = ttab[dither_idx];
                        let dr = ttab[(dither_idx + 1) & 0xffff];
                        dither_idx = (dither_idx + 2) & 0xffff;
                        (dl, dr)
                    } else {
                        (0.0, 0.0)
                    };

                    let sigma_l = u_l + a1 * s1_l + a2 * s2_l + a3 * s3_l + a4 * s4_l + a5 * s5_l + dither_l;
                    y_l = if sigma_l >= 0.0 { 1.0 } else { -1.0 };
                    if y_l > 0.0 { byte_val_l |= 1 << bit; }

                    let u_r = raw_r * scale;
                    s1_r = (s1_r * 0.999999 + c1 * (u_r - y_r)).clamp(-clamp, clamp);
                    s2_r = (s2_r * 0.999999 + c2 * s1_r - g1 * s3_r).clamp(-clamp, clamp);
                    s3_r = (s3_r * 0.999999 + c3 * s2_r).clamp(-clamp, clamp);
                    s4_r = (s4_r * 0.999999 + c4 * s3_r - g2 * s5_r).clamp(-clamp, clamp);
                    s5_r = (s5_r * 0.999999 + c5 * s4_r).clamp(-clamp, clamp);

                    let sigma_r = u_r + a1 * s1_r + a2 * s2_r + a3 * s3_r + a4 * s4_r + a5 * s5_r + dither_r;
                    y_r = if sigma_r >= 0.0 { 1.0 } else { -1.0 };
                    if y_r > 0.0 { byte_val_r |= 1 << bit; }
                }

                out[left_off + byte_idx] = byte_val_l;
                out[right_off + byte_idx] = byte_val_r;
            }
        }

        self.s1_l = s1_l; self.s2_l = s2_l; self.s3_l = s3_l; self.s4_l = s4_l; self.s5_l = s5_l; self.y_l = y_l;
        self.s1_r = s1_r; self.s2_r = s2_r; self.s3_r = s3_r; self.s4_r = s4_r; self.s5_r = s5_r; self.y_r = y_r;
    }

    fn run_blocks7(
        &mut self,
        left: &[f64],
        right: &[f64],
        in_len: usize,
        start_bit: usize,
        num_blocks: usize,
        inv_ratio: f64,
        out: &mut [u8],
    ) {
        let p = get_ciff7_params(inv_ratio);
        let c1 = CIFFR_7_C[0]; let c2 = CIFFR_7_C[1]; let c3 = CIFFR_7_C[2]; let c4 = CIFFR_7_C[3];
        let c5 = CIFFR_7_C[4]; let c6 = CIFFR_7_C[5]; let c7 = CIFFR_7_C[6];
        let g1 = p.g1; let g2 = p.g2; let g3 = p.g3;
        let a1 = p.a[0]; let a2 = p.a[1]; let a3 = p.a[2]; let a4 = p.a[3]; let a5 = p.a[4]; let a6 = p.a[5]; let a7 = p.a[6];
        let scale = CIFFR_7_SCALE;
        let clamp = CIFFR_7_CLAMP;

        let mut s1_l = self.s1_l; let mut s2_l = self.s2_l; let mut s3_l = self.s3_l; let mut s4_l = self.s4_l;
        let mut s5_l = self.s5_l; let mut s6_l = self.s6_l; let mut s7_l = self.s7_l; let mut y_l = self.y_l;

        let mut s1_r = self.s1_r; let mut s2_r = self.s2_r; let mut s3_r = self.s3_r; let mut s4_r = self.s4_r;
        let mut s5_r = self.s5_r; let mut s6_r = self.s6_r; let mut s7_r = self.s7_r; let mut y_r = self.y_r;

        let ratio = 1.0 / inv_ratio;
        let is_exact_frac = (ratio - ratio.round()).abs() < 1e-9;
        let w_period = if is_exact_frac && ratio >= 1.0 && ratio <= 1024.0 { ratio.round() as usize } else { 0 };
        let w_tab = if w_period > 0 { Some(get_hermite_tab(w_period)) } else { None };

        let use_fir = self.gf.is_some();
        let dither_mode = self.dither_type;
        let gauss_table = if dither_mode == DitherType::Gauss1 { Some(&*GAUSS_DITHER_TABLE) } else { None };
        let tpdf_table = if dither_mode == DitherType::Tpdf { Some(&*TPDF_DITHER_TABLE) } else { None };
        let mut dither_idx: usize = (start_bit * 137) & 0xffff;

        let mut cur_idx = 0;
        let mut cur_phi = 0;
        if w_period > 0 {
            cur_idx = start_bit / w_period;
            cur_phi = start_bit - cur_idx * w_period;
        }

        let mut p1_l = if cur_idx < in_len { left[cur_idx] } else { 0.0 };
        let mut p2_l = if cur_idx + 1 < in_len { left[cur_idx + 1] } else { 0.0 };
        let mut p0_l = if cur_idx > 0 && cur_idx - 1 < in_len { left[cur_idx - 1] } else { p1_l };
        let mut p3_l = if cur_idx + 2 < in_len { left[cur_idx + 2] } else { p2_l };
        let mut m1_l = 0.5 * (p2_l - p0_l);
        let mut m2_l = 0.5 * (p3_l - p1_l);

        let mut p1_r = if cur_idx < in_len { right[cur_idx] } else { 0.0 };
        let mut p2_r = if cur_idx + 1 < in_len { right[cur_idx + 1] } else { 0.0 };
        let mut p0_r = if cur_idx > 0 && cur_idx - 1 < in_len { right[cur_idx - 1] } else { p1_r };
        let mut p3_r = if cur_idx + 2 < in_len { right[cur_idx + 2] } else { p2_r };
        let mut m1_r = 0.5 * (p2_r - p0_r);
        let mut m2_r = 0.5 * (p3_r - p1_r);

        for b in 0..num_blocks {
            let left_off = b * 2 * DSD_BLOCK_SIZE;
            let right_off = left_off + DSD_BLOCK_SIZE;
            let start_sample_idx = start_bit + b * DSD_SAMPLES_PER_BLOCK;

            for byte_idx in 0..DSD_BLOCK_SIZE {
                let mut byte_val_l = 0u8;
                let mut byte_val_r = 0u8;
                let base_sample = start_sample_idx + (byte_idx << 3);

                for bit in 0..8 {
                    let out_idx = base_sample + bit;
                    let (raw_l, raw_r) = if use_fir {
                        if let (Some(gf), Some(phases)) = (&self.gf, &self.poly_phases) {
                            let phi = out_idx % phases.p;
                            let entry = &phases.entries[phi];
                            let k_base = entry.k0 + (out_idx / phases.p) as i32 * phases.q as i32;
                            let t = entry.t;
                            if k_base >= 0 && (k_base as usize + t) <= in_len {
                                let mut acc_l = 0.0;
                                let mut acc_r = 0.0;
                                for j in 0..t {
                                    let k = (k_base as usize) + j;
                                    acc_l += left[k] * entry.g[j];
                                    acc_r += right[k] * entry.g[j];
                                }
                                (acc_l, acc_r)
                            } else {
                                (fir_sample(left, in_len, out_idx, ratio, gf), fir_sample(right, in_len, out_idx, ratio, gf))
                            }
                        } else if let Some(gf) = &self.gf {
                            (fir_sample(left, in_len, out_idx, ratio, gf), fir_sample(right, in_len, out_idx, ratio, gf))
                        } else {
                            (0.0, 0.0)
                        }
                    } else if let Some(tab) = &w_tab {
                        let o = cur_phi * 4;
                        cur_phi += 1;
                        if cur_phi == w_period {
                            cur_phi = 0;
                            cur_idx += 1;
                            p1_l = if cur_idx < in_len { left[cur_idx] } else { 0.0 };
                            p2_l = if cur_idx + 1 < in_len { left[cur_idx + 1] } else { 0.0 };
                            p0_l = if cur_idx > 0 && cur_idx - 1 < in_len { left[cur_idx - 1] } else { p1_l };
                            p3_l = if cur_idx + 2 < in_len { left[cur_idx + 2] } else { p2_l };
                            m1_l = 0.5 * (p2_l - p0_l);
                            m2_l = 0.5 * (p3_l - p1_l);

                            p1_r = if cur_idx < in_len { right[cur_idx] } else { 0.0 };
                            p2_r = if cur_idx + 1 < in_len { right[cur_idx + 1] } else { 0.0 };
                            p0_r = if cur_idx > 0 && cur_idx - 1 < in_len { right[cur_idx - 1] } else { p1_r };
                            p3_r = if cur_idx + 2 < in_len { right[cur_idx + 2] } else { p2_r };
                            m1_r = 0.5 * (p2_r - p0_r);
                            m2_r = 0.5 * (p3_r - p1_r);
                        }

                        let y_l_interp = tab[o] * p1_l + tab[o + 1] * m1_l + tab[o + 2] * p2_l + tab[o + 3] * m2_l;
                        let y_r_interp = tab[o] * p1_r + tab[o + 1] * m1_r + tab[o + 2] * p2_r + tab[o + 3] * m2_r;

                        (y_l_interp.clamp(-1.0, 1.0), y_r_interp.clamp(-1.0, 1.0))
                    } else {
                        let t_src = out_idx as f64 * inv_ratio;
                        let idx = t_src.floor() as usize;
                        let frac = t_src - idx as f64;

                        let p1_l = if idx < in_len { left[idx] } else { 0.0 };
                        let p2_l = if idx + 1 < in_len { left[idx + 1] } else { 0.0 };
                        let p0_l = if idx > 0 && idx - 1 < in_len { left[idx - 1] } else { p1_l };
                        let p3_l = if idx + 2 < in_len { left[idx + 2] } else { p2_l };

                        let p1_r = if idx < in_len { right[idx] } else { 0.0 };
                        let p2_r = if idx + 1 < in_len { right[idx + 1] } else { 0.0 };
                        let p0_r = if idx > 0 && idx - 1 < in_len { right[idx - 1] } else { p1_r };
                        let p3_r = if idx + 2 < in_len { right[idx + 2] } else { p2_r };

                        match self.phase_name.as_str() {
                            "minimum" => (interpolate_minimum(p0_l, p1_l, p2_l, p3_l, frac), interpolate_minimum(p0_r, p1_r, p2_r, p3_r, frac)),
                            "intermediate" => (interpolate_intermediate(p0_l, p1_l, p2_l, p3_l, frac), interpolate_intermediate(p0_r, p1_r, p2_r, p3_r, frac)),
                            _ => (interpolate_linear(p0_l, p1_l, p2_l, p3_l, frac), interpolate_linear(p0_r, p1_r, p2_r, p3_r, frac)),
                        }
                    };

                    // CIFF7 modulator equations
                    let u_l = raw_l * scale;
                    s1_l = (s1_l * 0.999999 + c1 * (u_l - y_l)).clamp(-clamp, clamp);
                    s2_l = (s2_l * 0.999999 + c2 * s1_l - g1 * s3_l).clamp(-clamp, clamp);
                    s3_l = (s3_l * 0.999999 + c3 * s2_l).clamp(-clamp, clamp);
                    s4_l = (s4_l * 0.999999 + c4 * s3_l - g2 * s5_l).clamp(-clamp, clamp);
                    s5_l = (s5_l * 0.999999 + c5 * s4_l).clamp(-clamp, clamp);
                    s6_l = (s6_l * 0.999999 + c6 * s5_l - g3 * s7_l).clamp(-clamp, clamp);
                    s7_l = (s7_l * 0.999999 + c7 * s6_l).clamp(-clamp, clamp);

                    let (dither_l, dither_r) = if let Some(gtab) = gauss_table {
                        let dl = gtab[dither_idx];
                        let dr = gtab[(dither_idx + 1) & 0xffff];
                        dither_idx = (dither_idx + 2) & 0xffff;
                        (dl, dr)
                    } else if let Some(ttab) = tpdf_table {
                        let dl = ttab[dither_idx];
                        let dr = ttab[(dither_idx + 1) & 0xffff];
                        dither_idx = (dither_idx + 2) & 0xffff;
                        (dl, dr)
                    } else {
                        (0.0, 0.0)
                    };

                    let sigma_l = u_l + a1 * s1_l + a2 * s2_l + a3 * s3_l + a4 * s4_l + a5 * s5_l + a6 * s6_l + a7 * s7_l + dither_l;
                    y_l = if sigma_l >= 0.0 { 1.0 } else { -1.0 };
                    if y_l > 0.0 { byte_val_l |= 1 << bit; }

                    let u_r = raw_r * scale;
                    s1_r = (s1_r * 0.999999 + c1 * (u_r - y_r)).clamp(-clamp, clamp);
                    s2_r = (s2_r * 0.999999 + c2 * s1_r - g1 * s3_r).clamp(-clamp, clamp);
                    s3_r = (s3_r * 0.999999 + c3 * s2_r).clamp(-clamp, clamp);
                    s4_r = (s4_r * 0.999999 + c4 * s3_r - g2 * s5_r).clamp(-clamp, clamp);
                    s5_r = (s5_r * 0.999999 + c5 * s4_r).clamp(-clamp, clamp);
                    s6_r = (s6_r * 0.999999 + c6 * s5_r - g3 * s7_r).clamp(-clamp, clamp);
                    s7_r = (s7_r * 0.999999 + c7 * s6_r).clamp(-clamp, clamp);

                    let sigma_r = u_r + a1 * s1_r + a2 * s2_r + a3 * s3_r + a4 * s4_r + a5 * s5_r + a6 * s6_r + a7 * s7_r + dither_r;
                    y_r = if sigma_r >= 0.0 { 1.0 } else { -1.0 };
                    if y_r > 0.0 { byte_val_r |= 1 << bit; }
                }

                out[left_off + byte_idx] = byte_val_l;
                out[right_off + byte_idx] = byte_val_r;
            }
        }

        self.s1_l = s1_l; self.s2_l = s2_l; self.s3_l = s3_l; self.s4_l = s4_l;
        self.s5_l = s5_l; self.s6_l = s6_l; self.s7_l = s7_l; self.y_l = y_l;

        self.s1_r = s1_r; self.s2_r = s2_r; self.s3_r = s3_r; self.s4_r = s4_r;
        self.s5_r = s5_r; self.s6_r = s6_r; self.s7_r = s7_r; self.y_r = y_r;
    }
}
