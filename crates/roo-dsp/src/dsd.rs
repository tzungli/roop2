use std::f64::consts::PI;
use serde::{Deserialize, Serialize};
use crate::fir::{fir_sample, get_hermite_tab, interpolate_linear, interpolate_minimum, FirFilter, PolyphaseBank};
pub use crate::dither::{DitherType, DsdChannelDither, DsdDitherer};

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
    get_ciff5_params_for_osr(1.0 / inv_ratio)
}

pub fn get_ciff5_params_for_osr(ratio: f64) -> Ciff5Coeffs {
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
    get_ciff7_params_for_osr(1.0 / inv_ratio)
}

pub fn get_ciff7_params_for_osr(ratio: f64) -> Ciff7Coeffs {
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
    if fc >= 0.48 * sample_rate {
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

pub struct DsdModulator {
    pub order: u8, // 5 or 7
    pub dither_type: DitherType,
    pub dsd_ditherer: DsdDitherer,
    pub gf: Option<FirFilter>,
    pub poly_phases: Option<PolyphaseBank>,
    pub phase_name: String,
    /// Optional explicit DSD sample rate (e.g. 2822400, 5644800, 11289600, 22579200).
    /// When set, CIFF5/CIFF7 resonator notch coefficients use `dsd_rate / 44100.0` (true OSR)
    /// rather than `dsd_rate / src_rate` (critical for Hi-Res 96k/192k or 2-stage 352.8k input!).
    pub dsd_rate: Option<u32>,

    // CIFF5 state
    pub s1_l: f64, pub s2_l: f64, pub s3_l: f64, pub s4_l: f64, pub s5_l: f64, pub y_l: f64,
    pub s1_r: f64, pub s2_r: f64, pub s3_r: f64, pub s4_r: f64, pub s5_r: f64, pub y_r: f64,

    // CIFF7 extra state
    pub s6_l: f64, pub s7_l: f64,
    pub s6_r: f64, pub s7_r: f64,
}

#[derive(Clone, Copy)]
struct ChanState5 {
    s1: f64, s2: f64, s3: f64, s4: f64, s5: f64, y: f64,
}

#[derive(Clone, Copy)]
struct ChanState7 {
    s1: f64, s2: f64, s3: f64, s4: f64, s5: f64, s6: f64, s7: f64, y: f64,
}

#[inline(always)]
fn load_hermite_window(pcm: &[f64], in_len: usize, cur_idx: usize) -> (f64, f64, f64, f64) {
    let p1 = if cur_idx < in_len { pcm[cur_idx] } else { 0.0 };
    let p2 = if cur_idx + 1 < in_len { pcm[cur_idx + 1] } else { 0.0 };
    let p0 = if cur_idx > 0 && cur_idx - 1 < in_len { pcm[cur_idx - 1] } else { p1 };
    let p3 = if cur_idx + 2 < in_len { pcm[cur_idx + 2] } else { p2 };
    let m1 = 0.5 * (p2 - p0);
    let m2 = 0.5 * (p3 - p1);
    (p1, p2, m1, m2)
}

fn modulate_channel5(
    pcm: &[f64],
    in_len: usize,
    start_bit: usize,
    num_blocks: usize,
    inv_ratio: f64,
    p: Ciff5Coeffs,
    dither_type: DitherType,
    ditherer: &mut DsdChannelDither,
    gf: Option<&FirFilter>,
    poly_phases: Option<&PolyphaseBank>,
    phase_name: &str,
    w_period: usize,
    w_tab: Option<&[f64]>,
    st: &mut ChanState5,
    ch_out: &mut [u8],
) {
    let c1 = CIFFR_5_C[0]; let c2 = CIFFR_5_C[1]; let c3 = CIFFR_5_C[2]; let c4 = CIFFR_5_C[3]; let c5 = CIFFR_5_C[4];
    let g1 = p.g1; let g2 = p.g2;
    let a1 = p.a[0]; let a2 = p.a[1]; let a3 = p.a[2]; let a4 = p.a[3]; let a5 = p.a[4];
    let scale = CIFFR_5_SCALE;
    let clamp = CIFFR_5_CLAMP;

    let mut s1 = st.s1; let mut s2 = st.s2; let mut s3 = st.s3; let mut s4 = st.s4; let mut s5 = st.s5; let mut y = st.y;
    let total_bytes = num_blocks * DSD_BLOCK_SIZE;

    if gf.is_none() {
        if let Some(tab) = w_tab {
            // Fast branch-free Hermite table path
            let mut cur_idx = start_bit / w_period;
            let mut cur_phi = start_bit - cur_idx * w_period;
            let (mut p1, mut p2, mut m1, mut m2) = load_hermite_window(pcm, in_len, cur_idx);

            for byte_idx in 0..total_bytes {
                let mut byte_val = 0u8;
                for bit in 0..8 {
                    let o = cur_phi * 4;
                    let raw = (tab[o] * p1 + tab[o + 1] * m1 + tab[o + 2] * p2 + tab[o + 3] * m2).clamp(-1.0, 1.0);

                    cur_phi += 1;
                    if cur_phi == w_period {
                        cur_phi = 0;
                        cur_idx += 1;
                        let w = load_hermite_window(pcm, in_len, cur_idx);
                        p1 = w.0; p2 = w.1; m1 = w.2; m2 = w.3;
                    }

                    let u = raw * scale;
                    s1 = (s1 * 0.999999 + c1 * (u - y)).clamp(-clamp, clamp);
                    s2 = (s2 * 0.999999 + c2 * s1 - g1 * s3).clamp(-clamp, clamp);
                    s3 = (s3 * 0.999999 + c3 * s2).clamp(-clamp, clamp);
                    s4 = (s4 * 0.999999 + c4 * s3 - g2 * s5).clamp(-clamp, clamp);
                    s5 = (s5 * 0.999999 + c5 * s4).clamp(-clamp, clamp);

                    let dither = ditherer.next_dither(dither_type);
                    let sigma = u + a1 * s1 + a2 * s2 + a3 * s3 + a4 * s4 + a5 * s5 + dither;
                    let is_pos = sigma >= 0.0;
                    y = if is_pos { 1.0 } else { -1.0 };
                    // Sony DSF Spec v1.01 (bits_per_sample = 1): LSB-first (bit 0 = earliest sample)
                    byte_val |= (is_pos as u8) << bit;
                }
                ch_out[byte_idx] = byte_val;
            }

            st.s1 = s1; st.s2 = s2; st.s3 = s3; st.s4 = s4; st.s5 = s5; st.y = y;
            return;
        }
    }

    // General path (FIR or non-integer ratio Hermite)
    let ratio = 1.0 / inv_ratio;
    let is_min_phase = phase_name.contains("min");
    for byte_idx in 0..total_bytes {
        let mut byte_val = 0u8;
        let base_sample = start_bit + (byte_idx << 3);
        for bit in 0..8 {
            let out_idx = base_sample + bit;
            let raw = if let Some(gf_ref) = gf {
                if let Some(phases) = poly_phases {
                    let phi = out_idx % phases.p;
                    let entry = &phases.entries[phi];
                    let k_base = entry.k0 + (out_idx / phases.p) as i32 * phases.q as i32;
                    let t = entry.t;
                    if k_base >= 0 && (k_base as usize + t) <= in_len {
                        let mut acc = 0.0;
                        let start = k_base as usize;
                        for j in 0..t {
                            acc += pcm[start + j] * entry.g[j];
                        }
                        acc
                    } else {
                        fir_sample(pcm, in_len, out_idx, ratio, gf_ref)
                    }
                } else {
                    fir_sample(pcm, in_len, out_idx, ratio, gf_ref)
                }
            } else {
                let t_src = out_idx as f64 * inv_ratio;
                let idx = t_src.floor() as usize;
                let frac = t_src - idx as f64;
                let p1 = if idx < in_len { pcm[idx] } else { 0.0 };
                let p2 = if idx + 1 < in_len { pcm[idx + 1] } else { 0.0 };
                let p0 = if idx > 0 && idx - 1 < in_len { pcm[idx - 1] } else { p1 };
                let p3 = if idx + 2 < in_len { pcm[idx + 2] } else { p2 };
                if is_min_phase {
                    interpolate_minimum(p0, p1, p2, p3, frac)
                } else {
                    interpolate_linear(p0, p1, p2, p3, frac)
                }
            };

            let u = raw * scale;
            s1 = (s1 * 0.999999 + c1 * (u - y)).clamp(-clamp, clamp);
            s2 = (s2 * 0.999999 + c2 * s1 - g1 * s3).clamp(-clamp, clamp);
            s3 = (s3 * 0.999999 + c3 * s2).clamp(-clamp, clamp);
            s4 = (s4 * 0.999999 + c4 * s3 - g2 * s5).clamp(-clamp, clamp);
            s5 = (s5 * 0.999999 + c5 * s4).clamp(-clamp, clamp);

            let dither = ditherer.next_dither(dither_type);
            let sigma = u + a1 * s1 + a2 * s2 + a3 * s3 + a4 * s4 + a5 * s5 + dither;
            let is_pos = sigma >= 0.0;
            y = if is_pos { 1.0 } else { -1.0 };
            // Sony DSF Spec v1.01 (bits_per_sample = 1): LSB-first (bit 0 = earliest sample)
            byte_val |= (is_pos as u8) << bit;
        }
        ch_out[byte_idx] = byte_val;
    }

    st.s1 = s1; st.s2 = s2; st.s3 = s3; st.s4 = s4; st.s5 = s5; st.y = y;
}

fn modulate_channel7(
    pcm: &[f64],
    in_len: usize,
    start_bit: usize,
    num_blocks: usize,
    inv_ratio: f64,
    p: Ciff7Coeffs,
    dither_type: DitherType,
    ditherer: &mut DsdChannelDither,
    gf: Option<&FirFilter>,
    poly_phases: Option<&PolyphaseBank>,
    phase_name: &str,
    w_period: usize,
    w_tab: Option<&[f64]>,
    st: &mut ChanState7,
    ch_out: &mut [u8],
) {
    let c1 = CIFFR_7_C[0]; let c2 = CIFFR_7_C[1]; let c3 = CIFFR_7_C[2]; let c4 = CIFFR_7_C[3];
    let c5 = CIFFR_7_C[4]; let c6 = CIFFR_7_C[5]; let c7 = CIFFR_7_C[6];
    let g1 = p.g1; let g2 = p.g2; let g3 = p.g3;
    let a1 = p.a[0]; let a2 = p.a[1]; let a3 = p.a[2]; let a4 = p.a[3]; let a5 = p.a[4]; let a6 = p.a[5]; let a7 = p.a[6];
    let scale = CIFFR_7_SCALE;
    let clamp = CIFFR_7_CLAMP;

    let mut s1 = st.s1; let mut s2 = st.s2; let mut s3 = st.s3; let mut s4 = st.s4;
    let mut s5 = st.s5; let mut s6 = st.s6; let mut s7 = st.s7; let mut y = st.y;
    let total_bytes = num_blocks * DSD_BLOCK_SIZE;

    if gf.is_none() {
        if let Some(tab) = w_tab {
            // Fast branch-free Hermite table path
            let mut cur_idx = start_bit / w_period;
            let mut cur_phi = start_bit - cur_idx * w_period;
            let (mut p1, mut p2, mut m1, mut m2) = load_hermite_window(pcm, in_len, cur_idx);

            for byte_idx in 0..total_bytes {
                let mut byte_val = 0u8;
                for bit in 0..8 {
                    let o = cur_phi * 4;
                    let raw = (tab[o] * p1 + tab[o + 1] * m1 + tab[o + 2] * p2 + tab[o + 3] * m2).clamp(-1.0, 1.0);

                    cur_phi += 1;
                    if cur_phi == w_period {
                        cur_phi = 0;
                        cur_idx += 1;
                        let w = load_hermite_window(pcm, in_len, cur_idx);
                        p1 = w.0; p2 = w.1; m1 = w.2; m2 = w.3;
                    }

                    let u = raw * scale;
                    s1 = (s1 * 0.999999 + c1 * (u - y)).clamp(-clamp, clamp);
                    s2 = (s2 * 0.999999 + c2 * s1 - g1 * s3).clamp(-clamp, clamp);
                    s3 = (s3 * 0.999999 + c3 * s2).clamp(-clamp, clamp);
                    s4 = (s4 * 0.999999 + c4 * s3 - g2 * s5).clamp(-clamp, clamp);
                    s5 = (s5 * 0.999999 + c5 * s4).clamp(-clamp, clamp);
                    s6 = (s6 * 0.999999 + c6 * s5 - g3 * s7).clamp(-clamp, clamp);
                    s7 = (s7 * 0.999999 + c7 * s6).clamp(-clamp, clamp);

                    let dither = ditherer.next_dither(dither_type);
                    let sigma = u + a1 * s1 + a2 * s2 + a3 * s3 + a4 * s4 + a5 * s5 + a6 * s6 + a7 * s7 + dither;
                    let is_pos = sigma >= 0.0;
                    y = if is_pos { 1.0 } else { -1.0 };
                    // Sony DSF Spec v1.01 (bits_per_sample = 1): LSB-first (bit 0 = earliest sample)
                    byte_val |= (is_pos as u8) << bit;
                }
                ch_out[byte_idx] = byte_val;
            }

            st.s1 = s1; st.s2 = s2; st.s3 = s3; st.s4 = s4;
            st.s5 = s5; st.s6 = s6; st.s7 = s7; st.y = y;
            return;
        }
    }

    // General path (FIR or non-integer ratio Hermite)
    let ratio = 1.0 / inv_ratio;
    let is_min_phase = phase_name.contains("min");
    for byte_idx in 0..total_bytes {
        let mut byte_val = 0u8;
        let base_sample = start_bit + (byte_idx << 3);
        for bit in 0..8 {
            let out_idx = base_sample + bit;
            let raw = if let Some(gf_ref) = gf {
                if let Some(phases) = poly_phases {
                    let phi = out_idx % phases.p;
                    let entry = &phases.entries[phi];
                    let k_base = entry.k0 + (out_idx / phases.p) as i32 * phases.q as i32;
                    let t = entry.t;
                    if k_base >= 0 && (k_base as usize + t) <= in_len {
                        let mut acc = 0.0;
                        let start = k_base as usize;
                        for j in 0..t {
                            acc += pcm[start + j] * entry.g[j];
                        }
                        acc
                    } else {
                        fir_sample(pcm, in_len, out_idx, ratio, gf_ref)
                    }
                } else {
                    fir_sample(pcm, in_len, out_idx, ratio, gf_ref)
                }
            } else {
                let t_src = out_idx as f64 * inv_ratio;
                let idx = t_src.floor() as usize;
                let frac = t_src - idx as f64;
                let p1 = if idx < in_len { pcm[idx] } else { 0.0 };
                let p2 = if idx + 1 < in_len { pcm[idx + 1] } else { 0.0 };
                let p0 = if idx > 0 && idx - 1 < in_len { pcm[idx - 1] } else { p1 };
                let p3 = if idx + 2 < in_len { pcm[idx + 2] } else { p2 };
                if is_min_phase {
                    interpolate_minimum(p0, p1, p2, p3, frac)
                } else {
                    interpolate_linear(p0, p1, p2, p3, frac)
                }
            };

            let u = raw * scale;
            s1 = (s1 * 0.999999 + c1 * (u - y)).clamp(-clamp, clamp);
            s2 = (s2 * 0.999999 + c2 * s1 - g1 * s3).clamp(-clamp, clamp);
            s3 = (s3 * 0.999999 + c3 * s2).clamp(-clamp, clamp);
            s4 = (s4 * 0.999999 + c4 * s3 - g2 * s5).clamp(-clamp, clamp);
            s5 = (s5 * 0.999999 + c5 * s4).clamp(-clamp, clamp);
            s6 = (s6 * 0.999999 + c6 * s5 - g3 * s7).clamp(-clamp, clamp);
            s7 = (s7 * 0.999999 + c7 * s6).clamp(-clamp, clamp);

            let dither = ditherer.next_dither(dither_type);
            let sigma = u + a1 * s1 + a2 * s2 + a3 * s3 + a4 * s4 + a5 * s5 + a6 * s6 + a7 * s7 + dither;
            let is_pos = sigma >= 0.0;
            y = if is_pos { 1.0 } else { -1.0 };
            // Sony DSF Spec v1.01 (bits_per_sample = 1): LSB-first (bit 0 = earliest sample)
            byte_val |= (is_pos as u8) << bit;
        }
        ch_out[byte_idx] = byte_val;
    }

    st.s1 = s1; st.s2 = s2; st.s3 = s3; st.s4 = s4;
    st.s5 = s5; st.s6 = s6; st.s7 = s7; st.y = y;
}

impl DsdModulator {
    pub fn new(order: u8, phase_name: &str, dither: DitherType, gf: Option<FirFilter>) -> Self {
        let poly_phases = gf.as_ref().and_then(|f| f.poly_phases.clone().or_else(|| crate::fir::build_fir_phases(f)));
        Self {
            order: match order { 7 => 7, _ => 5 },
            dither_type: dither,
            dsd_ditherer: DsdDitherer::new(),
            gf,
            poly_phases,
            phase_name: phase_name.to_string(),
            dsd_rate: None,
            s1_l: 0.0, s2_l: 0.0, s3_l: 0.0, s4_l: 0.0, s5_l: 0.0, y_l: 0.0,
            s1_r: 0.0, s2_r: 0.0, s3_r: 0.0, s4_r: 0.0, s5_r: 0.0, y_r: 0.0,
            s6_l: 0.0, s7_l: 0.0,
            s6_r: 0.0, s7_r: 0.0,
        }
    }

    /// Explicitly set the target DSD sample rate so resonator notch coefficients match the true OSR
    /// even when the PCM input has already been pre-upsampled (e.g. 352.8 kHz / 384 kHz Stage 1).
    pub fn set_dsd_rate(&mut self, dsd_rate: u32) {
        self.dsd_rate = Some(dsd_rate);
    }

    pub fn reset_state(&mut self) {
        self.dsd_ditherer = DsdDitherer::new();
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
        if num_blocks == 0 {
            return;
        }
        match self.order {
            7 => self.run_blocks7(left, right, in_len, start_bit, num_blocks, inv_ratio, out),
            _ => self.run_blocks5(left, right, in_len, start_bit, num_blocks, inv_ratio, out),
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
        let osr = self.dsd_rate.map(|r| r as f64 / 44100.0).unwrap_or_else(|| 1.0 / inv_ratio);
        let p = get_ciff5_params_for_osr(osr);

        let ratio = 1.0 / inv_ratio;
        let is_exact_frac = (ratio - ratio.round()).abs() < 1e-9;
        let w_period = if is_exact_frac && ratio >= 1.0 && ratio <= 1024.0 { ratio.round() as usize } else { 0 };
        let w_tab_vec = if w_period > 0 { Some(get_hermite_tab(w_period)) } else { None };
        let w_tab = w_tab_vec.as_deref();

        let mut st_l = ChanState5 { s1: self.s1_l, s2: self.s2_l, s3: self.s3_l, s4: self.s4_l, s5: self.s5_l, y: self.y_l };
        let mut st_r = ChanState5 { s1: self.s1_r, s2: self.s2_r, s3: self.s3_r, s4: self.s4_r, s5: self.s5_r, y: self.y_r };

        let ch_bytes = num_blocks * DSD_BLOCK_SIZE;
        let mut left_out = vec![0u8; ch_bytes];
        let mut right_out = vec![0u8; ch_bytes];

        let dither_type = self.dither_type;
        let gf = self.gf.as_ref();
        let poly_phases = self.poly_phases.as_ref();
        let phase_name = self.phase_name.as_str();
        let dither_l = &mut self.dsd_ditherer.left;
        let dither_r = &mut self.dsd_ditherer.right;

        if num_blocks >= 4 {
            std::thread::scope(|s| {
                s.spawn(|| {
                    modulate_channel5(
                        left, in_len, start_bit, num_blocks, inv_ratio, p,
                        dither_type, dither_l, gf, poly_phases, phase_name,
                        w_period, w_tab, &mut st_l, &mut left_out,
                    );
                });
                modulate_channel5(
                    right, in_len, start_bit, num_blocks, inv_ratio, p,
                    dither_type, dither_r, gf, poly_phases, phase_name,
                    w_period, w_tab, &mut st_r, &mut right_out,
                );
            });
        } else {
            modulate_channel5(
                left, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_l, gf, poly_phases, phase_name,
                w_period, w_tab, &mut st_l, &mut left_out,
            );
            modulate_channel5(
                right, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_r, gf, poly_phases, phase_name,
                w_period, w_tab, &mut st_r, &mut right_out,
            );
        }

        for b in 0..num_blocks {
            let src_off = b * DSD_BLOCK_SIZE;
            let dst_off = b * 2 * DSD_BLOCK_SIZE;
            out[dst_off..dst_off + DSD_BLOCK_SIZE].copy_from_slice(&left_out[src_off..src_off + DSD_BLOCK_SIZE]);
            out[dst_off + DSD_BLOCK_SIZE..dst_off + 2 * DSD_BLOCK_SIZE].copy_from_slice(&right_out[src_off..src_off + DSD_BLOCK_SIZE]);
        }

        self.s1_l = st_l.s1; self.s2_l = st_l.s2; self.s3_l = st_l.s3; self.s4_l = st_l.s4; self.s5_l = st_l.s5; self.y_l = st_l.y;
        self.s1_r = st_r.s1; self.s2_r = st_r.s2; self.s3_r = st_r.s3; self.s4_r = st_r.s4; self.s5_r = st_r.s5; self.y_r = st_r.y;
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
        let osr = self.dsd_rate.map(|r| r as f64 / 44100.0).unwrap_or_else(|| 1.0 / inv_ratio);
        let p = get_ciff7_params_for_osr(osr);

        let ratio = 1.0 / inv_ratio;
        let is_exact_frac = (ratio - ratio.round()).abs() < 1e-9;
        let w_period = if is_exact_frac && ratio >= 1.0 && ratio <= 1024.0 { ratio.round() as usize } else { 0 };
        let w_tab_vec = if w_period > 0 { Some(get_hermite_tab(w_period)) } else { None };
        let w_tab = w_tab_vec.as_deref();

        let mut st_l = ChanState7 {
            s1: self.s1_l, s2: self.s2_l, s3: self.s3_l, s4: self.s4_l,
            s5: self.s5_l, s6: self.s6_l, s7: self.s7_l, y: self.y_l,
        };
        let mut st_r = ChanState7 {
            s1: self.s1_r, s2: self.s2_r, s3: self.s3_r, s4: self.s4_r,
            s5: self.s5_r, s6: self.s6_r, s7: self.s7_r, y: self.y_r,
        };

        let ch_bytes = num_blocks * DSD_BLOCK_SIZE;
        let mut left_out = vec![0u8; ch_bytes];
        let mut right_out = vec![0u8; ch_bytes];

        let dither_type = self.dither_type;
        let gf = self.gf.as_ref();
        let poly_phases = self.poly_phases.as_ref();
        let phase_name = self.phase_name.as_str();
        let dither_l = &mut self.dsd_ditherer.left;
        let dither_r = &mut self.dsd_ditherer.right;

        if num_blocks >= 4 {
            std::thread::scope(|s| {
                s.spawn(|| {
                    modulate_channel7(
                        left, in_len, start_bit, num_blocks, inv_ratio, p,
                        dither_type, dither_l, gf, poly_phases, phase_name,
                        w_period, w_tab, &mut st_l, &mut left_out,
                    );
                });
                modulate_channel7(
                    right, in_len, start_bit, num_blocks, inv_ratio, p,
                    dither_type, dither_r, gf, poly_phases, phase_name,
                    w_period, w_tab, &mut st_r, &mut right_out,
                );
            });
        } else {
            modulate_channel7(
                left, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_l, gf, poly_phases, phase_name,
                w_period, w_tab, &mut st_l, &mut left_out,
            );
            modulate_channel7(
                right, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_r, gf, poly_phases, phase_name,
                w_period, w_tab, &mut st_r, &mut right_out,
            );
        }

        for b in 0..num_blocks {
            let src_off = b * DSD_BLOCK_SIZE;
            let dst_off = b * 2 * DSD_BLOCK_SIZE;
            out[dst_off..dst_off + DSD_BLOCK_SIZE].copy_from_slice(&left_out[src_off..src_off + DSD_BLOCK_SIZE]);
            out[dst_off + DSD_BLOCK_SIZE..dst_off + 2 * DSD_BLOCK_SIZE].copy_from_slice(&right_out[src_off..src_off + DSD_BLOCK_SIZE]);
        }

        self.s1_l = st_l.s1; self.s2_l = st_l.s2; self.s3_l = st_l.s3; self.s4_l = st_l.s4;
        self.s5_l = st_l.s5; self.s6_l = st_l.s6; self.s7_l = st_l.s7; self.y_l = st_l.y;

        self.s1_r = st_r.s1; self.s2_r = st_r.s2; self.s3_r = st_r.s3; self.s4_r = st_r.s4;
        self.s5_r = st_r.s5; self.s6_r = st_r.s6; self.s7_r = st_r.s7; self.y_r = st_r.y;
    }
}
