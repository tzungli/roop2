use std::f64::consts::PI;
use serde::{Deserialize, Serialize};

#[inline]
pub fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let half_x = x / 2.0;
    for k in 1..=60 {
        term *= half_x / k as f64;
        let t2 = term * term;
        sum += t2;
        if t2 < sum * 1e-17 {
            break;
        }
    }
    sum
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterpolatePhase {
    Linear,
    Minimum,
    Intermediate,
}

#[inline]
pub fn interpolate_sample(p0: f64, p1: f64, p2: f64, p3: f64, t: f64, phase: InterpolatePhase) -> f64 {
    match phase {
        InterpolatePhase::Minimum => interpolate_minimum(p0, p1, p2, p3, t),
        InterpolatePhase::Linear => interpolate_linear(p0, p1, p2, p3, t),
        InterpolatePhase::Intermediate => interpolate_intermediate(p0, p1, p2, p3, t),
    }
}

#[inline]
pub fn interpolate_linear(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    let m1 = 0.5 * (p2 - p0);
    let m2 = 0.5 * (p3 - p1);
    let y = h00 * p1 + h10 * m1 + h01 * p2 + h11 * m2;
    y.clamp(-1.0, 1.0)
}

#[inline]
pub fn interpolate_minimum(p0: f64, p1: f64, p2: f64, _p3: f64, t: f64) -> f64 {
    let d0 = p1 - p0;
    let d1 = p2 - p1;
    let y = p1 + t * d1 + 0.5 * t * (t - 1.0) * (0.85 * d0 + 0.15 * d1);
    y.clamp(-1.0, 1.0)
}

#[inline]
pub fn interpolate_intermediate(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    let m1 = 0.5 * (p2 - p0);
    let m2 = 0.5 * (p3 - p1);
    let y_lin = h00 * p1 + h10 * m1 + h01 * p2 + h11 * m2;
    let d0 = p1 - p0;
    let d1 = p2 - p1;
    let y_min = p1 + t * d1 + 0.5 * t * (t - 1.0) * (0.85 * d0 + 0.15 * d1);
    let y = 0.35 * y_lin + 0.65 * y_min;
    y.clamp(-1.0, 1.0)
}

pub fn get_hermite_tab(period: usize) -> Vec<f64> {
    let mut tab = vec![0.0; period * 4];
    for k in 0..period {
        let t = k as f64 / period as f64;
        let t2 = t * t;
        let t3 = t2 * t;
        let o = k * 4;
        tab[o] = 2.0 * t3 - 3.0 * t2 + 1.0;
        tab[o + 1] = t3 - 2.0 * t2 + t;
        tab[o + 2] = -2.0 * t3 + 3.0 * t2;
        tab[o + 3] = t3 - t2;
    }
    tab
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseEntry {
    pub k0: i32,
    pub t: usize,
    pub g: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyphaseBank {
    pub p: usize,
    pub q: usize,
    pub entries: Vec<PhaseEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirFilter {
    pub h: Vec<f64>,
    pub s: usize,
    pub lo: i32,
    pub hi: i32,
    pub ratio: f64,
    pub integer_ratio: bool,
    pub fs_out: f64,
    pub src_rate: f64,
    pub minimum_phase: bool,
    pub taps: usize,
    pub poly_phases: Option<PolyphaseBank>,
}

pub fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

pub fn build_fir_phases(gf: &FirFilter) -> Option<PolyphaseBank> {
    let fs_out = gf.fs_out.round() as usize;
    let src_rate = gf.src_rate.round() as usize;
    if fs_out == 0 || src_rate == 0 {
        return None;
    }
    let g = gcd(fs_out, src_rate);
    let p = fs_out / g;
    let q = src_rate / g;

    if p < 1 || p > 4096 {
        return None;
    }

    let h = &gf.h;
    let s = gf.s as i64;
    let lo = gf.lo as i64;
    let hi = gf.hi as i64;
    let max_j = (hi - lo) * s;
    let p_i64 = p as i64;
    let q_i64 = q as i64;

    let mut entries = Vec::with_capacity(p);

    for phi in 0..p {
        let phi_i64 = phi as i64;
        let k0 = (((phi_i64 - hi) * q_i64) as f64 / p_i64 as f64).ceil() as i64;
        let a = phi_i64 * q_i64 - k0 * p_i64 - lo * q_i64; // exact integer = (t0 - lo) * Q
        let t = (a / p_i64) + 1;

        if t < 1 || t > 8192 {
            entries.push(PhaseEntry {
                k0: 0,
                t: 0,
                g: Vec::new(),
            });
            continue;
        }

        let n0 = s * a;
        let sp = s * p_i64;
        let t_usize = t as usize;
        let mut g_vec = vec![0.0f64; t_usize];

        for j in 0..t_usize {
            let num = n0 - (j as i64) * sp;
            let mut j0 = num / q_i64;
            if j0 < 0 {
                j0 = 0;
            } else if j0 > max_j {
                j0 = max_j;
            }
            let rem = num - j0 * q_i64;
            let j0_u = j0 as usize;
            if rem <= 0 {
                g_vec[j] = if j0_u < h.len() { h[j0_u] } else { 0.0 };
            } else {
                let h0 = if j0_u < h.len() { h[j0_u] } else { 0.0 };
                let h1 = if j0_u + 1 < h.len() { h[j0_u + 1] } else { h0 };
                g_vec[j] = h0 + (rem as f64 / q as f64) * (h1 - h0);
            }
        }

        entries.push(PhaseEntry {
            k0: k0 as i32,
            t: t_usize,
            g: g_vec,
        });
    }

    Some(PolyphaseBank {
        p,
        q,
        entries,
    })
}

#[inline]
pub fn fir_sample(input: &[f64], in_len: usize, out_idx: usize, ratio: f64, gf: &FirFilter) -> f64 {
    // Fast path: contiguous polyphase scan with precomputed bank
    if let Some(ph) = &gf.poly_phases {
        let phi = out_idx % ph.p;
        let entry = &ph.entries[phi];
        let k_base = entry.k0 + (out_idx / ph.p) as i32 * ph.q as i32;
        let t = entry.t;
        if k_base >= 0 && (k_base as usize + t) <= in_len {
            let mut acc = 0.0;
            let start = k_base as usize;
            for j in 0..t {
                acc += input[start + j] * entry.g[j];
            }
            return acc;
        }
    }

    let h = &gf.h;
    let s = gf.s as f64;
    let lo = gf.lo;
    let hi = gf.hi;

    let mut k_start = ((out_idx as f64 - hi as f64) / ratio).ceil() as i32;
    let mut k_end = ((out_idx as f64 - lo as f64) / ratio).floor() as i32;
    if k_start < 0 {
        k_start = 0;
    }
    if k_end > in_len.saturating_sub(1) as i32 {
        k_end = in_len.saturating_sub(1) as i32;
    }
    if k_start > k_end {
        return 0.0;
    }

    let mut acc = 0.0;
    if gf.integer_ratio {
        let r = ratio.round() as i32;
        let mut m = out_idx as i32 - k_start * r - lo;
        for k in k_start..=k_end {
            if m >= 0 && (m as usize) < h.len() {
                acc += input[k as usize] * h[m as usize];
            }
            m -= r;
        }
    } else {
        let mut t = out_idx as f64 - k_start as f64 * ratio;
        let max_j = (hi - lo) as f64 * s;
        for k in k_start..=k_end {
            let mut j = (t - lo as f64) * s;
            if j < 0.0 {
                j = 0.0;
            } else if j > max_j {
                j = max_j;
            }
            let j0 = j.floor() as usize;
            let frac = j - j0 as f64;
            if j0 + 1 < h.len() {
                acc += input[k as usize] * (h[j0] + frac * (h[j0 + 1] - h[j0]));
            } else if j0 < h.len() {
                acc += input[k as usize] * h[j0];
            }
            t -= ratio;
        }
    }
    acc
}

pub fn kaiser_sinc_fir(fs_out: f64, src_rate: f64, as_db: f64, fc_factor: f64, trans_factor: f64) -> FirFilter {
    let fc = (fc_factor * src_rate / 2.0).min(0.45 * fs_out);
    let trans = trans_factor * src_rate;
    let beta = 0.1102 * (as_db - 8.7);
    let l_cap = (150e6 / src_rate / 2.0).floor() as i32;
    let mut l = (((as_db - 7.95) * fs_out / (2.0 * 14.347 * trans)).ceil() as i32).min(l_cap);
    if l < 8 { l = 8; }

    let ratio = fs_out / src_rate;
    let integer_ratio = (ratio - ratio.round()).abs() < 1e-9;
    let s = if integer_ratio { 1 } else { 8 };

    let i0_beta = bessel_i0(beta);
    let ext_tap = |t: f64| -> f64 {
        if t.abs() > l as f64 {
            return 0.0;
        }
        let x = 2.0 * fc * t / fs_out;
        let s_val = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
        let u = t / l as f64;
        let w = bessel_i0(beta * (0.0f64.max(1.0 - u * u)).sqrt()) / i0_beta;
        (2.0 * fc / fs_out) * s_val * w
    };

    let half = l * s as i32;
    let n = (2 * half + 2) as usize;
    let mut h = vec![0.0; n];
    for j in -half..=half {
        h[(j + half) as usize] = ext_tap(j as f64 / s as f64);
    }
    let sum: f64 = h[..n - 1].iter().sum();
    let g = ratio * s as f64 / sum;
    for x in &mut h {
        *x *= g;
    }

    let mut filter = FirFilter {
        h,
        s,
        lo: -l,
        hi: l,
        ratio,
        integer_ratio,
        fs_out,
        src_rate,
        minimum_phase: false,
        taps: (2 * half + 1) as usize,
        poly_phases: None,
    };
    filter.poly_phases = build_fir_phases(&filter);
    filter
}

pub fn build_poly_ext2_filter(fs_out: f64, src_rate: f64) -> FirFilter {
    kaiser_sinc_fir(fs_out, src_rate, 140.0, 1.27, 0.136)
}

pub fn build_hqp_ps_ext2_filter(fs_out: f64, src_rate: f64) -> FirFilter {
    kaiser_sinc_fir(fs_out, src_rate, 180.0, 1.135, 0.135)
}

pub fn build_hq_poly_sinc_gauss_long_filter(fs_out: f64, src_rate: f64) -> FirFilter {
    let ratio = fs_out / src_rate;
    let integer_ratio = (ratio - ratio.round()).abs() < 1e-9;
    let s = if integer_ratio { 1 } else { 8 };

    let trans_bw = 6.0 * 512.5 * (src_rate / 44100.0);
    let fc = ((src_rate / 2.0) + (trans_bw / 2.0)).min(0.45 * fs_out);

    let half_input_span = 255.0;
    let l = (half_input_span * ratio).round().max(8.0) as i32;
    let sigma = 0.125 * l as f64;

    let half = l * s as i32;
    let n = (2 * half + 2) as usize;
    let mut h = vec![0.0; n];

    for j in -half..=half {
        let t = j as f64 / s as f64;
        let x = 2.0 * fc * t / fs_out;
        let sinc = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
        let w = (-0.5 * (t / sigma) * (t / sigma)).exp();
        h[(j + half) as usize] = (2.0 * fc / fs_out) * sinc * w;
    }

    let sum: f64 = h[..n - 1].iter().sum();
    let g = ratio * s as f64 / sum;
    for x in &mut h {
        *x *= g;
    }

    let mut filter = FirFilter {
        h,
        s,
        lo: -l,
        hi: l,
        ratio,
        integer_ratio,
        fs_out,
        src_rate,
        minimum_phase: false,
        taps: (2 * half + 1) as usize,
        poly_phases: None,
    };
    filter.poly_phases = build_fir_phases(&filter);
    filter
}

pub fn build_gauss_filter(fs_out: f64, src_rate: f64, minimum_phase: bool) -> FirFilter {
    let fc = (1.27 * src_rate / 2.0).min(0.45 * fs_out);
    let mut sigma = 180.0 * (src_rate / 44100.0) * (fs_out / 2822400.0);
    if sigma < 3.0 { sigma = 3.0; }
    let l_cap = (150e6 / src_rate / 2.0).floor() as i32;
    let mut l = (4.5 * sigma).ceil().min(l_cap.max((2.5 * sigma).ceil() as i32) as f64) as i32;
    if l < 8 { l = 8; }

    let ratio = fs_out / src_rate;
    let integer_ratio = (ratio - ratio.round()).abs() < 1e-9;
    let s = if integer_ratio { 1 } else { 8 };

    let gauss_tap = |t: f64| -> f64 {
        let x = 2.0 * fc * t / fs_out;
        let sinc = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
        (2.0 * fc / fs_out) * sinc * (- (t / sigma) * (t / sigma)).exp()
    };

    if !minimum_phase {
        let half = l * s as i32;
        let n = (2 * half + 2) as usize;
        let mut h = vec![0.0; n];
        for j in -half..=half {
            h[(j + half) as usize] = gauss_tap(j as f64 / s as f64);
        }
        let sum: f64 = h[..n - 1].iter().sum();
        let g = ratio * s as f64 / sum;
        for x in &mut h {
            *x *= g;
        }

        let mut filter = FirFilter {
            h,
            s,
            lo: -l,
            hi: l,
            ratio,
            integer_ratio,
            fs_out,
            src_rate,
            minimum_phase: false,
            taps: (2 * half + 1) as usize,
            poly_phases: None,
        };
        filter.poly_phases = build_fir_phases(&filter);
        filter
    } else {
        crate::phase::minimum_phase_from_tap_fn(fs_out, src_rate, l, s, &gauss_tap)
    }
}

pub fn windowed_sinc_fir(fs_out: f64, src_rate: f64, cutoff_rel: f64, taps: usize, window: &str) -> FirFilter {
    windowed_sinc_fir_ext(fs_out, src_rate, cutoff_rel, taps, window, 0.3, 100.0)
}

pub fn windowed_sinc_fir_ext(
    fs_out: f64,
    src_rate: f64,
    cutoff_rel: f64,
    taps: usize,
    window: &str,
    sigma_rel: f64,
    as_db: f64,
) -> FirFilter {
    let ratio = fs_out / src_rate;
    let integer_ratio = (ratio - ratio.round()).abs() < 1e-9;
    let s = if integer_ratio { 1 } else { 8 };

    let fc = (cutoff_rel * src_rate / 2.0).min(0.45 * fs_out);
    let l = ((taps as f64 / 2.0).round() as i32).max(8);

    let half = l * s as i32;
    let n = (2 * half + 2) as usize;
    let mut h = vec![0.0; n];

    for j in -half..=half {
        let t = j as f64 / s as f64;
        let x = 2.0 * fc * t / fs_out;
        let sinc = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
        let phase = PI * (t + l as f64) / (2.0 * l as f64);
        let w = match window {
            "blackmanHarris" => {
                let a0 = 0.35875;
                let a1 = 0.48829;
                let a2 = 0.14128;
                let a3 = 0.01168;
                a0 - a1 * (2.0 * phase).cos() + a2 * (4.0 * phase).cos() - a3 * (6.0 * phase).cos()
            }
            "hann" => {
                (phase.sin()).powi(2)
            }
            "hamming" => {
                0.54 - 0.46 * (2.0 * phase).cos()
            }
            "blackman" => {
                0.42 - 0.5 * (2.0 * phase).cos() + 0.08 * (4.0 * phase).cos()
            }
            "nuttall" => {
                let a0 = 0.355768;
                let a1 = 0.487396;
                let a2 = 0.144232;
                let a3 = 0.012604;
                a0 - a1 * (2.0 * phase).cos() + a2 * (4.0 * phase).cos() - a3 * (6.0 * phase).cos()
            }
            "blackmanNuttall" => {
                let a0 = 0.3635819;
                let a1 = 0.4891775;
                let a2 = 0.1365995;
                let a3 = 0.0106411;
                a0 - a1 * (2.0 * phase).cos() + a2 * (4.0 * phase).cos() - a3 * (6.0 * phase).cos()
            }
            "gauss" => {
                let sigma = (sigma_rel.clamp(0.1, 1.0)) * l as f64;
                (-0.5 * (t / sigma).powi(2)).exp()
            }
            "chebyshev" | "kaiser" => {
                let beta = if as_db > 50.0 {
                    0.1102 * (as_db - 8.7)
                } else if as_db > 21.0 {
                    0.5842 * (as_db - 21.0).powf(0.4) + 0.07886 * (as_db - 21.0)
                } else {
                    0.0
                };
                let u = t / l as f64;
                let arg = (0.0f64.max(1.0 - u * u)).sqrt();
                bessel_i0(beta * arg) / bessel_i0(beta)
            }
            _ => 1.0, // rectangular
        };
        h[(j + half) as usize] = (2.0 * fc / fs_out) * sinc * w;
    }

    let sum: f64 = h[..n - 1].iter().sum();
    let g = if sum.abs() > 1e-12 { ratio * s as f64 / sum } else { 1.0 };
    for x in &mut h {
        *x *= g;
    }

    let mut filter = FirFilter {
        h,
        s,
        lo: -l,
        hi: l,
        ratio,
        integer_ratio,
        fs_out,
        src_rate,
        minimum_phase: false,
        taps: (2 * half + 1) as usize,
        poly_phases: None,
    };
    filter.poly_phases = build_fir_phases(&filter);
    filter
}
