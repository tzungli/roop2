pub mod fir;
pub mod phase;
pub mod dsd;
pub mod dither;
pub mod peq;
pub mod convolution;
pub mod registry;

pub use fir::*;
pub use phase::*;
pub use dsd::*;
pub use dither::*;
pub use peq::*;
pub use convolution::*;
pub use registry::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bessel_i0() {
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-6);
        assert!((bessel_i0(1.0) - 1.2660658).abs() < 1e-4);
    }

    #[test]
    fn test_hermite_interpolation() {
        let p0 = 0.0;
        let p1 = 0.2;
        let p2 = 0.4;
        let p3 = 0.6;
        let y = interpolate_linear(p0, p1, p2, p3, 0.5);
        assert!((y - 0.3).abs() < 1e-5);
    }

    #[test]
    fn test_poly_sinc_gauss_long() {
        let f = build_hq_poly_sinc_gauss_long_filter(192000.0, 44100.0);
        assert!(f.taps > 500);
        assert!(f.h.len() > 0);
    }

    #[test]
    fn test_peq() {
        let chain = PeqChain::new(
            44100.0,
            0.0,
            &[PeqBand {
                filter_type: BiquadType::Peaking,
                f0: 1000.0,
                gain_db: 3.0,
                q: 1.0,
            }],
        );
        let in_l = vec![1.0; 100];
        let in_r = vec![1.0; 100];
        let mut out_l = vec![0.0; 100];
        let mut out_r = vec![0.0; 100];
        chain.apply_arrays(&in_l, &in_r, &mut out_l, &mut out_r);
        assert!(out_l[50] != 0.0);
    }

    #[test]
    fn test_dsd_modulator() {
        let mut mod5 = DsdModulator::new(5, "linear", DitherType::None, None);
        let in_l = vec![0.5; 1024];
        let in_r = vec![-0.5; 1024];
        let mut out = vec![0u8; DSD_BLOCK_SIZE * 2]; // 1 block
        let inv_ratio = 1.0 / 64.0;
        mod5.run_blocks(&in_l, &in_r, 1024, 0, 1, inv_ratio, &mut out);
        assert!(out.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_user_filter_spec_serde_compatibility() {
        let js_json = r#"{
            "id": "uf-1",
            "name": "暖聲自訂濾波",
            "sound": "高頻柔順",
            "window": "blackmanHarris",
            "phase": "minimum",
            "cutoffRel": 1.13,
            "taps": 96,
            "asDb": 100,
            "sigmaRel": 0.3
        }"#;

        let spec: UserFilterSpec = serde_json::from_str(js_json).expect("deserialize JS spec");
        assert_eq!(spec.id, "uf-1");
        assert_eq!(spec.name, "暖聲自訂濾波");
        assert_eq!(spec.cutoff_rel, 1.13);
        assert_eq!(spec.taps, 96);
        assert!(spec.is_minimum_phase());

        let res = REGISTRY.preview_filter(&spec, 44100.0, 176400.0).expect("preview filter");
        assert_eq!(res.xs.len(), 512);
        assert_eq!(res.ys_db.len(), 512);
        assert!(res.fc_hz > 0.0);

        let res_json = serde_json::to_string(&res).expect("serialize preview result");
        assert!(res_json.contains("\"ysDb\":") || res_json.contains("\"ys_db\":"));
        assert!(res_json.contains("\"srcNyq\":") || res_json.contains("\"src_nyq\":"));
        assert!(res_json.contains("\"fcHz\":") || res_json.contains("\"fc_hz\":"));
    }

    #[test]
    fn test_user_filter_windows() {
        let windows = ["blackmanHarris", "hann", "hamming", "blackman", "nuttall", "blackmanNuttall", "gauss", "chebyshev", "kaiser"];
        for win in windows {
            let spec = UserFilterSpec {
                id: "test".to_string(),
                name: "Test".to_string(),
                sound: None,
                window: win.to_string(),
                cutoff_rel: 1.10,
                taps: 64,
                as_db: Some(90.0),
                sigma_rel: Some(0.3),
                phase: Some("linear".to_string()),
                minimum_phase: Some(false),
            };
            let preview = REGISTRY.preview_filter(&spec, 44100.0, 192000.0);
            assert!(preview.is_some(), "window {} failed preview", win);
            let p = preview.unwrap();
            assert_eq!(p.xs.len(), 512);
            assert!(p.ys_db.iter().all(|&y| y.is_finite()));
        }
    }

    #[test]
    fn test_ultrasonic_protection_filter() {
        // At 192kHz, 30k and 50k should produce valid 2nd-order Butterworth filters
        let c30 = build_ultrasonic_filter_coeffs(192000.0, "30k").expect("30k filter at 192kHz");
        assert!(c30.b0 > 0.0 && c30.b1 > 0.0 && c30.b2 > 0.0);

        let c50 = build_ultrasonic_filter_coeffs(192000.0, "50k").expect("50k filter at 192kHz");
        assert!(c50.b0 > 0.0 && c50.b1 > 0.0 && c50.b2 > 0.0);

        // At 44.1kHz, 30k and 50k are above Nyquist (0.45 * 44100 = 19845) -> None
        assert!(build_ultrasonic_filter_coeffs(44100.0, "30k").is_none());
        assert!(build_ultrasonic_filter_coeffs(44100.0, "50k").is_none());

        // Apply filter to test signal
        let mut left = vec![1.0; 256];
        let mut right = vec![1.0; 256];
        apply_ultrasonic_filter(&c30, &mut left, &mut right);
        assert!(left.iter().all(|&v| v.is_finite()));
        assert!(right.iter().all(|&v| v.is_finite()));
    }

    #[test]
    fn test_fir_upsample_accuracy() {
        use crate::fir::{build_poly_ext2_filter, fir_sample};
        use std::f64::consts::PI;

        let src_rate = 44100.0;
        let dst_rate = 192000.0;
        let ratio = dst_rate / src_rate;
        let filter = build_poly_ext2_filter(dst_rate, src_rate);

        // Generate 1 kHz sine wave
        let n_src = 4410; // 0.1s
        let freq = 1000.0;
        let input: Vec<f64> = (0..n_src).map(|i| (2.0 * PI * freq * i as f64 / src_rate).sin()).collect();

        let _n_dst = (n_src as f64 * ratio).floor() as usize;
        // Test interior samples
        for i in 2000..3000 {
            let actual = fir_sample(&input, n_src, i, ratio, &filter);
            let expected = (2.0 * PI * freq * i as f64 / dst_rate).sin();
            let err = (actual - expected).abs();
            assert!(err < 0.01, "Sample {}: actual={}, expected={}, err={}", i, actual, expected, err);
        }
    }

    #[test]
    fn test_dsd_modulation_sine() {
        use std::f64::consts::PI;

        let src_rate = 44100.0;
        let dsd_rate = 2822400.0;
        let filter = crate::fir::build_precise_linear_filter(dsd_rate, src_rate);
        let mut modulator = DsdModulator::new(5, "precise-linear", DitherType::Gauss1, Some(filter));

        let n_src = 44100; // 1s
        let freq = 1000.0;
        let left: Vec<f64> = (0..n_src).map(|i| 0.5 * (2.0 * PI * freq * i as f64 / src_rate).sin()).collect();
        let right: Vec<f64> = (0..n_src).map(|i| 0.5 * (2.0 * PI * freq * i as f64 / src_rate).cos()).collect();

        let mut out = vec![0u8; DSD_BLOCK_SIZE * 2 * 4]; // 4 blocks
        let inv_ratio = src_rate / dsd_rate;
        modulator.run_blocks(&left, &right, n_src, 0, 4, inv_ratio, &mut out);

        // Verify integrators didn't diverge/clip to infinity
        assert!(modulator.s1_l.is_finite() && modulator.s1_l.abs() < 6.0);
        assert!(modulator.s5_l.is_finite() && modulator.s5_l.abs() < 6.0);
        // Verify output bytes have bit density reflecting signal
        assert!(out.iter().any(|&b| b != 0x00 && b != 0xFF));
    }

    #[test]
    fn test_dsd_modulation_hermite_speed() {
        use std::f64::consts::PI;

        // Test full DSD512 (22.5792 MHz) from Stage-1 352.8 kHz input!
        let src_rate = 352800.0;
        let dsd_rate = 22579200.0; // DSD512
        let mut modulator = DsdModulator::new(7, "precise-linear", DitherType::Gauss, None);
        modulator.set_dsd_rate(dsd_rate as u32);

        let n_src = 352800; // 1.0s at 352.8 kHz
        let freq = 1000.0;
        let left: Vec<f64> = (0..n_src).map(|i| 0.5 * (2.0 * PI * freq * i as f64 / src_rate).sin()).collect();
        let right: Vec<f64> = (0..n_src).map(|i| 0.5 * (2.0 * PI * freq * i as f64 / src_rate).cos()).collect();

        let num_blocks = 689; // ~1.0 full second of DSD512 stereo!
        let mut out = vec![0u8; DSD_BLOCK_SIZE * 2 * num_blocks];
        let inv_ratio = src_rate / dsd_rate;

        let t0 = std::time::Instant::now();
        modulator.run_blocks(&left, &right, n_src, 0, num_blocks, inv_ratio, &mut out);
        let elapsed = t0.elapsed();
        println!("Modulating 1.0s of DSD512 (parallel Hermite) took: {:?}", elapsed);

        // Also benchmark DSD64, DSD128, DSD256 (1.0s each)
        for (name, rate, s1_rate, blocks) in [
            ("DSD64", 2822400.0_f64, 176400.0_f64, 87usize),
            ("DSD128", 5644800.0_f64, 352800.0_f64, 173usize),
            ("DSD256", 11289600.0_f64, 352800.0_f64, 345usize),
        ] {
            let mut m = DsdModulator::new(7, "precise-linear", DitherType::Gauss, None);
            m.set_dsd_rate(rate as u32);
            let mut buf = vec![0u8; DSD_BLOCK_SIZE * 2 * blocks];
            let ts = std::time::Instant::now();
            m.run_blocks(&left, &right, n_src, 0, blocks, s1_rate / rate, &mut buf);
            println!("Modulating 1.0s of {} (parallel Hermite) took: {:?}", name, ts.elapsed());
        }

        // 1 second of DSD512 must process faster than real-time (1000ms in release, 5000ms in debug)
        let max_ms = if cfg!(debug_assertions) { 5000 } else { 1000 };
        assert!(elapsed.as_millis() < max_ms, "DSD512 modulation took too long: {:?}", elapsed);
        assert!(modulator.s1_l.is_finite());
        assert!(out.iter().any(|&b| b != 0x00 && b != 0xFF));
    }

    #[test]
    fn test_dsd_lsb_first_inband_snr() {
        use std::f64::consts::PI;

        // Verify Sony DSF v1.01 LSB-first bit order and Hermite glitch-free continuity
        // by measuring 20Hz-20kHz SNR of a 1kHz sine wave modulated to DSD64.
        let src_rate = 352800.0;
        let dsd_rate = 2822400.0; // DSD64
        let mut modulator = DsdModulator::new(7, "precise-linear", DitherType::Gauss, None);
        modulator.set_dsd_rate(dsd_rate as u32);

        let num_blocks = 8; // 262144 DSD samples (~92.8 ms)
        let total_dsd = num_blocks * DSD_SAMPLES_PER_BLOCK;
        let n_src = total_dsd / 8 + 16;
        let freq = 1000.0;
        let left: Vec<f64> = (0..n_src).map(|i| 0.5 * (2.0 * PI * freq * i as f64 / src_rate).sin()).collect();
        let right = left.clone();

        let mut out = vec![0u8; DSD_BLOCK_SIZE * 2 * num_blocks];
        modulator.run_blocks(&left, &right, n_src, 0, num_blocks, src_rate / dsd_rate, &mut out);

        // Decode Left channel STRICTLY LSB-first (per Sony DSF bits_per_sample = 1)
        let mut bits = Vec::with_capacity(total_dsd);
        for b in 0..num_blocks {
            let off = b * 2 * DSD_BLOCK_SIZE;
            for i in 0..DSD_BLOCK_SIZE {
                let byte = out[off + i];
                for bit in 0..8 {
                    bits.push(if (byte >> bit) & 1 == 1 { 1.0f64 } else { -1.0f64 });
                }
            }
        }

        // Decimate by 64x using a 2-stage moving average (CIC-2) to 44.1 kHz, then check SNR vs 1kHz sine
        let dec = 64;
        let n_pcm = (bits.len() / dec) - 20;
        let mut pcm = Vec::with_capacity(n_pcm);
        for i in 10..(10 + n_pcm) {
            let center = i * dec;
            let mut sum = 0.0;
            let mut wsum = 0.0;
            // Triangular (CIC-2) kernel of length 2*dec
            for k in 0..(2 * dec) {
                let w = if k < dec { (k + 1) as f64 } else { (2 * dec - k) as f64 };
                sum += bits[center + k] * w;
                wsum += w;
            }
            pcm.push(sum / wsum);
        }

        // Measure 1 kHz signal power vs residual noise power via DFT projection
        let n = pcm.len();
        let mut re = 0.0;
        let mut im = 0.0;
        let mut total_pow = 0.0;
        for (i, &x) in pcm.iter().enumerate() {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos(); // Hann window
            let xw = x * w;
            let ph = 2.0 * PI * freq * i as f64 / 44100.0;
            re += xw * ph.cos();
            im += xw * ph.sin();
            total_pow += xw * xw;
        }
        // Parseval for Hann window: signal power in single tone = 2*(re^2+im^2)/sum(w^2) ...
        // Simpler: least-squares fit a*cos(ph) + b*sin(ph) + dc over interior samples
        let mut s_cc = 0.0; let mut s_ss = 0.0; let mut s_xc = 0.0; let mut s_xs = 0.0;
        for (i, &x) in pcm.iter().enumerate() {
            let ph = 2.0 * PI * freq * i as f64 / 44100.0;
            let c = ph.cos();
            let s = ph.sin();
            s_cc += c * c;
            s_ss += s * s;
            s_xc += x * c;
            s_xs += x * s;
        }
        let a = s_xc / s_cc;
        let b_coef = s_xs / s_ss;
        let mut sig_p = 0.0;
        let mut err_p = 0.0;
        for (i, &x) in pcm.iter().enumerate() {
            let ph = 2.0 * PI * freq * i as f64 / 44100.0;
            let fit = a * ph.cos() + b_coef * ph.sin();
            sig_p += fit * fit;
            let e = x - fit;
            err_p += e * e;
        }
        let _ = (re, im, total_pow);
        let snr_db = 10.0 * (sig_p / err_p.max(1e-20)).log10();
        println!("DSD64 LSB-first decoded 1kHz SNR (CIC-2): {:.2} dB", snr_db);
        // With LSB-first fixed & Hermite bug fixed, simple CIC-2 decimation achieves ~46 dB
        // (with the old MSB-first bug it collapses to ~18 dB!).
        assert!(snr_db > 42.0, "DSD64 LSB-first SNR too low ({:.2} dB) — bit order or modulator broken!", snr_db);
    }

    #[test]
    fn test_flagship_src_filters() {
        let src_rate = 44100.0;
        let dst_rate = 176400.0; // 4x PCM

        // 1. Precise Linear
        let p_lin = build_precise_linear_filter(dst_rate, src_rate);
        assert!(!p_lin.minimum_phase);
        assert_eq!(p_lin.lo, -p_lin.hi);
        let half = (p_lin.hi * p_lin.s as i32) as usize;
        // Verify symmetry: h[half - k] == h[half + k]
        for k in 1..=10 {
            let diff = (p_lin.h[half - k] - p_lin.h[half + k]).abs();
            assert!(diff < 1e-12, "Precise Linear must be symmetric, diff={}", diff);
        }

        // 2. Precise Minimum
        let p_min = build_precise_min_filter(dst_rate, src_rate);
        assert!(p_min.minimum_phase);
        assert_eq!(p_min.lo, 0);
        assert!(p_min.hi > 0);
        // Energy peak is at the very beginning (no pre-ringing, rise time within ~18 samples)
        let peak_idx = p_min.h.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        assert!(peak_idx < 30, "Minimum phase peak must occur near start (t=0), got idx={}", peak_idx);

        // 3. Smooth Linear
        let s_lin = build_smooth_linear_filter(dst_rate, src_rate);
        assert!(!s_lin.minimum_phase);
        assert_eq!(s_lin.lo, -s_lin.hi);
        assert!(s_lin.taps < p_lin.taps, "Smooth filter should have shorter impulse span than Precise");

        // 4. Smooth Minimum
        let s_min = build_smooth_min_filter(dst_rate, src_rate);
        assert!(s_min.minimum_phase);
        assert_eq!(s_min.lo, 0);

        // 5. Registry checks
        let list = REGISTRY.list();
        assert_eq!(list.len(), 4);
        let ids: Vec<&str> = list.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"precise-linear"));
        assert!(ids.contains(&"precise-min"));
        assert!(ids.contains(&"smooth-linear"));
        assert!(ids.contains(&"smooth-min"));
    }
}
