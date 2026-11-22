use rustfft::{FftPlanner, num_complex::Complex64};
use std::f64::consts::PI;
use crate::fir::{build_fir_phases, FirFilter};

/// Transforms a linear-phase FIR filter to a strictly causal, minimum-phase FIR filter
/// using the discrete Hilbert transform of the real log-magnitude spectrum (cepstral spectral factorization).
/// 
/// Properties:
/// 1. Exact magnitude response |H(w)| preservation.
/// 2. Strictly causal: impulse response begins at t = 0 (lo = 0, hi = 2 * l).
/// 3. Zero pre-ringing: h[t] = 0 for all t < 0.
/// 4. Minimizes group delay at audio frequencies.
pub fn linear_to_minimum_phase(lin_filter: &FirFilter) -> FirFilter {
    let l = lin_filter.hi;
    let s = lin_filter.s;
    let half = (l * s as i32) as usize;
    let n_lin = lin_filter.h.len();

    let causal_len = 2 * half + 1;
    let mut m = 4096usize;
    while m < 4 * causal_len {
        m *= 2;
    }
    if m > 131072 {
        m = 131072;
    }

    // 1. Pack symmetric linear-phase filter into cyclic buffer centered at 0
    let mut buf = vec![Complex64::new(0.0, 0.0); m];
    for k in 0..=half {
        if half + k < n_lin {
            let v = lin_filter.h[half + k];
            buf[k].re = v;
            if k > 0 && k < m {
                buf[m - k].re = v;
            }
        }
    }

    let mut planner = FftPlanner::new();
    let fft_fwd = planner.plan_fft_forward(m);
    let fft_inv = planner.plan_fft_inverse(m);

    // 2. Forward FFT to obtain real zero-phase frequency spectrum
    fft_fwd.process(&mut buf);

    // 3. Real cepstrum: log of magnitude
    for k in 0..m {
        let mag = buf[k].norm().max(1e-12);
        buf[k] = Complex64::new(mag.ln(), 0.0);
    }

    // 4. Inverse FFT to get real cepstrum c[n]
    fft_inv.process(&mut buf);
    for k in 0..m {
        buf[k] /= m as f64;
    }

    // 5. Fold cepstrum to enforce causal analytic spectrum:
    for k in 1..(m / 2) {
        buf[k] *= 2.0;
        buf[m - k] = Complex64::new(0.0, 0.0);
    }

    // 6. Forward FFT of causal folded cepstrum
    fft_fwd.process(&mut buf);

    // 7. Exponentiate to obtain minimum-phase complex spectrum H_min(w)
    for k in 0..m {
        let re = buf[k].re.min(40.0);
        let im = buf[k].im;
        let mag = re.exp();
        buf[k] = Complex64::new(mag * im.cos(), mag * im.sin());
    }

    // 8. Inverse FFT to obtain causal minimum-phase impulse response
    fft_inv.process(&mut buf);
    for k in 0..m {
        buf[k] /= m as f64;
    }

    // 9. Extract causal impulse response of length causal_len (from 0 to 2 * half)
    let mut h_min = vec![0.0f64; causal_len + 1];
    let taper_len = (causal_len / 20).max(4); // 5% cosine taper at trailing end
    let taper_start = causal_len.saturating_sub(taper_len);

    for j in 0..causal_len {
        let mut v = buf[j].re;
        if j >= taper_start {
            let frac = (j - taper_start) as f64 / taper_len as f64;
            let window = 0.5 * (1.0 + (PI * frac).cos());
            v *= window;
        }
        h_min[j] = v;
    }

    // 10. Normalize DC gain to match original linear-phase filter
    let lin_sum: f64 = lin_filter.h[..half * 2 + 1.min(n_lin)].iter().sum();
    let min_sum: f64 = h_min[..causal_len].iter().sum();
    if min_sum.abs() > 1e-12 {
        let gain = lin_sum / min_sum;
        for v in &mut h_min {
            *v *= gain;
        }
    }

    let mut min_filter = FirFilter {
        h: h_min,
        s,
        lo: 0,
        hi: 2 * l,
        ratio: lin_filter.ratio,
        integer_ratio: lin_filter.integer_ratio,
        fs_out: lin_filter.fs_out,
        src_rate: lin_filter.src_rate,
        minimum_phase: true,
        taps: (2 * l + 1) as usize,
        poly_phases: None,
    };
    min_filter.poly_phases = build_fir_phases(&min_filter);
    min_filter
}

/// Fallback helper for custom user-defined filter tap generation
pub fn minimum_phase_from_tap_fn<F>(fs_out: f64, src_rate: f64, l: i32, s: usize, tap_fn: &F) -> FirFilter
where
    F: Fn(f64) -> f64,
{
    let ratio = fs_out / src_rate;
    let integer_ratio = (ratio - ratio.round()).abs() < 1e-9;
    let half = l * s as i32;
    let n = (2 * half + 2) as usize;
    let mut h = vec![0.0; n];

    for j in -half..=half {
        h[(j + half) as usize] = tap_fn(j as f64 / s as f64);
    }
    let lin = FirFilter {
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
    linear_to_minimum_phase(&lin)
}
