use std::f64::consts::PI;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BiquadType {
    Lowpass,
    Highpass,
    Peaking,
    Lowshelf,
    Highshelf,
    Notch,
    Bandpass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeqBand {
    #[serde(rename = "type")]
    pub filter_type: BiquadType,
    pub f0: f64,
    #[serde(default)]
    pub gain_db: f64,
    #[serde(default = "default_q")]
    pub q: f64,
}

fn default_q() -> f64 { 0.7071 }

#[derive(Debug, Clone)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

pub fn biquad_coeffs(filter_type: BiquadType, fs: f64, f0: f64, gain_db: f64, q: f64) -> Option<BiquadCoeffs> {
    if fs <= 0.0 || f0 <= 0.0 || f0 >= 0.499 * fs {
        return None;
    }
    let a_gain = 10.0f64.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * f0 / fs;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * q.max(1e-4));

    let (b0, b1, b2, a0, a1, a2) = match filter_type {
        BiquadType::Lowpass => {
            let b1 = 1.0 - cos_w0;
            let b0 = b1 / 2.0;
            let b2 = b0;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        BiquadType::Highpass => {
            let b1 = -(1.0 + cos_w0);
            let b0 = (1.0 + cos_w0) / 2.0;
            let b2 = b0;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        BiquadType::Peaking => {
            let b0 = 1.0 + alpha * a_gain;
            let b1 = -2.0 * cos_w0;
            let b2 = 1.0 - alpha * a_gain;
            let a0 = 1.0 + alpha / a_gain;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha / a_gain;
            (b0, b1, b2, a0, a1, a2)
        }
        BiquadType::Lowshelf => {
            let sqrt_a = a_gain.sqrt();
            let b0 = a_gain * ((a_gain + 1.0) - (a_gain - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
            let b1 = 2.0 * a_gain * ((a_gain - 1.0) - (a_gain + 1.0) * cos_w0);
            let b2 = a_gain * ((a_gain + 1.0) - (a_gain - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
            let a0 = (a_gain + 1.0) + (a_gain - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
            let a1 = -2.0 * ((a_gain - 1.0) + (a_gain + 1.0) * cos_w0);
            let a2 = (a_gain + 1.0) + (a_gain - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        BiquadType::Highshelf => {
            let sqrt_a = a_gain.sqrt();
            let b0 = a_gain * ((a_gain + 1.0) + (a_gain - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha);
            let b1 = -2.0 * a_gain * ((a_gain - 1.0) + (a_gain + 1.0) * cos_w0);
            let b2 = a_gain * ((a_gain + 1.0) + (a_gain - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha);
            let a0 = (a_gain + 1.0) - (a_gain - 1.0) * cos_w0 + 2.0 * sqrt_a * alpha;
            let a1 = 2.0 * ((a_gain - 1.0) - (a_gain + 1.0) * cos_w0);
            let a2 = (a_gain + 1.0) - (a_gain - 1.0) * cos_w0 - 2.0 * sqrt_a * alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        BiquadType::Notch => {
            let b0 = 1.0;
            let b1 = -2.0 * cos_w0;
            let b2 = 1.0;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
        BiquadType::Bandpass => {
            let b0 = alpha;
            let b1 = 0.0;
            let b2 = -alpha;
            let a0 = 1.0 + alpha;
            let a1 = -2.0 * cos_w0;
            let a2 = 1.0 - alpha;
            (b0, b1, b2, a0, a1, a2)
        }
    };

    Some(BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    })
}

#[derive(Debug, Clone)]
pub struct PeqChain {
    pub pre_gain_lin: f64,
    pub biquads: Vec<BiquadCoeffs>,
}

#[derive(Debug, Clone)]
pub struct PeqState {
    pub r1_l: Vec<f64>,
    pub r2_l: Vec<f64>,
    pub r1_r: Vec<f64>,
    pub r2_r: Vec<f64>,
}

impl PeqChain {
    pub fn new(fs: f64, pre_gain_db: f64, bands: &[PeqBand]) -> Self {
        let pre_gain_lin = 10.0f64.powf(pre_gain_db / 20.0);
        let mut biquads = Vec::new();
        for b in bands {
            if let Some(c) = biquad_coeffs(b.filter_type, fs, b.f0, b.gain_db, b.q) {
                biquads.push(c);
            }
        }
        Self { pre_gain_lin, biquads }
    }

    pub fn create_state(&self) -> PeqState {
        let n = self.biquads.len();
        PeqState {
            r1_l: vec![0.0; n],
            r2_l: vec![0.0; n],
            r1_r: vec![0.0; n],
            r2_r: vec![0.0; n],
        }
    }

    #[inline]
    pub fn process_pair(&self, st: &mut PeqState, x_l: f64, x_r: f64) -> (f64, f64) {
        let mut l = x_l * self.pre_gain_lin;
        let mut r = x_r * self.pre_gain_lin;

        for (i, b) in self.biquads.iter().enumerate() {
            // Direct Form II Transposed - Left Channel
            let y_l = b.b0 * l + st.r1_l[i];
            st.r1_l[i] = b.b1 * l - b.a1 * y_l + st.r2_l[i];
            st.r2_l[i] = b.b2 * l - b.a2 * y_l;
            l = y_l;

            // Direct Form II Transposed - Right Channel
            let y_r = b.b0 * r + st.r1_r[i];
            st.r1_r[i] = b.b1 * r - b.a1 * y_r + st.r2_r[i];
            st.r2_r[i] = b.b2 * r - b.a2 * y_r;
            r = y_r;
        }

        (l, r)
    }

    pub fn apply_arrays(&self, in_l: &[f64], in_r: &[f64], out_l: &mut [f64], out_r: &mut [f64]) {
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        let mut st = self.create_state();
        for i in 0..n {
            let (ol, or) = self.process_pair(&mut st, in_l[i], in_r[i]);
            out_l[i] = ol;
            out_r[i] = or;
        }
    }
}
