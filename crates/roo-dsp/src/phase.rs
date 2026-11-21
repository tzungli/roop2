use rustfft::{FftPlanner, num_complex::Complex64};
use crate::fir::FirFilter;

pub fn minimum_phase_from_tap_fn<F>(fs_out: f64, src_rate: f64, l: i32, s: usize, tap_fn: &F) -> FirFilter
where
    F: Fn(f64) -> f64,
{
    let ratio = fs_out / src_rate;
    let integer_ratio = (ratio - ratio.round()).abs() < 1e-9;
    let budget = (2500usize).min((64usize).max((fs_out * 4.3e-4).round() as usize));
    let mut m = 4096usize;
    while (m as f64) < 2.2 * fs_out * 3.5e-3 || m < 4 * (2 * l as usize + 1) {
        m *= 2;
    }
    if m > 65536 {
        m = 65536;
    }

    let mut buf = vec![Complex64::new(0.0, 0.0); m];
    for t in -l..=l {
        let v = tap_fn(t as f64);
        if t < 0 {
            buf[(m as i32 + t) as usize].re = v;
        } else {
            buf[t as usize].re = v;
        }
    }

    let mut planner = FftPlanner::new();
    let fft_fwd = planner.plan_fft_forward(m);
    let fft_inv = planner.plan_fft_inverse(m);

    // 1. Forward FFT to frequency domain
    fft_fwd.process(&mut buf);

    // 2. Log magnitude and phase
    for k in 0..m {
        let mag = buf[k].norm().max(1e-14);
        let ph = buf[k].im.atan2(buf[k].re);
        buf[k] = Complex64::new(mag.ln(), ph);
    }

    // 3. Inverse FFT to get complex cepstrum
    fft_inv.process(&mut buf);
    for k in 0..m {
        buf[k] /= m as f64;
    }

    // 4. Fold cepstrum to enforce causal/minimum phase
    for t in 1..(m / 2) {
        buf[t] *= 2.0;
        buf[m - t] = Complex64::new(0.0, 0.0);
    }

    // 5. Forward FFT of modified cepstrum
    fft_fwd.process(&mut buf);

    // 6. Exponentiate back to spectrum
    for k in 0..m {
        let lm = buf[k].re.min(40.0);
        let ph = buf[k].im;
        buf[k] = Complex64::new(lm.exp() * ph.cos(), lm.exp() * ph.sin());
    }

    // 7. Inverse FFT to obtain minimum-phase impulse response
    fft_inv.process(&mut buf);
    for k in 0..m {
        buf[k] /= m as f64;
    }

    let mut sum = 0.0;
    for i in 0..budget {
        sum += buf[i].re;
    }
    let g = ratio / sum;

    let half = (budget - 1) * s;
    let mut h = vec![0.0; half + 2];
    for j in 0..=half {
        let t = j as f64 / s as f64;
        let t0 = t.floor() as usize;
        let fr = t - t0 as f64;
        let next_idx = (t0 + 1).min(budget - 1);
        h[j] = (buf[t0].re + fr * (buf[next_idx].re - buf[t0].re)) * g;
    }

    FirFilter {
        h,
        s,
        lo: 0,
        hi: budget as i32 - 1,
        ratio,
        integer_ratio,
        fs_out,
        src_rate,
        minimum_phase: true,
        taps: budget,
    }
}
