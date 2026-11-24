use std::f64::consts::PI;
use serde::{Deserialize, Serialize};
use crate::fir::{fir_sample, interpolate_linear, interpolate_minimum, FirFilter, PolyphaseBank};
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

/// High-H_inf (H_inf = 1.85) CIFF-5 NTF coefficients dedicated to ASDM5EC M-algorithm Trellis modulator.
/// Greedy 1-bit quantizers diverge at this aggressive NTF, whereas a 16-step M=4 Trellis
/// keeps it unconditionally stable even at 0 dBFS while lowering 0-20 kHz noise by ~14 dB.
pub fn get_asdm5ec_params_for_osr(ratio: f64) -> Ciff5Coeffs {
    if ratio >= 384.0 {
        // dsd512
        Ciff5Coeffs {
            g1: 2.245248e-5,
            g2: 1.695662e-4,
            a: [1.415642, 1.406230, 1.248609, 1.021081, 0.975786],
        }
    } else if ratio >= 192.0 {
        // dsd256
        Ciff5Coeffs {
            g1: 8.980970e-5,
            g2: 6.782604e-4,
            a: [1.415672, 1.406184, 1.248261, 1.020642, 0.973850],
        }
    } else if ratio >= 96.0 {
        // dsd128
        Ciff5Coeffs {
            g1: 3.592356e-4,
            g2: 2.712973e-3,
            a: [1.415793, 1.406003, 1.246867, 1.018884, 0.966114],
        }
    } else {
        // dsd64
        Ciff5Coeffs {
            g1: 1.436891e-3,
            g2: 1.085079e-2,
            a: [1.416275, 1.405278, 1.241290, 1.011846, 0.935265],
        }
    }
}

/// High-H_inf (H_inf = 1.75) CIFF-7 NTF coefficients dedicated to ASDM7EC M-algorithm Trellis modulator.
/// Enables deep 7th-order noise shaping (~19 dB lower audio-band NTF floor than H_inf=1.50),
/// remaining rock-solid stable even at 0 dBFS full-scale input.
pub fn get_asdm7ec_params_for_osr(ratio: f64) -> Ciff7Coeffs {
    if ratio >= 384.0 {
        // dsd512
        Ciff7Coeffs {
            g1: 1.700599e-5,
            g2: 8.515906e-5,
            g3: 2.790184e-4,
            a: [1.683694, 1.933359, 2.131761, 1.733882, 1.538879, 0.925548, 0.595212],
        }
    } else if ratio >= 192.0 {
        // dsd256
        Ciff7Coeffs {
            g1: 6.802387e-5,
            g2: 3.406348e-4,
            g3: 1.116066e-3,
            a: [1.683755, 1.933225, 2.130843, 1.732316, 1.535252, 0.922894, 0.589038],
        }
    } else if ratio >= 96.0 {
        // dsd128
        Ciff7Coeffs {
            g1: 2.720941e-4,
            g2: 1.362516e-3,
            g3: 4.464138e-3,
            a: [1.684000, 1.932691, 2.127167, 1.726050, 1.520760, 0.912299, 0.564496],
        }
    } else {
        // dsd64
        Ciff7Coeffs {
            g1: 1.088354e-3,
            g2: 5.449693e-3,
            g3: 1.785456e-2,
            a: [1.684980, 1.930559, 2.112441, 1.700949, 1.463033, 0.870259, 0.468778],
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

pub const EC_BEAM_WIDTH: usize = 4;
pub const EC_TRACEBACK: u32 = 32;

#[derive(Clone, Copy)]
struct TrellisPath5 {
    s1: f64, s2: f64, s3: f64, s4: f64, s5: f64, y: f64,
    cost: f64,
    bits: u64,
}

impl TrellisPath5 {
    fn init_beam() -> [Self; EC_BEAM_WIDTH] {
        let dummy = Self {
            s1: 0.0, s2: 0.0, s3: 0.0, s4: 0.0, s5: 0.0, y: 0.0,
            cost: 1e30,
            bits: 0xAAAA_AAAA_AAAA_AAAA,
        };
        let mut beam = [dummy; EC_BEAM_WIDTH];
        beam[0].cost = 0.0;
        beam
    }
}

#[derive(Clone, Copy)]
struct TrellisPath7 {
    s1: f64, s2: f64, s3: f64, s4: f64, s5: f64, s6: f64, s7: f64, y: f64,
    cost: f64,
    bits: u64,
}

impl TrellisPath7 {
    fn init_beam() -> [Self; EC_BEAM_WIDTH] {
        let dummy = Self {
            s1: 0.0, s2: 0.0, s3: 0.0, s4: 0.0, s5: 0.0, s6: 0.0, s7: 0.0, y: 0.0,
            cost: 1e30,
            bits: 0xAAAA_AAAA_AAAA_AAAA,
        };
        let mut beam = [dummy; EC_BEAM_WIDTH];
        beam[0].cost = 0.0;
        beam
    }
}

pub struct DsdModulator {
    pub order: u8, // 5, 7, 50 (ASDM5EC), 70 (ASDM7EC)
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

    // Stateful M-algorithm Trellis beams for ASDM5EC / ASDM7EC (seamless across blocks)
    beam5_l: [TrellisPath5; EC_BEAM_WIDTH],
    beam5_r: [TrellisPath5; EC_BEAM_WIDTH],
    beam7_l: [TrellisPath7; EC_BEAM_WIDTH],
    beam7_r: [TrellisPath7; EC_BEAM_WIDTH],
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
fn load_hermite_forward_diffs(pcm: &[f64], in_len: usize, cur_idx: usize, inv_w: f64, scale: f64) -> (f64, f64, f64, f64) {
    let p1 = if cur_idx < in_len { pcm[cur_idx] } else { 0.0 };
    let p2 = if cur_idx + 1 < in_len { pcm[cur_idx + 1] } else { 0.0 };
    let p0 = if cur_idx > 0 && cur_idx - 1 < in_len { pcm[cur_idx - 1] } else { p1 };
    let p3 = if cur_idx + 2 < in_len { pcm[cur_idx + 2] } else { p2 };
    let m1 = 0.5 * (p2 - p0);
    let m2 = 0.5 * (p3 - p1);

    // Cubic Hermite polynomial H(t) = A*t^3 + B*t^2 + C*t + D, pre-scaled by `scale`
    let cap_a = 2.0 * p1 - 2.0 * p2 + m1 + m2;
    let cap_b = -3.0 * p1 + 3.0 * p2 - 2.0 * m1 - m2;
    let cap_c = m1;
    let cap_d = p1;

    let h = inv_w;
    let h2 = h * h;
    let h3 = h2 * h;
    let a = scale * cap_a * h3;
    let b = scale * cap_b * h2;
    let c = scale * cap_c * h;
    let d = scale * cap_d;

    // Forward finite differences: 3 additions per DSD sample, mathematically exact to f64 precision
    let r0 = d;
    let r1 = a + b + c;
    let r2 = 6.0 * a + 2.0 * b;
    let r3 = 6.0 * a;
    (r0, r1, r2, r3)
}

const DSD_GAUSS_CLT_SCALE: f64 = (0.0125 * 0.707) / 37837.22;

#[inline(always)]
fn next_reg_gauss(prng: &mut u64) -> f64 {
    let mut x = *prng;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *prng = x;
    let sum = (x as i16 as i32)
        + ((x >> 16) as i16 as i32)
        + ((x >> 32) as i16 as i32)
        + ((x >> 48) as i16 as i32);
    (sum as f64) * DSD_GAUSS_CLT_SCALE
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

    if gf.is_none() && w_period > 0 {
        // Ultra-fast Forward-Difference Hermite + Register-local PRNG + Per-byte stability guard
        let inv_w = 1.0 / w_period as f64;
        let mut cur_idx = start_bit / w_period;
        let mut cur_phi = start_bit - cur_idx * w_period;
        let (mut r0, mut r1, mut r2, mut r3) = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
        for _ in 0..cur_phi {
            r0 += r1;
            r1 += r2;
            r2 += r3;
        }
        let mut prng = if ditherer.prng1 == 0 { 0x9e37_79b9_7f4a_7c15 } else { ditherer.prng1 };
        let use_fast_gauss = matches!(dither_type, DitherType::Gauss);
        let use_no_dither = matches!(dither_type, DitherType::None);

        for byte_idx in 0..total_bytes {
            let mut byte_val = 0u8;
            for bit in 0..8 {
                let u = r0;
                r0 += r1;
                r1 += r2;
                r2 += r3;

                cur_phi += 1;
                if cur_phi == w_period {
                    cur_phi = 0;
                    cur_idx += 1;
                    let d = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
                    r0 = d.0; r1 = d.1; r2 = d.2; r3 = d.3;
                }

                s1 = s1 * 0.999999 + c1 * (u - y);
                s2 = s2 * 0.999999 + c2 * s1 - g1 * s3;
                s3 = s3 * 0.999999 + c3 * s2;
                s4 = s4 * 0.999999 + c4 * s3 - g2 * s5;
                s5 = s5 * 0.999999 + c5 * s4;

                let dither = if use_fast_gauss {
                    next_reg_gauss(&mut prng)
                } else if use_no_dither {
                    0.0
                } else {
                    ditherer.next_dither(dither_type)
                };
                let sigma = u + a1 * s1 + a2 * s2 + a3 * s3 + a4 * s4 + a5 * s5 + dither;
                let is_pos = sigma >= 0.0;
                y = if is_pos { 1.0 } else { -1.0 };
                // Sony DSF Spec v1.01 (bits_per_sample = 1): LSB-first (bit 0 = earliest sample)
                byte_val |= (is_pos as u8) << bit;
            }
            if s1.abs() > clamp || s5.abs() > clamp || !s5.is_finite() {
                s1 = s1.clamp(-clamp, clamp);
                s2 = s2.clamp(-clamp, clamp);
                s3 = s3.clamp(-clamp, clamp);
                s4 = s4.clamp(-clamp, clamp);
                s5 = s5.clamp(-clamp, clamp);
            }
            ch_out[byte_idx] = byte_val;
        }

        if use_fast_gauss {
            ditherer.prng1 = prng;
        }
        st.s1 = s1; st.s2 = s2; st.s3 = s3; st.s4 = s4; st.s5 = s5; st.y = y;
        return;
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

    if gf.is_none() && w_period > 0 {
        // Ultra-fast Forward-Difference Hermite + Register-local PRNG + Per-byte stability guard
        let inv_w = 1.0 / w_period as f64;
        let mut cur_idx = start_bit / w_period;
        let mut cur_phi = start_bit - cur_idx * w_period;
        let (mut r0, mut r1, mut r2, mut r3) = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
        for _ in 0..cur_phi {
            r0 += r1;
            r1 += r2;
            r2 += r3;
        }
        let mut prng = if ditherer.prng1 == 0 { 0x9e37_79b9_7f4a_7c15 } else { ditherer.prng1 };
        let use_fast_gauss = matches!(dither_type, DitherType::Gauss);
        let use_no_dither = matches!(dither_type, DitherType::None);

        for byte_idx in 0..total_bytes {
            let mut byte_val = 0u8;
            for bit in 0..8 {
                let u = r0;
                r0 += r1;
                r1 += r2;
                r2 += r3;

                cur_phi += 1;
                if cur_phi == w_period {
                    cur_phi = 0;
                    cur_idx += 1;
                    let d = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
                    r0 = d.0; r1 = d.1; r2 = d.2; r3 = d.3;
                }

                s1 = s1 * 0.999999 + c1 * (u - y);
                s2 = s2 * 0.999999 + c2 * s1 - g1 * s3;
                s3 = s3 * 0.999999 + c3 * s2;
                s4 = s4 * 0.999999 + c4 * s3 - g2 * s5;
                s5 = s5 * 0.999999 + c5 * s4;
                s6 = s6 * 0.999999 + c6 * s5 - g3 * s7;
                s7 = s7 * 0.999999 + c7 * s6;

                let dither = if use_fast_gauss {
                    next_reg_gauss(&mut prng)
                } else if use_no_dither {
                    0.0
                } else {
                    ditherer.next_dither(dither_type)
                };
                let sigma = u + a1 * s1 + a2 * s2 + a3 * s3 + a4 * s4 + a5 * s5 + a6 * s6 + a7 * s7 + dither;
                let is_pos = sigma >= 0.0;
                y = if is_pos { 1.0 } else { -1.0 };
                // Sony DSF Spec v1.01 (bits_per_sample = 1): LSB-first (bit 0 = earliest sample)
                byte_val |= (is_pos as u8) << bit;
            }
            if s1.abs() > clamp || s7.abs() > clamp || !s7.is_finite() {
                s1 = s1.clamp(-clamp, clamp);
                s2 = s2.clamp(-clamp, clamp);
                s3 = s3.clamp(-clamp, clamp);
                s4 = s4.clamp(-clamp, clamp);
                s5 = s5.clamp(-clamp, clamp);
                s6 = s6.clamp(-clamp, clamp);
                s7 = s7.clamp(-clamp, clamp);
            }
            ch_out[byte_idx] = byte_val;
        }

        if use_fast_gauss {
            ditherer.prng1 = prng;
        }
        st.s1 = s1; st.s2 = s2; st.s3 = s3; st.s4 = s4;
        st.s5 = s5; st.s6 = s6; st.s7 = s7; st.y = y;
        return;
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
            order: match order {
                7 => 7,
                50 => 50, // ASDM5EC (5th-order 2-step Lookahead Trellis Enhanced Correction)
                70 => 70, // ASDM7EC (7th-order 2-step Lookahead Trellis Enhanced Correction)
                _ => 5,
            },
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
            beam5_l: TrellisPath5::init_beam(),
            beam5_r: TrellisPath5::init_beam(),
            beam7_l: TrellisPath7::init_beam(),
            beam7_r: TrellisPath7::init_beam(),
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
        self.beam5_l = TrellisPath5::init_beam();
        self.beam5_r = TrellisPath5::init_beam();
        self.beam7_l = TrellisPath7::init_beam();
        self.beam7_r = TrellisPath7::init_beam();
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
            50 => self.run_blocks5_ec(left, right, in_len, start_bit, num_blocks, inv_ratio, out),
            70 => self.run_blocks7_ec(left, right, in_len, start_bit, num_blocks, inv_ratio, out),
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
                        w_period, &mut st_l, &mut left_out,
                    );
                });
                modulate_channel5(
                    right, in_len, start_bit, num_blocks, inv_ratio, p,
                    dither_type, dither_r, gf, poly_phases, phase_name,
                    w_period, &mut st_r, &mut right_out,
                );
            });
        } else {
            modulate_channel5(
                left, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_l, gf, poly_phases, phase_name,
                w_period, &mut st_l, &mut left_out,
            );
            modulate_channel5(
                right, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_r, gf, poly_phases, phase_name,
                w_period, &mut st_r, &mut right_out,
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
                        w_period, &mut st_l, &mut left_out,
                    );
                });
                modulate_channel7(
                    right, in_len, start_bit, num_blocks, inv_ratio, p,
                    dither_type, dither_r, gf, poly_phases, phase_name,
                    w_period, &mut st_r, &mut right_out,
                );
            });
        } else {
            modulate_channel7(
                left, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_l, gf, poly_phases, phase_name,
                w_period, &mut st_l, &mut left_out,
            );
            modulate_channel7(
                right, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_r, gf, poly_phases, phase_name,
                w_period, &mut st_r, &mut right_out,
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

    fn run_blocks5_ec(
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
        let p = get_asdm5ec_params_for_osr(osr);

        let ratio = 1.0 / inv_ratio;
        let is_exact_frac = (ratio - ratio.round()).abs() < 1e-9;
        let w_period = if is_exact_frac && ratio >= 1.0 && ratio <= 1024.0 { ratio.round() as usize } else { 0 };

        let mut beam_l = self.beam5_l;
        let mut beam_r = self.beam5_r;

        let ch_bytes = num_blocks * DSD_BLOCK_SIZE;
        let mut left_out = vec![0u8; ch_bytes];
        let mut right_out = vec![0u8; ch_bytes];

        let dither_type = self.dither_type;
        let gf = self.gf.as_ref();
        let poly_phases = self.poly_phases.as_ref();
        let phase_name = self.phase_name.as_str();
        let dither_l = &mut self.dsd_ditherer.left;
        let dither_r = &mut self.dsd_ditherer.right;

        if num_blocks >= 2 {
            std::thread::scope(|s| {
                s.spawn(|| {
                    modulate_channel5_ec(
                        left, in_len, start_bit, num_blocks, inv_ratio, p,
                        dither_type, dither_l, gf, poly_phases, phase_name,
                        w_period, &mut beam_l, &mut left_out,
                    );
                });
                modulate_channel5_ec(
                    right, in_len, start_bit, num_blocks, inv_ratio, p,
                    dither_type, dither_r, gf, poly_phases, phase_name,
                    w_period, &mut beam_r, &mut right_out,
                );
            });
        } else {
            modulate_channel5_ec(
                left, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_l, gf, poly_phases, phase_name,
                w_period, &mut beam_l, &mut left_out,
            );
            modulate_channel5_ec(
                right, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_r, gf, poly_phases, phase_name,
                w_period, &mut beam_r, &mut right_out,
            );
        }

        for b in 0..num_blocks {
            let src_off = b * DSD_BLOCK_SIZE;
            let dst_off = b * 2 * DSD_BLOCK_SIZE;
            out[dst_off..dst_off + DSD_BLOCK_SIZE].copy_from_slice(&left_out[src_off..src_off + DSD_BLOCK_SIZE]);
            out[dst_off + DSD_BLOCK_SIZE..dst_off + 2 * DSD_BLOCK_SIZE].copy_from_slice(&right_out[src_off..src_off + DSD_BLOCK_SIZE]);
        }

        self.beam5_l = beam_l;
        self.beam5_r = beam_r;
        self.s1_l = beam_l[0].s1; self.s2_l = beam_l[0].s2; self.s3_l = beam_l[0].s3;
        self.s4_l = beam_l[0].s4; self.s5_l = beam_l[0].s5; self.y_l = beam_l[0].y;
        self.s1_r = beam_r[0].s1; self.s2_r = beam_r[0].s2; self.s3_r = beam_r[0].s3;
        self.s4_r = beam_r[0].s4; self.s5_r = beam_r[0].s5; self.y_r = beam_r[0].y;
    }

    fn run_blocks7_ec(
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
        let p = get_asdm7ec_params_for_osr(osr);

        let ratio = 1.0 / inv_ratio;
        let is_exact_frac = (ratio - ratio.round()).abs() < 1e-9;
        let w_period = if is_exact_frac && ratio >= 1.0 && ratio <= 1024.0 { ratio.round() as usize } else { 0 };

        let mut beam_l = self.beam7_l;
        let mut beam_r = self.beam7_r;

        let ch_bytes = num_blocks * DSD_BLOCK_SIZE;
        let mut left_out = vec![0u8; ch_bytes];
        let mut right_out = vec![0u8; ch_bytes];

        let dither_type = self.dither_type;
        let gf = self.gf.as_ref();
        let poly_phases = self.poly_phases.as_ref();
        let phase_name = self.phase_name.as_str();
        let dither_l = &mut self.dsd_ditherer.left;
        let dither_r = &mut self.dsd_ditherer.right;

        if num_blocks >= 2 {
            std::thread::scope(|s| {
                s.spawn(|| {
                    modulate_channel7_ec(
                        left, in_len, start_bit, num_blocks, inv_ratio, p,
                        dither_type, dither_l, gf, poly_phases, phase_name,
                        w_period, &mut beam_l, &mut left_out,
                    );
                });
                modulate_channel7_ec(
                    right, in_len, start_bit, num_blocks, inv_ratio, p,
                    dither_type, dither_r, gf, poly_phases, phase_name,
                    w_period, &mut beam_r, &mut right_out,
                );
            });
        } else {
            modulate_channel7_ec(
                left, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_l, gf, poly_phases, phase_name,
                w_period, &mut beam_l, &mut left_out,
            );
            modulate_channel7_ec(
                right, in_len, start_bit, num_blocks, inv_ratio, p,
                dither_type, dither_r, gf, poly_phases, phase_name,
                w_period, &mut beam_r, &mut right_out,
            );
        }

        for b in 0..num_blocks {
            let src_off = b * DSD_BLOCK_SIZE;
            let dst_off = b * 2 * DSD_BLOCK_SIZE;
            out[dst_off..dst_off + DSD_BLOCK_SIZE].copy_from_slice(&left_out[src_off..src_off + DSD_BLOCK_SIZE]);
            out[dst_off + DSD_BLOCK_SIZE..dst_off + 2 * DSD_BLOCK_SIZE].copy_from_slice(&right_out[src_off..src_off + DSD_BLOCK_SIZE]);
        }

        self.beam7_l = beam_l;
        self.beam7_r = beam_r;
        self.s1_l = beam_l[0].s1; self.s2_l = beam_l[0].s2; self.s3_l = beam_l[0].s3; self.s4_l = beam_l[0].s4;
        self.s5_l = beam_l[0].s5; self.s6_l = beam_l[0].s6; self.s7_l = beam_l[0].s7; self.y_l = beam_l[0].y;
        self.s1_r = beam_r[0].s1; self.s2_r = beam_r[0].s2; self.s3_r = beam_r[0].s3; self.s4_r = beam_r[0].s4;
        self.s5_r = beam_r[0].s5; self.s6_r = beam_r[0].s6; self.s7_r = beam_r[0].s7; self.y_r = beam_r[0].y;
    }
}

/// Stateful M-Algorithm (Beam-Search) Trellis step for ASDM5EC (M = 4 paths, L = 32 lookahead depth).
/// Minimizes accumulated (1/NTF)*(u - y) energy across the Trellis horizon, stabilizing H_inf = 1.85 NTF.
#[inline(always)]
fn trellis_step5(beam: &mut [TrellisPath5; EC_BEAM_WIDTH], u: f64, dither: f64, p: &Ciff5Coeffs) -> u8 {
    let c1 = CIFFR_5_C[0]; let c2 = CIFFR_5_C[1]; let c3 = CIFFR_5_C[2]; let c4 = CIFFR_5_C[3]; let c5 = CIFFR_5_C[4];
    let a1 = p.a[0]; let a2 = p.a[1]; let a3 = p.a[2]; let a4 = p.a[3]; let a5 = p.a[4];
    let g1 = p.g1; let g2 = p.g2;

    let mut ns1 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns2 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns3 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns4 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns5 = [0.0f64; EC_BEAM_WIDTH];
    let mut nbits = [0u64; EC_BEAM_WIDTH];
    let mut costs = [1e30f64; EC_BEAM_WIDTH * 2];

    for k in 0..EC_BEAM_WIDTH {
        let path = &beam[k];
        if path.cost >= 1e29 {
            continue;
        }
        let s1 = path.s1 * 0.999999 + c1 * (u - path.y);
        let s2 = path.s2 * 0.999999 + c2 * s1 - g1 * path.s3;
        let s3 = path.s3 * 0.999999 + c3 * s2;
        let s4 = path.s4 * 0.999999 + c4 * s3 - g2 * path.s5;
        let s5 = path.s5 * 0.999999 + c5 * s4;
        let sigma = u + a1 * s1 + a2 * s2 + a3 * s3 + a4 * s4 + a5 * s5 + dither;

        let pen = if s5.abs() > 3.5 {
            let ex = s5.abs() - 3.5;
            0.5 * ex * ex
        } else {
            0.0
        };

        let e_pos = sigma - 1.0;
        let e_neg = sigma + 1.0;
        let base = path.cost + pen;
        ns1[k] = s1; ns2[k] = s2; ns3[k] = s3; ns4[k] = s4; ns5[k] = s5;
        nbits[k] = path.bits << 1;
        costs[2 * k] = base + e_pos * e_pos;
        costs[2 * k + 1] = base + e_neg * e_neg;
    }

    for out_i in 0..EC_BEAM_WIDTH {
        let mut best_j = 0;
        let mut best_c = costs[0];
        for j in 1..(EC_BEAM_WIDTH * 2) {
            if costs[j] < best_c {
                best_c = costs[j];
                best_j = j;
            }
        }
        costs[best_j] = f64::INFINITY;
        let pk = best_j >> 1;
        let is_pos = (best_j & 1) == 0;
        beam[out_i] = TrellisPath5 {
            s1: ns1[pk], s2: ns2[pk], s3: ns3[pk], s4: ns4[pk], s5: ns5[pk],
            y: if is_pos { 1.0 } else { -1.0 },
            cost: best_c,
            bits: nbits[pk] | (is_pos as u64),
        };
    }

    let out_bit = ((beam[0].bits >> (EC_TRACEBACK - 1)) & 1) as u8;
    for path in beam.iter_mut().skip(1) {
        if (((path.bits >> (EC_TRACEBACK - 1)) & 1) as u8) != out_bit {
            path.cost = 1e30;
        }
    }
    out_bit
}

/// Stateful M-Algorithm (Beam-Search) Trellis step for ASDM7EC (M = 4 paths, L = 32 lookahead depth).
/// Minimizes accumulated (1/NTF)*(u - y) energy across the Trellis horizon, stabilizing H_inf = 1.75 NTF.
#[inline(always)]
fn trellis_step7(beam: &mut [TrellisPath7; EC_BEAM_WIDTH], u: f64, dither: f64, p: &Ciff7Coeffs) -> u8 {
    let c1 = CIFFR_7_C[0]; let c2 = CIFFR_7_C[1]; let c3 = CIFFR_7_C[2]; let c4 = CIFFR_7_C[3];
    let c5 = CIFFR_7_C[4]; let c6 = CIFFR_7_C[5]; let c7 = CIFFR_7_C[6];
    let a1 = p.a[0]; let a2 = p.a[1]; let a3 = p.a[2]; let a4 = p.a[3];
    let a5 = p.a[4]; let a6 = p.a[5]; let a7 = p.a[6];
    let g1 = p.g1; let g2 = p.g2; let g3 = p.g3;

    let mut ns1 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns2 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns3 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns4 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns5 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns6 = [0.0f64; EC_BEAM_WIDTH];
    let mut ns7 = [0.0f64; EC_BEAM_WIDTH];
    let mut nbits = [0u64; EC_BEAM_WIDTH];
    let mut costs = [1e30f64; EC_BEAM_WIDTH * 2];

    for k in 0..EC_BEAM_WIDTH {
        let path = &beam[k];
        if path.cost >= 1e29 {
            continue;
        }
        let s1 = path.s1 * 0.999999 + c1 * (u - path.y);
        let s2 = path.s2 * 0.999999 + c2 * s1 - g1 * path.s3;
        let s3 = path.s3 * 0.999999 + c3 * s2;
        let s4 = path.s4 * 0.999999 + c4 * s3 - g2 * path.s5;
        let s5 = path.s5 * 0.999999 + c5 * s4;
        let s6 = path.s6 * 0.999999 + c6 * s5 - g3 * path.s7;
        let s7 = path.s7 * 0.999999 + c7 * s6;
        let sigma = u + a1 * s1 + a2 * s2 + a3 * s3 + a4 * s4 + a5 * s5 + a6 * s6 + a7 * s7 + dither;

        let pen = if s7.abs() > 3.5 {
            let ex = s7.abs() - 3.5;
            0.5 * ex * ex
        } else {
            0.0
        };

        let e_pos = sigma - 1.0;
        let e_neg = sigma + 1.0;
        let base = path.cost + pen;
        ns1[k] = s1; ns2[k] = s2; ns3[k] = s3; ns4[k] = s4;
        ns5[k] = s5; ns6[k] = s6; ns7[k] = s7;
        nbits[k] = path.bits << 1;
        costs[2 * k] = base + e_pos * e_pos;
        costs[2 * k + 1] = base + e_neg * e_neg;
    }

    for out_i in 0..EC_BEAM_WIDTH {
        let mut best_j = 0;
        let mut best_c = costs[0];
        for j in 1..(EC_BEAM_WIDTH * 2) {
            if costs[j] < best_c {
                best_c = costs[j];
                best_j = j;
            }
        }
        costs[best_j] = f64::INFINITY;
        let pk = best_j >> 1;
        let is_pos = (best_j & 1) == 0;
        beam[out_i] = TrellisPath7 {
            s1: ns1[pk], s2: ns2[pk], s3: ns3[pk], s4: ns4[pk],
            s5: ns5[pk], s6: ns6[pk], s7: ns7[pk],
            y: if is_pos { 1.0 } else { -1.0 },
            cost: best_c,
            bits: nbits[pk] | (is_pos as u64),
        };
    }

    let out_bit = ((beam[0].bits >> (EC_TRACEBACK - 1)) & 1) as u8;
    for path in beam.iter_mut().skip(1) {
        if (((path.bits >> (EC_TRACEBACK - 1)) & 1) as u8) != out_bit {
            path.cost = 1e30;
        }
    }
    out_bit
}

#[allow(clippy::too_many_arguments)]
fn modulate_channel5_ec(
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
    beam: &mut [TrellisPath5; EC_BEAM_WIDTH],
    ch_out: &mut [u8],
) {
    let total_bytes = num_blocks * DSD_BLOCK_SIZE;
    let scale = CIFFR_5_SCALE;
    let clamp = CIFFR_5_CLAMP;
    let ratio = 1.0 / inv_ratio;
    let is_min_phase = phase_name.contains("min");

    let mut prng = if ditherer.prng1 == 0 { 0x9e37_79b9_7f4a_7c15 } else { ditherer.prng1 };
    let use_fast_gauss = matches!(dither_type, DitherType::Gauss);
    let use_no_dither = matches!(dither_type, DitherType::None);

    if gf.is_none() && w_period > 0 {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma") {
            // Safety: runtime-checked AVX2 + FMA support
            unsafe {
                modulate_channel5_ec_avx2(
                    pcm, in_len, start_bit, total_bytes, p,
                    dither_type, ditherer, w_period, beam, ch_out,
                );
            }
            return;
        }

        let inv_w = 1.0 / w_period as f64;
        let mut cur_idx = start_bit / w_period;
        let mut cur_phi = start_bit - cur_idx * w_period;
        let (mut r0, mut r1, mut r2, mut r3) = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
        for _ in 0..cur_phi {
            r0 += r1; r1 += r2; r2 += r3;
        }

        for byte_idx in 0..total_bytes {
            let mut byte_val = 0u8;
            for bit in 0..8 {
                let u = r0;
                r0 += r1; r1 += r2; r2 += r3;
                cur_phi += 1;
                if cur_phi == w_period {
                    cur_phi = 0;
                    cur_idx += 1;
                    let d = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
                    r0 = d.0; r1 = d.1; r2 = d.2; r3 = d.3;
                }

                let dither = if use_fast_gauss {
                    next_reg_gauss(&mut prng)
                } else if use_no_dither {
                    0.0
                } else {
                    ditherer.next_dither(dither_type)
                };
                let b = trellis_step5(beam, u, dither, &p);
                byte_val |= b << bit;
            }
            let base_cost = beam[0].cost;
            for path in beam.iter_mut() {
                if path.cost < 1e29 {
                    path.cost -= base_cost;
                }
                if path.s1.abs() > clamp || path.s5.abs() > clamp || !path.s5.is_finite() {
                    path.s1 = path.s1.clamp(-clamp, clamp);
                    path.s2 = path.s2.clamp(-clamp, clamp);
                    path.s3 = path.s3.clamp(-clamp, clamp);
                    path.s4 = path.s4.clamp(-clamp, clamp);
                    path.s5 = path.s5.clamp(-clamp, clamp);
                }
            }
            ch_out[byte_idx] = byte_val;
        }
    } else {
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
                    let k = t_src.floor() as usize;
                    let frac = t_src - k as f64;
                    let p1 = if k < in_len { pcm[k] } else { 0.0 };
                    let p2 = if k + 1 < in_len { pcm[k + 1] } else { 0.0 };
                    let p0 = if k > 0 && k - 1 < in_len { pcm[k - 1] } else { p1 };
                    let p3 = if k + 2 < in_len { pcm[k + 2] } else { p2 };
                    if is_min_phase {
                        interpolate_minimum(p0, p1, p2, p3, frac)
                    } else {
                        interpolate_linear(p0, p1, p2, p3, frac)
                    }
                };
                let u = raw * scale;
                let dither = if use_fast_gauss {
                    next_reg_gauss(&mut prng)
                } else if use_no_dither {
                    0.0
                } else {
                    ditherer.next_dither(dither_type)
                };
                let b = trellis_step5(beam, u, dither, &p);
                byte_val |= b << bit;
            }
            let base_cost = beam[0].cost;
            for path in beam.iter_mut() {
                if path.cost < 1e29 {
                    path.cost -= base_cost;
                }
                if path.s1.abs() > clamp || path.s5.abs() > clamp || !path.s5.is_finite() {
                    path.s1 = path.s1.clamp(-clamp, clamp);
                    path.s2 = path.s2.clamp(-clamp, clamp);
                    path.s3 = path.s3.clamp(-clamp, clamp);
                    path.s4 = path.s4.clamp(-clamp, clamp);
                    path.s5 = path.s5.clamp(-clamp, clamp);
                }
            }
            ch_out[byte_idx] = byte_val;
        }
    }

    if use_fast_gauss {
        ditherer.prng1 = prng;
    }
}

#[allow(clippy::too_many_arguments)]
fn modulate_channel7_ec(
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
    beam: &mut [TrellisPath7; EC_BEAM_WIDTH],
    ch_out: &mut [u8],
) {
    let total_bytes = num_blocks * DSD_BLOCK_SIZE;
    let scale = CIFFR_7_SCALE;
    let clamp = CIFFR_7_CLAMP;
    let ratio = 1.0 / inv_ratio;
    let is_min_phase = phase_name.contains("min");

    let mut prng = if ditherer.prng1 == 0 { 0x9e37_79b9_7f4a_7c15 } else { ditherer.prng1 };
    let use_fast_gauss = matches!(dither_type, DitherType::Gauss);
    let use_no_dither = matches!(dither_type, DitherType::None);

    if gf.is_none() && w_period > 0 {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma") {
            // Safety: runtime-checked AVX2 + FMA support
            unsafe {
                modulate_channel7_ec_avx2(
                    pcm, in_len, start_bit, total_bytes, p,
                    dither_type, ditherer, w_period, beam, ch_out,
                );
            }
            return;
        }

        let inv_w = 1.0 / w_period as f64;
        let mut cur_idx = start_bit / w_period;
        let mut cur_phi = start_bit - cur_idx * w_period;
        let (mut r0, mut r1, mut r2, mut r3) = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
        for _ in 0..cur_phi {
            r0 += r1; r1 += r2; r2 += r3;
        }

        for byte_idx in 0..total_bytes {
            let mut byte_val = 0u8;
            for bit in 0..8 {
                let u = r0;
                r0 += r1; r1 += r2; r2 += r3;
                cur_phi += 1;
                if cur_phi == w_period {
                    cur_phi = 0;
                    cur_idx += 1;
                    let d = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
                    r0 = d.0; r1 = d.1; r2 = d.2; r3 = d.3;
                }

                let dither = if use_fast_gauss {
                    next_reg_gauss(&mut prng)
                } else if use_no_dither {
                    0.0
                } else {
                    ditherer.next_dither(dither_type)
                };
                let b = trellis_step7(beam, u, dither, &p);
                byte_val |= b << bit;
            }
            let base_cost = beam[0].cost;
            for path in beam.iter_mut() {
                if path.cost < 1e29 {
                    path.cost -= base_cost;
                }
                if path.s1.abs() > clamp || path.s7.abs() > clamp || !path.s7.is_finite() {
                    path.s1 = path.s1.clamp(-clamp, clamp);
                    path.s2 = path.s2.clamp(-clamp, clamp);
                    path.s3 = path.s3.clamp(-clamp, clamp);
                    path.s4 = path.s4.clamp(-clamp, clamp);
                    path.s5 = path.s5.clamp(-clamp, clamp);
                    path.s6 = path.s6.clamp(-clamp, clamp);
                    path.s7 = path.s7.clamp(-clamp, clamp);
                }
            }
            ch_out[byte_idx] = byte_val;
        }
    } else {
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
                    let k = t_src.floor() as usize;
                    let frac = t_src - k as f64;
                    let p1 = if k < in_len { pcm[k] } else { 0.0 };
                    let p2 = if k + 1 < in_len { pcm[k + 1] } else { 0.0 };
                    let p0 = if k > 0 && k - 1 < in_len { pcm[k - 1] } else { p1 };
                    let p3 = if k + 2 < in_len { pcm[k + 2] } else { p2 };
                    if is_min_phase {
                        interpolate_minimum(p0, p1, p2, p3, frac)
                    } else {
                        interpolate_linear(p0, p1, p2, p3, frac)
                    }
                };
                let u = raw * scale;
                let dither = if use_fast_gauss {
                    next_reg_gauss(&mut prng)
                } else if use_no_dither {
                    0.0
                } else {
                    ditherer.next_dither(dither_type)
                };
                let b = trellis_step7(beam, u, dither, &p);
                byte_val |= b << bit;
            }
            let base_cost = beam[0].cost;
            for path in beam.iter_mut() {
                if path.cost < 1e29 {
                    path.cost -= base_cost;
                }
                if path.s1.abs() > clamp || path.s7.abs() > clamp || !path.s7.is_finite() {
                    path.s1 = path.s1.clamp(-clamp, clamp);
                    path.s2 = path.s2.clamp(-clamp, clamp);
                    path.s3 = path.s3.clamp(-clamp, clamp);
                    path.s4 = path.s4.clamp(-clamp, clamp);
                    path.s5 = path.s5.clamp(-clamp, clamp);
                    path.s6 = path.s6.clamp(-clamp, clamp);
                    path.s7 = path.s7.clamp(-clamp, clamp);
                }
            }
            ch_out[byte_idx] = byte_val;
        }
    }

    if use_fast_gauss {
        ditherer.prng1 = prng;
    }
}

#[cfg(target_arch = "x86_64")]
const LANE_PERM_64: [i64; 4] = [
    0x0000_0001_0000_0000u64 as i64,
    0x0000_0003_0000_0002u64 as i64,
    0x0000_0005_0000_0004u64 as i64,
    0x0000_0007_0000_0006u64 as i64,
];

#[cfg(target_arch = "x86_64")]
const Y_MASK_LUT: [[f64; 4]; 16] = [
    [-1.0, -1.0, -1.0, -1.0], [ 1.0, -1.0, -1.0, -1.0], [-1.0,  1.0, -1.0, -1.0], [ 1.0,  1.0, -1.0, -1.0],
    [-1.0, -1.0,  1.0, -1.0], [ 1.0, -1.0,  1.0, -1.0], [-1.0,  1.0,  1.0, -1.0], [ 1.0,  1.0,  1.0, -1.0],
    [-1.0, -1.0, -1.0,  1.0], [ 1.0, -1.0, -1.0,  1.0], [-1.0,  1.0, -1.0,  1.0], [ 1.0,  1.0, -1.0,  1.0],
    [-1.0, -1.0,  1.0,  1.0], [ 1.0, -1.0,  1.0,  1.0], [-1.0,  1.0,  1.0,  1.0], [ 1.0,  1.0,  1.0,  1.0],
];

#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn cmp_swap_u64(a: &mut u64, b: &mut u64) {
    let x = *a;
    let y = *b;
    *a = x.min(y);
    *b = x.max(y);
}

/// Ultra-fast branchless Batcher bitonic half-cleaner on u64-packed (f32_bits << 3 | idx).
/// For non-negative f32 costs, IEEE-754 bit order is strictly monotonic; returns 4 winning indices (0..7)
/// with result[0] guaranteed to be the global minimum.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn select_top4_of_8_fast(pos_bits: [u32; 4], neg_bits: [u32; 4]) -> [usize; 4] {
    let mut l0 = ((pos_bits[0] as u64) << 3) | 0;
    let mut l1 = ((pos_bits[1] as u64) << 3) | 1;
    let mut l2 = ((pos_bits[2] as u64) << 3) | 2;
    let mut l3 = ((pos_bits[3] as u64) << 3) | 3;
    let mut h0 = ((neg_bits[0] as u64) << 3) | 4;
    let mut h1 = ((neg_bits[1] as u64) << 3) | 5;
    let mut h2 = ((neg_bits[2] as u64) << 3) | 6;
    let mut h3 = ((neg_bits[3] as u64) << 3) | 7;

    // 4 parent-pair comparisons
    cmp_swap_u64(&mut l0, &mut h0);
    cmp_swap_u64(&mut l1, &mut h1);
    cmp_swap_u64(&mut l2, &mut h2);
    cmp_swap_u64(&mut l3, &mut h3);

    // 5-op sorting network on l0..l3
    cmp_swap_u64(&mut l0, &mut l1);
    cmp_swap_u64(&mut l2, &mut l3);
    cmp_swap_u64(&mut l0, &mut l2);
    cmp_swap_u64(&mut l1, &mut l3);
    cmp_swap_u64(&mut l1, &mut l2);

    // 5-op sorting network on h0..h3
    cmp_swap_u64(&mut h0, &mut h1);
    cmp_swap_u64(&mut h2, &mut h3);
    cmp_swap_u64(&mut h0, &mut h2);
    cmp_swap_u64(&mut h1, &mut h3);
    cmp_swap_u64(&mut h1, &mut h2);

    // 3-op bitonic merge
    let w0 = l0;
    let w1 = l1.min(h2);
    let w2 = l2.min(h1);
    let w3 = l3.min(h0);
    [(w0 & 7) as usize, (w1 & 7) as usize, (w2 & 7) as usize, (w3 & 7) as usize]
}

/// AVX2 + FMA 4-wide SoA Trellis kernel for ASDM5EC (M = 4 paths in parallel YMM registers, L = 32).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
unsafe fn modulate_channel5_ec_avx2(
    pcm: &[f64],
    in_len: usize,
    start_bit: usize,
    total_bytes: usize,
    p: Ciff5Coeffs,
    dither_type: DitherType,
    ditherer: &mut DsdChannelDither,
    w_period: usize,
    beam: &mut [TrellisPath5; EC_BEAM_WIDTH],
    ch_out: &mut [u8],
) {
    use std::arch::x86_64::*;

    let scale = CIFFR_5_SCALE;
    let clamp = CIFFR_5_CLAMP;
    let inv_w = 1.0 / w_period as f64;
    let mut cur_idx = start_bit / w_period;
    let mut cur_phi = start_bit - cur_idx * w_period;
    let (mut r0, mut r1, mut r2, mut r3) = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
    for _ in 0..cur_phi {
        r0 += r1; r1 += r2; r2 += r3;
    }

    let mut prng = if ditherer.prng1 == 0 { 0x9e37_79b9_7f4a_7c15 } else { ditherer.prng1 };
    let use_fast_gauss = matches!(dither_type, DitherType::Gauss);
    let use_no_dither = matches!(dither_type, DitherType::None);

    // Load SoA state into YMM registers (lane 0..3 = beam[0..3])
    let mut vs1 = _mm256_setr_pd(beam[0].s1, beam[1].s1, beam[2].s1, beam[3].s1);
    let mut vs2 = _mm256_setr_pd(beam[0].s2, beam[1].s2, beam[2].s2, beam[3].s2);
    let mut vs3 = _mm256_setr_pd(beam[0].s3, beam[1].s3, beam[2].s3, beam[3].s3);
    let mut vs4 = _mm256_setr_pd(beam[0].s4, beam[1].s4, beam[2].s4, beam[3].s4);
    let mut vs5 = _mm256_setr_pd(beam[0].s5, beam[1].s5, beam[2].s5, beam[3].s5);
    let mut vy = _mm256_setr_pd(beam[0].y, beam[1].y, beam[2].y, beam[3].y);
    let mut vcost = _mm256_setr_pd(
        beam[0].cost.min(1e20),
        beam[1].cost.min(1e20),
        beam[2].cost.min(1e20),
        beam[3].cost.min(1e20),
    );
    let mut bits = [beam[0].bits, beam[1].bits, beam[2].bits, beam[3].bits];

    let vleak = _mm256_set1_pd(0.999999);
    let vc1 = _mm256_set1_pd(CIFFR_5_C[0]);
    let vc2 = _mm256_set1_pd(CIFFR_5_C[1]);
    let vc3 = _mm256_set1_pd(CIFFR_5_C[2]);
    let vc4 = _mm256_set1_pd(CIFFR_5_C[3]);
    let vc5 = _mm256_set1_pd(CIFFR_5_C[4]);
    let vg1 = _mm256_set1_pd(p.g1);
    let vg2 = _mm256_set1_pd(p.g2);
    let va1 = _mm256_set1_pd(p.a[0]);
    let va2 = _mm256_set1_pd(p.a[1]);
    let va3 = _mm256_set1_pd(p.a[2]);
    let va4 = _mm256_set1_pd(p.a[3]);
    let va5 = _mm256_set1_pd(p.a[4]);
    let vone = _mm256_set1_pd(1.0);
    let vzero = _mm256_setzero_pd();
    let vhalf = _mm256_set1_pd(0.5);
    let vthresh = _mm256_set1_pd(3.5);
    let vsign_mask = _mm256_set1_pd(-0.0);
    let vclamp_pos = _mm256_set1_pd(clamp);
    let vclamp_neg = _mm256_set1_pd(-clamp);

    let mut c_all = [0.0f64; 8];
    let mut pos_bits = [0u32; 4];
    let mut neg_bits = [0u32; 4];

    for byte_idx in 0..total_bytes {
        let mut byte_val = 0u8;
        for bit in 0..8 {
            let u = r0;
            r0 += r1; r1 += r2; r2 += r3;
            cur_phi += 1;
            if cur_phi == w_period {
                cur_phi = 0;
                cur_idx += 1;
                let d = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
                r0 = d.0; r1 = d.1; r2 = d.2; r3 = d.3;
            }

            let dither = if use_fast_gauss {
                next_reg_gauss(&mut prng)
            } else if use_no_dither {
                0.0
            } else {
                ditherer.next_dither(dither_type)
            };

            let vu = _mm256_set1_pd(u);
            let ns1 = _mm256_fmadd_pd(vs1, vleak, _mm256_mul_pd(vc1, _mm256_sub_pd(vu, vy)));
            let ns2 = _mm256_fnmadd_pd(vg1, vs3, _mm256_fmadd_pd(vs2, vleak, _mm256_mul_pd(vc2, ns1)));
            let ns3 = _mm256_fmadd_pd(vs3, vleak, _mm256_mul_pd(vc3, ns2));
            let ns4 = _mm256_fnmadd_pd(vg2, vs5, _mm256_fmadd_pd(vs4, vleak, _mm256_mul_pd(vc4, ns3)));
            let ns5 = _mm256_fmadd_pd(vs5, vleak, _mm256_mul_pd(vc5, ns4));

            let mut sigma = _mm256_set1_pd(u + dither);
            sigma = _mm256_fmadd_pd(va1, ns1, sigma);
            sigma = _mm256_fmadd_pd(va2, ns2, sigma);
            sigma = _mm256_fmadd_pd(va3, ns3, sigma);
            sigma = _mm256_fmadd_pd(va4, ns4, sigma);
            sigma = _mm256_fmadd_pd(va5, ns5, sigma);

            let abs_s5 = _mm256_andnot_pd(vsign_mask, ns5);
            let ex = _mm256_max_pd(_mm256_sub_pd(abs_s5, vthresh), vzero);
            let base = _mm256_fmadd_pd(vhalf, _mm256_mul_pd(ex, ex), vcost);

            let e_pos = _mm256_sub_pd(sigma, vone);
            let e_neg = _mm256_add_pd(sigma, vone);
            let vc_pos = _mm256_fmadd_pd(e_pos, e_pos, base);
            let vc_neg = _mm256_fmadd_pd(e_neg, e_neg, base);

            _mm256_storeu_pd(c_all.as_mut_ptr(), vc_pos);
            _mm256_storeu_pd(c_all.as_mut_ptr().add(4), vc_neg);
            _mm_storeu_si128(pos_bits.as_mut_ptr() as *mut __m128i, _mm_castps_si128(_mm256_cvtpd_ps(vc_pos)));
            _mm_storeu_si128(neg_bits.as_mut_ptr() as *mut __m128i, _mm_castps_si128(_mm256_cvtpd_ps(vc_neg)));

            let w = select_top4_of_8_fast(pos_bits, neg_bits);
            let p0 = w[0] & 3;
            let p1 = w[1] & 3;
            let p2 = w[2] & 3;
            let p3 = w[3] & 3;

            let vperm = _mm256_setr_epi64x(
                LANE_PERM_64[p0],
                LANE_PERM_64[p1],
                LANE_PERM_64[p2],
                LANE_PERM_64[p3],
            );
            vs1 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns1), vperm));
            vs2 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns2), vperm));
            vs3 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns3), vperm));
            vs4 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns4), vperm));
            vs5 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns5), vperm));

            let b0 = ((w[0] & 4) == 0) as usize;
            let b1 = ((w[1] & 4) == 0) as usize;
            let b2 = ((w[2] & 4) == 0) as usize;
            let b3 = ((w[3] & 4) == 0) as usize;
            let y_mask = b0 | (b1 << 1) | (b2 << 2) | (b3 << 3);
            vy = _mm256_loadu_pd(Y_MASK_LUT[y_mask].as_ptr());

            let nb0 = (bits[p0] << 1) | (b0 as u64);
            let nb1 = (bits[p1] << 1) | (b1 as u64);
            let nb2 = (bits[p2] << 1) | (b2 as u64);
            let nb3 = (bits[p3] << 1) | (b3 as u64);
            bits = [nb0, nb1, nb2, nb3];

            let out_bit = ((nb0 >> (EC_TRACEBACK - 1)) & 1) as u8;
            byte_val |= out_bit << bit;

            // Ancestor pruning: penalize any lane whose bit at EC_TRACEBACK-1 disagrees with out_bit
            let c0 = c_all[w[0]];
            let c1 = if (((nb1 >> (EC_TRACEBACK - 1)) & 1) as u8) != out_bit { 1e20 } else { c_all[w[1]] };
            let c2 = if (((nb2 >> (EC_TRACEBACK - 1)) & 1) as u8) != out_bit { 1e20 } else { c_all[w[2]] };
            let c3 = if (((nb3 >> (EC_TRACEBACK - 1)) & 1) as u8) != out_bit { 1e20 } else { c_all[w[3]] };
            vcost = _mm256_setr_pd(c0, c1, c2, c3);
        }

        // Per-byte cost re-zeroing (clamped >= 0.0 for monotonic f32 bit sorting) and rare stability clamp
        let base_c = _mm256_broadcastsd_pd(_mm256_castpd256_pd128(vcost));
        vcost = _mm256_max_pd(_mm256_sub_pd(vcost, base_c), vzero);

        let abs1 = _mm256_andnot_pd(vsign_mask, vs1);
        let abs5 = _mm256_andnot_pd(vsign_mask, vs5);
        let ov = _mm256_or_pd(
            _mm256_cmp_pd(abs1, vclamp_pos, _CMP_GT_OQ),
            _mm256_cmp_pd(abs5, vclamp_pos, _CMP_GT_OQ),
        );
        if _mm256_movemask_pd(ov) != 0 {
            vs1 = _mm256_min_pd(_mm256_max_pd(vs1, vclamp_neg), vclamp_pos);
            vs2 = _mm256_min_pd(_mm256_max_pd(vs2, vclamp_neg), vclamp_pos);
            vs3 = _mm256_min_pd(_mm256_max_pd(vs3, vclamp_neg), vclamp_pos);
            vs4 = _mm256_min_pd(_mm256_max_pd(vs4, vclamp_neg), vclamp_pos);
            vs5 = _mm256_min_pd(_mm256_max_pd(vs5, vclamp_neg), vclamp_pos);
        }
        ch_out[byte_idx] = byte_val;
    }

    if use_fast_gauss {
        ditherer.prng1 = prng;
    }

    let mut a1 = [0.0f64; 4]; let mut a2 = [0.0f64; 4]; let mut a3 = [0.0f64; 4];
    let mut a4 = [0.0f64; 4]; let mut a5 = [0.0f64; 4];
    let mut ay = [0.0f64; 4]; let mut ac = [0.0f64; 4];
    _mm256_storeu_pd(a1.as_mut_ptr(), vs1);
    _mm256_storeu_pd(a2.as_mut_ptr(), vs2);
    _mm256_storeu_pd(a3.as_mut_ptr(), vs3);
    _mm256_storeu_pd(a4.as_mut_ptr(), vs4);
    _mm256_storeu_pd(a5.as_mut_ptr(), vs5);
    _mm256_storeu_pd(ay.as_mut_ptr(), vy);
    _mm256_storeu_pd(ac.as_mut_ptr(), vcost);
    for k in 0..4 {
        beam[k] = TrellisPath5 {
            s1: a1[k], s2: a2[k], s3: a3[k], s4: a4[k], s5: a5[k],
            y: ay[k],
            cost: if ac[k] >= 1e19 { 1e30 } else { ac[k] },
            bits: bits[k],
        };
    }
}

/// AVX2 + FMA 4-wide SoA Trellis kernel for ASDM7EC (M = 4 paths in parallel YMM registers, L = 32).
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(clippy::too_many_arguments)]
unsafe fn modulate_channel7_ec_avx2(
    pcm: &[f64],
    in_len: usize,
    start_bit: usize,
    total_bytes: usize,
    p: Ciff7Coeffs,
    dither_type: DitherType,
    ditherer: &mut DsdChannelDither,
    w_period: usize,
    beam: &mut [TrellisPath7; EC_BEAM_WIDTH],
    ch_out: &mut [u8],
) {
    use std::arch::x86_64::*;

    let scale = CIFFR_7_SCALE;
    let clamp = CIFFR_7_CLAMP;
    let inv_w = 1.0 / w_period as f64;
    let mut cur_idx = start_bit / w_period;
    let mut cur_phi = start_bit - cur_idx * w_period;
    let (mut r0, mut r1, mut r2, mut r3) = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
    for _ in 0..cur_phi {
        r0 += r1; r1 += r2; r2 += r3;
    }

    let mut prng = if ditherer.prng1 == 0 { 0x9e37_79b9_7f4a_7c15 } else { ditherer.prng1 };
    let use_fast_gauss = matches!(dither_type, DitherType::Gauss);
    let use_no_dither = matches!(dither_type, DitherType::None);

    let mut vs1 = _mm256_setr_pd(beam[0].s1, beam[1].s1, beam[2].s1, beam[3].s1);
    let mut vs2 = _mm256_setr_pd(beam[0].s2, beam[1].s2, beam[2].s2, beam[3].s2);
    let mut vs3 = _mm256_setr_pd(beam[0].s3, beam[1].s3, beam[2].s3, beam[3].s3);
    let mut vs4 = _mm256_setr_pd(beam[0].s4, beam[1].s4, beam[2].s4, beam[3].s4);
    let mut vs5 = _mm256_setr_pd(beam[0].s5, beam[1].s5, beam[2].s5, beam[3].s5);
    let mut vs6 = _mm256_setr_pd(beam[0].s6, beam[1].s6, beam[2].s6, beam[3].s6);
    let mut vs7 = _mm256_setr_pd(beam[0].s7, beam[1].s7, beam[2].s7, beam[3].s7);
    let mut vy = _mm256_setr_pd(beam[0].y, beam[1].y, beam[2].y, beam[3].y);
    let mut vcost = _mm256_setr_pd(
        beam[0].cost.min(1e20),
        beam[1].cost.min(1e20),
        beam[2].cost.min(1e20),
        beam[3].cost.min(1e20),
    );
    let mut bits = [beam[0].bits, beam[1].bits, beam[2].bits, beam[3].bits];

    let vleak = _mm256_set1_pd(0.999999);
    let vc1 = _mm256_set1_pd(CIFFR_7_C[0]);
    let vc2 = _mm256_set1_pd(CIFFR_7_C[1]);
    let vc3 = _mm256_set1_pd(CIFFR_7_C[2]);
    let vc4 = _mm256_set1_pd(CIFFR_7_C[3]);
    let vc5 = _mm256_set1_pd(CIFFR_7_C[4]);
    let vc6 = _mm256_set1_pd(CIFFR_7_C[5]);
    let vc7 = _mm256_set1_pd(CIFFR_7_C[6]);
    let vg1 = _mm256_set1_pd(p.g1);
    let vg2 = _mm256_set1_pd(p.g2);
    let vg3 = _mm256_set1_pd(p.g3);
    let va1 = _mm256_set1_pd(p.a[0]);
    let va2 = _mm256_set1_pd(p.a[1]);
    let va3 = _mm256_set1_pd(p.a[2]);
    let va4 = _mm256_set1_pd(p.a[3]);
    let va5 = _mm256_set1_pd(p.a[4]);
    let va6 = _mm256_set1_pd(p.a[5]);
    let va7 = _mm256_set1_pd(p.a[6]);
    let vone = _mm256_set1_pd(1.0);
    let vzero = _mm256_setzero_pd();
    let vhalf = _mm256_set1_pd(0.5);
    let vthresh = _mm256_set1_pd(3.5);
    let vsign_mask = _mm256_set1_pd(-0.0);
    let vclamp_pos = _mm256_set1_pd(clamp);
    let vclamp_neg = _mm256_set1_pd(-clamp);

    let mut c_all = [0.0f64; 8];
    let mut pos_bits = [0u32; 4];
    let mut neg_bits = [0u32; 4];

    for byte_idx in 0..total_bytes {
        let mut byte_val = 0u8;
        for bit in 0..8 {
            let u = r0;
            r0 += r1; r1 += r2; r2 += r3;
            cur_phi += 1;
            if cur_phi == w_period {
                cur_phi = 0;
                cur_idx += 1;
                let d = load_hermite_forward_diffs(pcm, in_len, cur_idx, inv_w, scale);
                r0 = d.0; r1 = d.1; r2 = d.2; r3 = d.3;
            }

            let dither = if use_fast_gauss {
                next_reg_gauss(&mut prng)
            } else if use_no_dither {
                0.0
            } else {
                ditherer.next_dither(dither_type)
            };

            let vu = _mm256_set1_pd(u);
            let ns1 = _mm256_fmadd_pd(vs1, vleak, _mm256_mul_pd(vc1, _mm256_sub_pd(vu, vy)));
            let ns2 = _mm256_fnmadd_pd(vg1, vs3, _mm256_fmadd_pd(vs2, vleak, _mm256_mul_pd(vc2, ns1)));
            let ns3 = _mm256_fmadd_pd(vs3, vleak, _mm256_mul_pd(vc3, ns2));
            let ns4 = _mm256_fnmadd_pd(vg2, vs5, _mm256_fmadd_pd(vs4, vleak, _mm256_mul_pd(vc4, ns3)));
            let ns5 = _mm256_fmadd_pd(vs5, vleak, _mm256_mul_pd(vc5, ns4));
            let ns6 = _mm256_fnmadd_pd(vg3, vs7, _mm256_fmadd_pd(vs6, vleak, _mm256_mul_pd(vc6, ns5)));
            let ns7 = _mm256_fmadd_pd(vs7, vleak, _mm256_mul_pd(vc7, ns6));

            let mut sigma = _mm256_set1_pd(u + dither);
            sigma = _mm256_fmadd_pd(va1, ns1, sigma);
            sigma = _mm256_fmadd_pd(va2, ns2, sigma);
            sigma = _mm256_fmadd_pd(va3, ns3, sigma);
            sigma = _mm256_fmadd_pd(va4, ns4, sigma);
            sigma = _mm256_fmadd_pd(va5, ns5, sigma);
            sigma = _mm256_fmadd_pd(va6, ns6, sigma);
            sigma = _mm256_fmadd_pd(va7, ns7, sigma);

            let abs_s7 = _mm256_andnot_pd(vsign_mask, ns7);
            let ex = _mm256_max_pd(_mm256_sub_pd(abs_s7, vthresh), vzero);
            let base = _mm256_fmadd_pd(vhalf, _mm256_mul_pd(ex, ex), vcost);

            let e_pos = _mm256_sub_pd(sigma, vone);
            let e_neg = _mm256_add_pd(sigma, vone);
            let vc_pos = _mm256_fmadd_pd(e_pos, e_pos, base);
            let vc_neg = _mm256_fmadd_pd(e_neg, e_neg, base);

            _mm256_storeu_pd(c_all.as_mut_ptr(), vc_pos);
            _mm256_storeu_pd(c_all.as_mut_ptr().add(4), vc_neg);
            _mm_storeu_si128(pos_bits.as_mut_ptr() as *mut __m128i, _mm_castps_si128(_mm256_cvtpd_ps(vc_pos)));
            _mm_storeu_si128(neg_bits.as_mut_ptr() as *mut __m128i, _mm_castps_si128(_mm256_cvtpd_ps(vc_neg)));

            let w = select_top4_of_8_fast(pos_bits, neg_bits);
            let p0 = w[0] & 3;
            let p1 = w[1] & 3;
            let p2 = w[2] & 3;
            let p3 = w[3] & 3;

            let vperm = _mm256_setr_epi64x(
                LANE_PERM_64[p0],
                LANE_PERM_64[p1],
                LANE_PERM_64[p2],
                LANE_PERM_64[p3],
            );
            vs1 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns1), vperm));
            vs2 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns2), vperm));
            vs3 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns3), vperm));
            vs4 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns4), vperm));
            vs5 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns5), vperm));
            vs6 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns6), vperm));
            vs7 = _mm256_castsi256_pd(_mm256_permutevar8x32_epi32(_mm256_castpd_si256(ns7), vperm));

            let b0 = ((w[0] & 4) == 0) as usize;
            let b1 = ((w[1] & 4) == 0) as usize;
            let b2 = ((w[2] & 4) == 0) as usize;
            let b3 = ((w[3] & 4) == 0) as usize;
            let y_mask = b0 | (b1 << 1) | (b2 << 2) | (b3 << 3);
            vy = _mm256_loadu_pd(Y_MASK_LUT[y_mask].as_ptr());

            let nb0 = (bits[p0] << 1) | (b0 as u64);
            let nb1 = (bits[p1] << 1) | (b1 as u64);
            let nb2 = (bits[p2] << 1) | (b2 as u64);
            let nb3 = (bits[p3] << 1) | (b3 as u64);
            bits = [nb0, nb1, nb2, nb3];

            let out_bit = ((nb0 >> (EC_TRACEBACK - 1)) & 1) as u8;
            byte_val |= out_bit << bit;

            let c0 = c_all[w[0]];
            let c1 = if (((nb1 >> (EC_TRACEBACK - 1)) & 1) as u8) != out_bit { 1e20 } else { c_all[w[1]] };
            let c2 = if (((nb2 >> (EC_TRACEBACK - 1)) & 1) as u8) != out_bit { 1e20 } else { c_all[w[2]] };
            let c3 = if (((nb3 >> (EC_TRACEBACK - 1)) & 1) as u8) != out_bit { 1e20 } else { c_all[w[3]] };
            vcost = _mm256_setr_pd(c0, c1, c2, c3);
        }

        let base_c = _mm256_broadcastsd_pd(_mm256_castpd256_pd128(vcost));
        vcost = _mm256_max_pd(_mm256_sub_pd(vcost, base_c), vzero);

        let abs1 = _mm256_andnot_pd(vsign_mask, vs1);
        let abs7 = _mm256_andnot_pd(vsign_mask, vs7);
        let ov = _mm256_or_pd(
            _mm256_cmp_pd(abs1, vclamp_pos, _CMP_GT_OQ),
            _mm256_cmp_pd(abs7, vclamp_pos, _CMP_GT_OQ),
        );
        if _mm256_movemask_pd(ov) != 0 {
            vs1 = _mm256_min_pd(_mm256_max_pd(vs1, vclamp_neg), vclamp_pos);
            vs2 = _mm256_min_pd(_mm256_max_pd(vs2, vclamp_neg), vclamp_pos);
            vs3 = _mm256_min_pd(_mm256_max_pd(vs3, vclamp_neg), vclamp_pos);
            vs4 = _mm256_min_pd(_mm256_max_pd(vs4, vclamp_neg), vclamp_pos);
            vs5 = _mm256_min_pd(_mm256_max_pd(vs5, vclamp_neg), vclamp_pos);
            vs6 = _mm256_min_pd(_mm256_max_pd(vs6, vclamp_neg), vclamp_pos);
            vs7 = _mm256_min_pd(_mm256_max_pd(vs7, vclamp_neg), vclamp_pos);
        }
        ch_out[byte_idx] = byte_val;
    }

    if use_fast_gauss {
        ditherer.prng1 = prng;
    }

    let mut a1 = [0.0f64; 4]; let mut a2 = [0.0f64; 4]; let mut a3 = [0.0f64; 4];
    let mut a4 = [0.0f64; 4]; let mut a5 = [0.0f64; 4]; let mut a6 = [0.0f64; 4];
    let mut a7 = [0.0f64; 4]; let mut ay = [0.0f64; 4]; let mut ac = [0.0f64; 4];
    _mm256_storeu_pd(a1.as_mut_ptr(), vs1);
    _mm256_storeu_pd(a2.as_mut_ptr(), vs2);
    _mm256_storeu_pd(a3.as_mut_ptr(), vs3);
    _mm256_storeu_pd(a4.as_mut_ptr(), vs4);
    _mm256_storeu_pd(a5.as_mut_ptr(), vs5);
    _mm256_storeu_pd(a6.as_mut_ptr(), vs6);
    _mm256_storeu_pd(a7.as_mut_ptr(), vs7);
    _mm256_storeu_pd(ay.as_mut_ptr(), vy);
    _mm256_storeu_pd(ac.as_mut_ptr(), vcost);
    for k in 0..4 {
        beam[k] = TrellisPath7 {
            s1: a1[k], s2: a2[k], s3: a3[k], s4: a4[k],
            s5: a5[k], s6: a6[k], s7: a7[k],
            y: ay[k],
            cost: if ac[k] >= 1e19 { 1e30 } else { ac[k] },
            bits: bits[k],
        };
    }
}
