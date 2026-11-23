//! Quantization Dither & Noise Shaping Subsystem
//!
//! Provides mathematically rigorous, audiophile-grade quantization dither and noise shaping
//! for both high-precision PCM wordlength reduction (64-bit float -> 24-bit / 16-bit) and
//! 1-bit DSD Delta-Sigma modulation:
//!
//! 1. **TPDF (Triangular Probability Density Function)**:
//!    Formed by the difference of two independent uniform random variables (2 LSB peak-to-peak).
//!    Conforms to Lipshitz & Vanderkooy's Theorem: completely decorrelates both the first and
//!    second moments of quantization error from the input audio signal, eliminating harmonic
//!    distortion, intermodulation spurs, and signal-dependent noise modulation.
//!
//! 2. **HP-TPDF (High-Pass Filtered TPDF)**:
//!    Applies a differentiating filter H(z) = 1 - z^-1 to the triangular dither sequence.
//!    Shapes dither energy with a +6 dB/octave tilt, depressing noise in the ear's most sensitive
//!    frequency range (100 Hz - 4 kHz) by ~3.5 dB while tilting it towards high frequencies.
//!
//! 3. **Noise Shaped (Psychoacoustically Weighted Noise Shaping)**:
//!    Lipshitz-Vanderkooy / F-weighted 3rd-order error-feedback loop. Places a spectral notch
//!    at 3.5 kHz - 4 kHz (minimum audible threshold of human hearing, ISO 226), yielding
//!    an effective subjective noise floor reduction of 12 dB to 15 dB.
//!
//! 4. **Gaussian Dither (Box-Muller Normal Distribution)**:
//!    Generates true unbounded Gaussian noise N(0, sigma^2). Particularly vital for high-order
//!    Delta-Sigma modulators (DSD), completely disrupting internal integrator limit cycles and
//!    idle tones even during digital silence and DC offsets.
//!
//! 5. **None (Bit-Exact Truncation)**:
//!    Pass-through rounding without dither for measurement or bit-perfect testing.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Quantization Dither Strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DitherType {
    None,
    #[serde(alias = "tpdf")]
    Tpdf,
    #[serde(alias = "hp-tpdf", alias = "hptpdf")]
    HpTpdf,
    #[serde(alias = "shaped", alias = "noise-shaped", alias = "f-weighted")]
    NoiseShaped,
    #[serde(alias = "gauss", alias = "gauss1", alias = "gaussian")]
    Gauss,
}

impl Default for DitherType {
    fn default() -> Self {
        DitherType::Gauss
    }
}

impl DitherType {
    #[allow(non_upper_case_globals)]
    pub const Gauss1: DitherType = DitherType::Gauss;
    #[allow(non_upper_case_globals)]
    pub const Shaped: DitherType = DitherType::NoiseShaped;
    pub const GAUSS1: DitherType = DitherType::Gauss;
    pub const SHAPED: DitherType = DitherType::NoiseShaped;

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "tpdf" => Some(DitherType::Tpdf),
            "hp-tpdf" | "hptpdf" => Some(DitherType::HpTpdf),
            "shaped" | "noise-shaped" | "f-weighted" | "psychoacoustic" => Some(DitherType::NoiseShaped),
            "gauss" | "gauss1" | "gaussian" => Some(DitherType::Gauss),
            "none" | "off" => Some(DitherType::None),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DitherType::Tpdf => "TPDF (標準三角形分佈 - 錄音室無失真線性)",
            DitherType::HpTpdf => "HP-TPDF (高頻加權三角形 - 聽覺感度最佳化)",
            DitherType::NoiseShaped => "Noise-Shaped (人耳等響度權衡噪聲整形 - 極致漆黑背景)",
            DitherType::Gauss => "Gauss (高斯溫潤分佈 - 徹底打散極限環與閒置雜音)",
            DitherType::None => "關閉 (None - 直通截斷)",
        }
    }
}

/// Fast, lock-free 64-bit XorShift Pseudo-Random Number Generator
#[inline(always)]
pub fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    if x == 0 {
        x = 0x8a5cd78963421245;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Uniform floating point in [0, 1) with 53 bits of precision
#[inline(always)]
pub fn rand_u01(state: &mut u64) -> f64 {
    (xorshift64(state) >> 11) as f64 * (1.0 / 9007199254740992.0)
}

/// Uniform floating point in (-1, 1)
#[inline(always)]
pub fn rand_sym(state: &mut u64) -> f64 {
    rand_u01(state) * 2.0 - 1.0
}

/// Single channel PCM Dither State
#[derive(Debug, Clone)]
pub struct PcmChannelDither {
    pub prng1: u64,
    pub prng2: u64,
    pub prev_tpdf: f64,
    pub err_history: [f64; 3],
}

impl PcmChannelDither {
    pub fn new(seed1: u64, seed2: u64) -> Self {
        Self {
            prng1: if seed1 == 0 { 0x5a17_1932_74a8_b3c1 } else { seed1 },
            prng2: if seed2 == 0 { 0x3f98_2c41_d7e0_95a2 } else { seed2 },
            prev_tpdf: 0.0,
            err_history: [0.0; 3],
        }
    }

    /// Generate raw dither sample in LSB units (+-1.0 LSB for TPDF)
    #[inline]
    pub fn next_dither_lsb(&mut self, dither: DitherType) -> f64 {
        match dither {
            DitherType::None => 0.0,
            DitherType::Tpdf => {
                // Difference of two independent uniform random variables
                rand_u01(&mut self.prng1) - rand_u01(&mut self.prng2)
            }
            DitherType::HpTpdf => {
                let cur = rand_u01(&mut self.prng1) - rand_u01(&mut self.prng2);
                let hp = cur - self.prev_tpdf;
                self.prev_tpdf = cur;
                hp * 0.5
            }
            DitherType::NoiseShaped => {
                // TPDF base dither for error-feedback loop
                rand_u01(&mut self.prng1) - rand_u01(&mut self.prng2)
            }
            DitherType::Gauss => {
                // Box-Muller transform for normal distribution N(0, sigma^2), sigma = 0.5 LSB
                let u1 = rand_u01(&mut self.prng1).max(1e-15);
                let u2 = rand_u01(&mut self.prng2);
                let r = (-2.0 * u1.ln()).sqrt() * 0.5;
                r * (2.0 * PI * u2).cos()
            }
        }
    }

    /// Quantize a normalized sample [-1.0, 1.0] to 24-bit integer [-8388608, 8388607]
    #[inline]
    pub fn quantize_24bit(&mut self, sample: f64, dither: DitherType) -> i32 {
        const SCALE: f64 = 8388607.0;
        let x = sample.clamp(-1.0, 1.0) * SCALE;

        match dither {
            DitherType::None => (x.round() as i64).clamp(-8388608, 8388607) as i32,
            DitherType::Tpdf | DitherType::HpTpdf | DitherType::Gauss => {
                let d = self.next_dither_lsb(dither);
                ((x + d).round() as i64).clamp(-8388608, 8388607) as i32
            }
            DitherType::NoiseShaped => {
                // Lipshitz-Vanderkooy 3rd-order F-weighted coefficients
                // H_err(z) = 1.623 z^-1 - 0.982 z^-2 + 0.109 z^-3
                let feedback = 1.623 * self.err_history[0]
                    - 0.982 * self.err_history[1]
                    + 0.109 * self.err_history[2];

                let d = self.next_dither_lsb(DitherType::Tpdf);
                let input_with_fb = x - feedback;
                let target = input_with_fb + d;
                let q = (target.round() as i64).clamp(-8388608, 8388607);
                let err = (q as f64) - input_with_fb;

                self.err_history[2] = self.err_history[1];
                self.err_history[1] = self.err_history[0];
                self.err_history[0] = err;

                q as i32
            }
        }
    }

    /// Quantize a normalized sample [-1.0, 1.0] to 16-bit integer [-32768, 32767]
    #[inline]
    pub fn quantize_16bit(&mut self, sample: f64, dither: DitherType) -> i16 {
        const SCALE: f64 = 32767.0;
        let x = sample.clamp(-1.0, 1.0) * SCALE;

        match dither {
            DitherType::None => (x.round() as i64).clamp(-32768, 32767) as i16,
            DitherType::Tpdf | DitherType::HpTpdf | DitherType::Gauss => {
                let d = self.next_dither_lsb(dither);
                ((x + d).round() as i64).clamp(-32768, 32767) as i16
            }
            DitherType::NoiseShaped => {
                let feedback = 1.623 * self.err_history[0]
                    - 0.982 * self.err_history[1]
                    + 0.109 * self.err_history[2];

                let d = self.next_dither_lsb(DitherType::Tpdf);
                let input_with_fb = x - feedback;
                let target = input_with_fb + d;
                let q = (target.round() as i64).clamp(-32768, 32767);
                let err = (q as f64) - input_with_fb;

                self.err_history[2] = self.err_history[1];
                self.err_history[1] = self.err_history[0];
                self.err_history[0] = err;

                q as i16
            }
        }
    }
}

/// Stereo PCM Ditherer Engine
#[derive(Debug, Clone)]
pub struct StereoPcmDitherer {
    pub left: PcmChannelDither,
    pub right: PcmChannelDither,
}

impl Default for StereoPcmDitherer {
    fn default() -> Self {
        Self {
            left: PcmChannelDither::new(0x1234_5678_9abc_def0, 0xfedc_ba98_7654_3210),
            right: PcmChannelDither::new(0x0123_4567_89ab_cdef, 0x9876_5432_10fe_dcba),
        }
    }
}

impl StereoPcmDitherer {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn quantize_24bit(&mut self, l: f64, r: f64, dither: DitherType) -> (i32, i32) {
        (
            self.left.quantize_24bit(l, dither),
            self.right.quantize_24bit(r, dither),
        )
    }

    #[inline]
    pub fn quantize_16bit(&mut self, l: f64, r: f64, dither: DitherType) -> (i16, i16) {
        (
            self.left.quantize_16bit(l, dither),
            self.right.quantize_16bit(r, dither),
        )
    }
}

/// Single-channel DSD Modulator Ditherer State (allows lock-free parallel L/R modulation)
#[derive(Debug, Clone)]
pub struct DsdChannelDither {
    pub prng1: u64,
    pub prng2: u64,
    pub prev_tpdf: f64,
    pub err_history: [f64; 2],
}

impl DsdChannelDither {
    pub fn new(seed1: u64, seed2: u64) -> Self {
        Self {
            prng1: seed1,
            prng2: seed2,
            prev_tpdf: 0.0,
            err_history: [0.0; 2],
        }
    }

    /// Generate single-channel dither value for 1-bit Delta-Sigma comparator input.
    #[inline(always)]
    pub fn next_dither(&mut self, dither: DitherType) -> f64 {
        const DSD_DITHER_AMP: f64 = 0.0125;
        // Scale factor for 4-way 16-bit CLT Gaussian: (0.0125 * 0.707) / (32768 * sqrt(4/3))
        const GAUSS_CLT_SCALE: f64 = (DSD_DITHER_AMP * 0.707) / 37837.22;

        match dither {
            DitherType::None => 0.0,
            DitherType::Tpdf => {
                (rand_u01(&mut self.prng1) - rand_u01(&mut self.prng2)) * DSD_DITHER_AMP
            }
            DitherType::HpTpdf => {
                let cur = (rand_u01(&mut self.prng1) - rand_u01(&mut self.prng2)) * DSD_DITHER_AMP;
                let hp = cur - self.prev_tpdf;
                self.prev_tpdf = cur;
                hp * 0.5
            }
            DitherType::NoiseShaped => {
                let base = (rand_u01(&mut self.prng1) - rand_u01(&mut self.prng2)) * DSD_DITHER_AMP;
                let d = base - 1.5 * self.err_history[0] + 0.56 * self.err_history[1];
                self.err_history[1] = self.err_history[0];
                self.err_history[0] = base;
                d * 0.5
            }
            DitherType::Gauss => {
                // Ultra-fast 4-way 16-bit Irwin-Hall (CLT) Gaussian approximation from one xorshift64.
                // Eliminates ln/sqrt/sin_cos overhead at 22.58 MHz (DSD512) while completely breaking limit cycles.
                let r = xorshift64(&mut self.prng1);
                let sum = (r as i16 as i32)
                    + ((r >> 16) as i16 as i32)
                    + ((r >> 32) as i16 as i32)
                    + ((r >> 48) as i16 as i32);
                (sum as f64) * GAUSS_CLT_SCALE
            }
        }
    }
}

/// Stereo DSD Modulator Ditherer Engine
///
/// Injects precision dither into 1-bit Delta-Sigma comparator inputs with optimal scaling,
/// breaking up non-linear limit cycles and eliminating audible DC idle tones.
#[derive(Debug, Clone)]
pub struct DsdDitherer {
    pub left: DsdChannelDither,
    pub right: DsdChannelDither,
}

impl Default for DsdDitherer {
    fn default() -> Self {
        Self {
            left: DsdChannelDither::new(0x9e37_79b9_7f4a_7c15, 0xbf58_476d_1ce4_e5b9),
            right: DsdChannelDither::new(0x94d0_49bb_1331_11eb, 0x673c_9b65_708f_680b),
        }
    }
}

impl DsdDitherer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate stereo dither values for 1-bit Delta-Sigma comparator input.
    /// Scaled optimally for 1-bit unit feedback (comparator threshold ~ 0.0).
    #[inline]
    pub fn next_stereo_dither(&mut self, dither: DitherType) -> (f64, f64) {
        (self.left.next_dither(dither), self.right.next_dither(dither))
    }
}
