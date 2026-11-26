pub mod fir;
pub mod phase;
pub mod dsd;
pub mod dither;
pub mod peq;
pub mod convolution;
pub mod sinc_m;
pub mod registry;

pub use fir::*;
pub use phase::*;
pub use dsd::*;
pub use dither::*;
pub use peq::*;
pub use convolution::*;
pub use sinc_m::*;
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
            // Verify cutoff_rel > 1.0 is safely clamped below source Nyquist
            assert!(p.fc_hz < 44100.0 * 0.5, "fc_hz must be < src Nyquist, got {}", p.fc_hz);
        }

        // Verify Kaiser respects taps and minimum_phase
        let kaiser_min = UserFilterSpec {
            id: "k-min".to_string(),
            name: "Kaiser Min".to_string(),
            sound: None,
            window: "kaiser".to_string(),
            cutoff_rel: 0.95,
            taps: 128,
            as_db: Some(120.0),
            sigma_rel: None,
            phase: Some("minimum".to_string()),
            minimum_phase: Some(true),
        };
        let kf = REGISTRY.build_user_filter(176400.0, 44100.0, &kaiser_min).expect("kaiser min");
        assert!(kf.minimum_phase);
        assert_eq!(kf.lo, 0);
        // 128 input taps at 4x => l = 64 * 4 = 256 => 513 output taps
        assert_eq!(kf.taps, 513);

        // Verify Dolph-Chebyshev window is distinct from Kaiser and symmetric
        let cheb = dolph_chebyshev_window(129, 100.0);
        assert_eq!(cheb.len(), 129);
        assert!((cheb[64] - 1.0).abs() < 1e-12);
        assert!((cheb[0] - cheb[128]).abs() < 1e-12);
        assert!((cheb[10] - cheb[118]).abs() < 1e-12);
        let cheb_f = windowed_sinc_fir_ext(176400.0, 44100.0, 0.95, 128, "chebyshev", 0.25, 100.0);
        let kais_f = windowed_sinc_fir_ext(176400.0, 44100.0, 0.95, 128, "kaiser", 0.25, 100.0);
        let diff: f64 = cheb_f.h.iter().zip(kais_f.h.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 1e-3, "Chebyshev and Kaiser must produce distinct filters, diff={}", diff);
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
        assert_eq!(list.len(), 8);
        let ids: Vec<&str> = list.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["precise-linear", "precise-min", "smooth-linear", "smooth-min", "gauss-long", "poly-ext2", "sinc-m", "sinc-l"]
        );
    }

    #[test]
    fn test_sinc_m_fft_accuracy_and_speed() {
        let src_rate = 44100.0;
        let fs_out = 176400.0;
        let t0 = std::time::Instant::now();
        let resampler = SincMFftResampler::new(fs_out, src_rate).expect("sinc-m 4x");
        let build_ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(resampler.is_single_stage_integer());
        assert_eq!(resampler.total_taps(), 262_137);

        // Verify 16x (705.6k) reaches ~1 Million taps
        let t16 = std::time::Instant::now();
        let r16 = SincMFftResampler::new(705600.0, src_rate).expect("sinc-m 16x");
        let build16_ms = t16.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(r16.total_taps(), 1_048_545);

        // Stereo test: L = 1 kHz, R = 21.95 kHz (only 100 Hz below Nyquist!)
        let in_len = 140_000usize; // > 3 seconds so center window has zero edge effects
        let mut left = vec![0.0f64; in_len];
        let mut right = vec![0.0f64; in_len];
        for i in 0..in_len {
            let t = i as f64 / src_rate;
            left[i] = (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            right[i] = (2.0 * std::f64::consts::PI * 21950.0 * t).sin();
        }

        // Upsample center 1.0s (176,400 output samples) and time it
        let start_out = 40_000 * 4;
        let end_out = start_out + 176_400;
        let t_run = std::time::Instant::now();
        let (out_l, out_r) = resampler.upsample_range_stereo(&left, &right, start_out, end_out);
        let run_ms = t_run.elapsed().as_secs_f64() * 1000.0;

        // 1. Check 1 kHz L-channel reconstruction accuracy (SNR)
        let mut sig_pow = 0.0;
        let mut err_pow = 0.0;
        for (idx, &y) in out_l.iter().enumerate() {
            let t = (start_out + idx) as f64 / fs_out;
            let expected = (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            sig_pow += expected * expected;
            let e = y - expected;
            err_pow += e * e;
        }
        let snr_1k_db = 10.0 * (sig_pow / err_pow.max(1e-30)).log10();

        // 2. Check 21.95 kHz R-channel passband gain & 22.15 kHz mirror image suppression
        // Goertzel/DFT projection at 21,950 Hz (passband) and 22,150 Hz (1st mirror image!)
        let measure_tone_db = |sig: &[f64], freq_hz: f64| -> f64 {
            let n = sig.len();
            let mut re = 0.0;
            let mut im = 0.0;
            let mut w_sum = 0.0;
            for (k, &v) in sig.iter().enumerate() {
                // Hann window to eliminate spectral leakage
                let win = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * k as f64 / n as f64).cos());
                let ang = 2.0 * std::f64::consts::PI * freq_hz * (start_out + k) as f64 / fs_out;
                re += v * win * ang.cos();
                im -= v * win * ang.sin();
                w_sum += win;
            }
            let amp = 2.0 * (re * re + im * im).sqrt() / w_sum;
            20.0 * amp.max(1e-12).log10()
        };

        let db_21_95k = measure_tone_db(&out_r, 21950.0);
        let db_image_22_15k = measure_tone_db(&out_r, 22150.0);

        let t_run16 = std::time::Instant::now();
        let (_out16_l, _out16_r) = r16.upsample_range_stereo(&left, &right, 40_000 * 16, 40_000 * 16 + 705_600);
        let run16_ms = t_run16.elapsed().as_secs_f64() * 1000.0;

        println!(
            "sinc-M (multi-core): build4x={:.1}ms, build16x(1.05M taps)={:.1}ms, run4x(1s stereo)={:.1}ms, run16x(1s stereo)={:.1}ms, 1kHz SNR={:.2}dB, 21.95kHz={:.4}dB, 22.15kHz image={:.2}dB",
            build_ms, build16_ms, run_ms, run16_ms, snr_1k_db, db_21_95k, db_image_22_15k
        );

        assert!(snr_1k_db > 150.0, "1kHz SNR should exceed 150 dB, got {:.2}", snr_1k_db);
        assert!(db_21_95k.abs() < 0.01, "21.95kHz passband should be flat (<0.01dB), got {:.4}", db_21_95k);
        assert!(db_image_22_15k < -150.0, "22.15kHz mirror image should be < -150dB, got {:.2}", db_image_22_15k);

        // 3. Verify chunked streaming matches single-shot across block boundaries
        let chunk_start = 65530 * 4;
        let chunk_end = 65545 * 4;
        let (full_l, full_r) = resampler.upsample_range_stereo(&left, &right, chunk_start, chunk_end);
        let (c1_l, c1_r) = resampler.upsample_range_stereo(&left, &right, chunk_start, chunk_start + 31);
        let (c2_l, c2_r) = resampler.upsample_range_stereo(&left, &right, chunk_start + 31, chunk_end);
        for i in 0..full_l.len() {
            let gl = if i < 31 { c1_l[i] } else { c2_l[i - 31] };
            let gr = if i < 31 { c1_r[i] } else { c2_r[i - 31] };
            assert!((full_l[i] - gl).abs() < 1e-13, "L chunk mismatch at {}", i);
            assert!((full_r[i] - gr).abs() < 1e-13, "R chunk mismatch at {}", i);
        }

        // 4. Verify non-integer 2-stage upsampling (48k -> 176.4k)
        let r_nonint = SincMFftResampler::new(176400.0, 48000.0).expect("sinc-m non-int");
        assert!(!r_nonint.is_single_stage_integer());
        let mut s48 = vec![0.0f64; 96_000];
        for i in 0..s48.len() {
            s48[i] = (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48000.0).sin();
        }
        let (ni_l, _) = r_nonint.upsample_range_stereo(&s48, &s48, 88200, 92200);
        let mut max_err = 0.0f64;
        for (idx, &y) in ni_l.iter().enumerate() {
            let t = (88200 + idx) as f64 / 176400.0;
            let exp = (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            max_err = max_err.max((y - exp).abs());
        }
        assert!(max_err < 1e-4, "Non-integer sinc-M max error too high: {}", max_err);
    }

    #[test]
    fn test_asdm7ec_super512_benchmark() {
        use rustfft::{num_complex::Complex, FftPlanner};

        let src_rate = 352_800.0;
        let in_len = 352_800usize + 64;
        let fft_n = 262_144usize;
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_n);

        let mut win = vec![0.0f64; fft_n];
        let mut win_sum = 0.0;
        for i in 0..fft_n {
            let p = 2.0 * std::f64::consts::PI * i as f64 / fft_n as f64;
            let w = 0.27105140069342
                - 0.43329793923448 * p.cos()
                + 0.21812299954311 * (2.0 * p).cos()
                - 0.06592544638803 * (3.0 * p).cos()
                + 0.01081174209837 * (4.0 * p).cos()
                - 0.00077658482522 * (5.0 * p).cos()
                + 0.00001388721735 * (6.0 * p).cos();
            win[i] = w;
            win_sum += w;
        }

        let analyze = |bits_l: &[u8], dsd_rate: f64, tone_hz: f64| -> (f64, f64) {
            let skip_bytes = ((dsd_rate * 0.05 / 8.0) as usize).min(bits_l.len().saturating_sub(fft_n / 8 + 1));
            let mut buf = vec![Complex::new(0.0, 0.0); fft_n];
            for k in 0..fft_n {
                let b_idx = skip_bytes + (k >> 3);
                let bit = (bits_l[b_idx] >> (k & 7)) & 1;
                buf[k].re = (if bit == 1 { 1.0 } else { -1.0 }) * win[k];
            }
            fft.process(&mut buf);

            let bin_hz = dsd_rate / fft_n as f64;
            let f1_bin = (tone_hz / bin_hz).round() as usize;
            let bin_20k = (20_000.0 / bin_hz).floor() as usize;
            let bin_50k = (50_000.0 / bin_hz).floor() as usize;
            let scale2 = 4.0 / (win_sum * win_sum);

            let mut sig_pow = 0.0;
            let mut noise_pow = 0.0;
            let mut ultra_pow = 0.0;
            for k in 8..=bin_50k.min(fft_n / 2 - 1) {
                let p = (buf[k].re * buf[k].re + buf[k].im * buf[k].im) * scale2;
                if k >= f1_bin.saturating_sub(8) && k <= f1_bin + 8 {
                    sig_pow += p;
                } else if k <= bin_20k {
                    noise_pow += p;
                } else {
                    ultra_pow += p;
                }
            }
            let snr_db = 10.0 * (sig_pow / noise_pow.max(1e-30)).log10();
            let ultra_db = 10.0 * ultra_pow.max(1e-30).log10();
            (snr_db, ultra_db)
        };

        for &(mode_name, dsd_rate, stage2_ratio) in &[
            ("DSD256", 11_289_600u32, 32usize),
            ("DSD512", 22_579_200u32, 64usize),
        ] {
            let bin_hz = dsd_rate as f64 / fft_n as f64;
            let tone_hz = (1000.0 / bin_hz).round() * bin_hz;
            let pcm: Vec<f64> = (0..in_len)
                .map(|i| 0.5 * (2.0 * std::f64::consts::PI * tone_hz * i as f64 / src_rate).sin())
                .collect();
            let num_blocks = (dsd_rate as usize + DSD_SAMPLES_PER_BLOCK - 1) / DSD_SAMPLES_PER_BLOCK;
            let inv_ratio = 1.0 / stage2_ratio as f64;

            for &(name, ord) in &[("ASDM7EC", 70u8), ("ASDM7EC-super512+fs", 75u8)] {
                let mut m = DsdModulator::new(ord, "precise-linear", DitherType::Gauss, None);
                m.set_dsd_rate(dsd_rate);
                let mut out = vec![0u8; num_blocks * 2 * DSD_BLOCK_SIZE];
                let t0 = std::time::Instant::now();
                m.run_blocks(&pcm, &pcm, in_len, 0, num_blocks, inv_ratio, &mut out);
                let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

                let mut l_bytes = Vec::with_capacity(num_blocks * DSD_BLOCK_SIZE);
                for b in 0..num_blocks {
                    let off = b * 2 * DSD_BLOCK_SIZE;
                    l_bytes.extend_from_slice(&out[off..off + DSD_BLOCK_SIZE]);
                }
                let (snr_db, ultra_db) = analyze(&l_bytes, dsd_rate as f64, tone_hz);
                let max_state = [m.s1_l, m.s2_l, m.s3_l, m.s4_l, m.s5_l, m.s6_l, m.s7_l]
                    .iter()
                    .map(|v| v.abs())
                    .fold(0.0f64, f64::max);
                println!(
                    "{} {}: 1s={:.1}ms, 0-20k SNR={:.2}dB, 20-50k ultra={:.2}dBFS, max_state={:.3}",
                    mode_name, name, elapsed_ms, snr_db, ultra_db, max_state
                );
                assert!(snr_db > 150.0, "{} {} SNR too low: {:.2}", mode_name, name, snr_db);
                assert!(max_state.is_finite() && max_state < 5.5, "{} {} unstable state: {}", mode_name, name, max_state);
            }
        }

        // -3 dBFS PCM (+0.1 dB SACD) and 0 dBFS PCM (0.99 full-scale) stability stress tests at DSD512
        for &(label, amp_stress) in &[("-3dBFS", 0.707f64), ("0dBFS", 0.99f64)] {
            let pcm_stress: Vec<f64> = (0..in_len)
                .map(|i| amp_stress * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / src_rate).sin())
                .collect();
            let mut m_fs = DsdModulator::new(75, "precise-linear", DitherType::Gauss, None);
            m_fs.set_dsd_rate(22_579_200);
            let mut out_fs = vec![0u8; 200 * 2 * DSD_BLOCK_SIZE];
            m_fs.run_blocks(&pcm_stress, &pcm_stress, in_len, 0, 200, 1.0 / 64.0, &mut out_fs);
            let max_fs = [m_fs.s1_l, m_fs.s2_l, m_fs.s3_l, m_fs.s4_l, m_fs.s5_l, m_fs.s6_l, m_fs.s7_l]
                .iter()
                .map(|v| v.abs())
                .fold(0.0f64, f64::max);
            println!("DSD512 ASDM7EC-super512+fs {} stress max_state={:.3}", label, max_fs);
            assert!(max_fs.is_finite() && max_fs < 4.5, "{} stress unstable: {}", label, max_fs);
        }
    }

    #[test]
    fn test_sinc_l_fft_accuracy_and_speed() {
        let src_rate = 44100.0;
        let fs_out = 176400.0;
        let t0 = std::time::Instant::now();
        let resampler = SincMFftResampler::new_with_preset(fs_out, src_rate, SincLongPreset::SincL).expect("sinc-l 4x");
        let build_ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(resampler.is_single_stage_integer());
        assert_eq!(resampler.total_taps(), 524_281);

        // Verify 16x (705.6k) reaches ~2.1 Million taps (131,070 * 16 + 1)
        let r16 = SincMFftResampler::new_with_preset(705600.0, src_rate, SincLongPreset::SincL).expect("sinc-l 16x");
        assert_eq!(r16.total_taps(), 2_097_121);

        // Stereo test: L = 1 kHz, R = 22.00 kHz (only 50 Hz below Nyquist!)
        let in_len = 220_000usize;
        let mut left = vec![0.0f64; in_len];
        let mut right = vec![0.0f64; in_len];
        for i in 0..in_len {
            let t = i as f64 / src_rate;
            left[i] = (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            right[i] = (2.0 * std::f64::consts::PI * 22000.0 * t).sin();
        }

        let start_out = 75_000 * 4;
        let end_out = start_out + 176_400;
        let t_run = std::time::Instant::now();
        let (out_l, out_r) = resampler.upsample_range_stereo(&left, &right, start_out, end_out);
        let run_ms = t_run.elapsed().as_secs_f64() * 1000.0;

        let mut sig_pow = 0.0;
        let mut err_pow = 0.0;
        for (idx, &y) in out_l.iter().enumerate() {
            let t = (start_out + idx) as f64 / fs_out;
            let expected = (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            sig_pow += expected * expected;
            let e = y - expected;
            err_pow += e * e;
        }
        let snr_1k_db = 10.0 * (sig_pow / err_pow.max(1e-30)).log10();

        let measure_tone_db = |sig: &[f64], freq_hz: f64| -> f64 {
            let n = sig.len();
            let mut re = 0.0;
            let mut im = 0.0;
            let mut w_sum = 0.0;
            for (k, &v) in sig.iter().enumerate() {
                let win = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * k as f64 / n as f64).cos());
                let ang = 2.0 * std::f64::consts::PI * freq_hz * (start_out + k) as f64 / fs_out;
                re += v * win * ang.cos();
                im -= v * win * ang.sin();
                w_sum += win;
            }
            let amp = 2.0 * (re * re + im * im).sqrt() / w_sum;
            20.0 * amp.max(1e-12).log10()
        };

        let db_22_00k = measure_tone_db(&out_r, 22000.0);
        let db_image_22_10k = measure_tone_db(&out_r, 22100.0);

        println!(
            "sinc-L (multi-core): build4x={:.1}ms, run4x(1s stereo)={:.1}ms, 1kHz SNR={:.2}dB, 22.00kHz={:.4}dB, 22.10kHz image={:.2}dB",
            build_ms, run_ms, snr_1k_db, db_22_00k, db_image_22_10k
        );

        assert!(snr_1k_db > 150.0, "sinc-L 1kHz SNR should exceed 150 dB, got {:.2}", snr_1k_db);
        assert!(db_22_00k.abs() < 0.01, "sinc-L 22.00kHz passband should be flat (<0.01dB), got {:.4}", db_22_00k);
        assert!(db_image_22_10k < -150.0, "sinc-L 22.10kHz mirror image should be < -150dB, got {:.2}", db_image_22_10k);

        // Verify chunked streaming across 131,072-sample block boundary
        let chunk_start = 131066 * 4;
        let chunk_end = 131080 * 4;
        let (full_l, full_r) = resampler.upsample_range_stereo(&left, &right, chunk_start, chunk_end);
        let (c1_l, c1_r) = resampler.upsample_range_stereo(&left, &right, chunk_start, chunk_start + 29);
        let (c2_l, c2_r) = resampler.upsample_range_stereo(&left, &right, chunk_start + 29, chunk_end);
        for i in 0..full_l.len() {
            let gl = if i < 29 { c1_l[i] } else { c2_l[i - 29] };
            let gr = if i < 29 { c1_r[i] } else { c2_r[i - 29] };
            assert!((full_l[i] - gl).abs() < 1e-13, "sinc-L L chunk mismatch at {}", i);
            assert!((full_r[i] - gr).abs() < 1e-13, "sinc-L R chunk mismatch at {}", i);
        }
    }

    #[test]
    fn test_asdm_ec_benchmarks() {
        use rustfft::{num_complex::Complex, FftPlanner};

        // Measure 1 kHz -6 dBFS sine across DSD64, DSD128, DSD256 for orders 5, 7, 50 (ASDM5EC), 70 (ASDM7EC)
        let src_rate = 352_800.0; // Stage-1 upsampled rate (same as production DSD pipeline)
        let amp = 0.5; // -6 dBFS
        let freq = 1000.0;
        let in_len = 352_800usize + 64; // 1.0s
        let mut pcm = vec![0.0f64; in_len];
        for i in 0..in_len {
            pcm[i] = amp * (2.0 * std::f64::consts::PI * freq * i as f64 / src_rate).sin();
        }

        let fft_n = 262_144usize; // 2^18 FFT window on decoded bitstream
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_n);

        // Periodic 7-term Blackman-Harris window (< -180 dB sidelobes for true 180+ dB SNR measurement)
        let mut win = vec![0.0f64; fft_n];
        let mut win_sum = 0.0;
        for i in 0..fft_n {
            let p = 2.0 * std::f64::consts::PI * i as f64 / fft_n as f64;
            let w = 0.27105140069342
                - 0.43329793923448 * p.cos()
                + 0.21812299954311 * (2.0 * p).cos()
                - 0.06592544638803 * (3.0 * p).cos()
                + 0.01081174209837 * (4.0 * p).cos()
                - 0.00077658482522 * (5.0 * p).cos()
                + 0.00001388721735 * (6.0 * p).cos();
            win[i] = w;
            win_sum += w;
        }

        let analyze_bits = |bits_l: &[u8], dsd_rate: f64, tone_hz: f64| -> (f64, f64, f64) {
            // Skip ~50 ms of startup transient so 7th-order resonators fully settle at DSD64/128/256
            let skip_bytes = ((dsd_rate * 0.05 / 8.0) as usize).min(bits_l.len().saturating_sub(fft_n / 8 + 1));
            let mut buf = vec![Complex::new(0.0, 0.0); fft_n];
            for k in 0..fft_n {
                let b_idx = skip_bytes + (k >> 3);
                let bit = (bits_l[b_idx] >> (k & 7)) & 1;
                let v = if bit == 1 { 1.0 } else { -1.0 };
                buf[k].re = v * win[k];
            }
            fft.process(&mut buf);

            let bin_hz = dsd_rate / fft_n as f64;
            let f1_bin = (tone_hz / bin_hz).round() as usize;
            let bin_20k = (20_000.0 / bin_hz).floor() as usize;
            let bin_50k = (50_000.0 / bin_hz).floor() as usize;

            let mut sig_pow = 0.0;
            let mut noise_pow = 0.0;
            let mut ultra_pow = 0.0;
            let mut harm_pow = 0.0;
            let scale2 = 4.0 / (win_sum * win_sum);

            for k in 8..=bin_50k.min(fft_n / 2 - 1) {
                let p = (buf[k].re * buf[k].re + buf[k].im * buf[k].im) * scale2;
                if k >= f1_bin.saturating_sub(8) && k <= f1_bin + 8 {
                    sig_pow += p;
                } else if k <= bin_20k {
                    noise_pow += p;
                    // Check 2nd..5th harmonics
                    for h in 2..=5 {
                        let hb = f1_bin * h;
                        if k >= hb.saturating_sub(4) && k <= hb + 4 {
                            harm_pow += p;
                        }
                    }
                } else {
                    ultra_pow += p;
                }
            }
            let snr_db = 10.0 * (sig_pow / noise_pow.max(1e-30)).log10();
            let thd_db = 10.0 * (harm_pow / sig_pow.max(1e-30)).log10();
            let ultra_db = 10.0 * ultra_pow.max(1e-30).log10();
            (snr_db, thd_db, ultra_db)
        };

        let modes: [(&str, u32, usize); 4] = [
            ("DSD64", 2_822_400, 8),
            ("DSD128", 5_644_800, 16),
            ("DSD256", 11_289_600, 32),
            ("DSD512", 22_579_200, 64),
        ];
        let orders: [(&str, u8); 5] = [
            ("5th-CRFB", 5),
            ("ASDM5EC", 50),
            ("7th-CLANS", 7),
            ("ASDM7EC", 70),
            ("ASDM7EC-super512+fs", 75),
        ];

        for &(mode_name, dsd_rate, stage2_ratio) in &modes {
            // Coherent bin-aligned ~1 kHz tone for exact zero-leakage FFT measurement
            let bin_hz = dsd_rate as f64 / fft_n as f64;
            let tone_hz = (freq / bin_hz).round() * bin_hz;
            for i in 0..in_len {
                pcm[i] = amp * (2.0 * std::f64::consts::PI * tone_hz * i as f64 / src_rate).sin();
            }

            let num_blocks = (dsd_rate as usize + DSD_SAMPLES_PER_BLOCK - 1) / DSD_SAMPLES_PER_BLOCK;
            let inv_ratio = 1.0 / stage2_ratio as f64;
            let mut prev_bits_5: Vec<u8> = Vec::new();
            let mut prev_bits_7: Vec<u8> = Vec::new();

            for &(ord_name, ord) in &orders {
                let mut m = DsdModulator::new(ord, "precise-linear", DitherType::Gauss, None);
                m.set_dsd_rate(dsd_rate);
                let mut out = vec![0u8; num_blocks * 2 * DSD_BLOCK_SIZE];
                let t0 = std::time::Instant::now();
                m.run_blocks(&pcm, &pcm, in_len, 0, num_blocks, inv_ratio, &mut out);
                let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;

                // Extract Left channel contiguous bytes
                let mut l_bytes = Vec::with_capacity(num_blocks * DSD_BLOCK_SIZE);
                for b in 0..num_blocks {
                    let off = b * 2 * DSD_BLOCK_SIZE;
                    l_bytes.extend_from_slice(&out[off..off + DSD_BLOCK_SIZE]);
                }

                let (snr_db, thd_db, ultra_db) = analyze_bits(&l_bytes, dsd_rate as f64, tone_hz);
                let max_state = [m.s1_l, m.s2_l, m.s3_l, m.s4_l, m.s5_l, m.s6_l, m.s7_l]
                    .iter()
                    .map(|v| v.abs())
                    .fold(0.0f64, f64::max);

                let hamming_note = if ord == 5 {
                    prev_bits_5 = l_bytes.clone();
                    String::new()
                } else if ord == 50 {
                    let diff_bits: usize = l_bytes.iter().zip(prev_bits_5.iter()).map(|(a, b)| (a ^ b).count_ones() as usize).sum();
                    format!(", diff_vs_5th={:.1}%", 100.0 * diff_bits as f64 / (l_bytes.len() * 8) as f64)
                } else if ord == 7 {
                    prev_bits_7 = l_bytes.clone();
                    String::new()
                } else {
                    let diff_bits: usize = l_bytes.iter().zip(prev_bits_7.iter()).map(|(a, b)| (a ^ b).count_ones() as usize).sum();
                    format!(", diff_vs_7th={:.1}%", 100.0 * diff_bits as f64 / (l_bytes.len() * 8) as f64)
                };

                println!(
                    "[{} {:9}] SNR={:6.2} dB | THD={:7.2} dB | Ultra(20-50k)={:7.2} dB | MaxState={:.3} | 1s={:6.1} ms{}",
                    mode_name, ord_name, snr_db, thd_db, ultra_db, max_state, elapsed_ms, hamming_note
                );

                assert!(max_state < 6.0 && max_state.is_finite(), "{} {} unstable: {}", mode_name, ord_name, max_state);
                assert!(snr_db > 75.0, "{} {} SNR too low: {:.2}", mode_name, ord_name, snr_db);
            }
        }
    }

    #[test]
    fn test_poly_ext2_frequency_response() {
        let src_rate = 44100.0;
        let fs_out = 176400.0;
        let f = REGISTRY.build_filter(fs_out, src_rate, "poly-ext2").expect("poly-ext2 filter");
        assert!(!f.minimum_phase);
        assert_eq!(f.lo, -f.hi);
        assert!(f.poly_phases.is_some());

        let n = f.taps;
        for i in 0..(n / 2) {
            assert!(
                (f.h[i] - f.h[n - 1 - i]).abs() < 1e-12,
                "Non-symmetric tap at {}: {} vs {}",
                i,
                f.h[i],
                f.h[n - 1 - i]
            );
        }

        let eval_db = |freq_hz: f64| -> f64 {
            let s_g = f.s as f64;
            let norm = f.ratio * s_g;
            let w = 2.0 * std::f64::consts::PI * freq_hz / (fs_out * s_g);
            let mut re = 0.0;
            let mut im = 0.0;
            for (j, &a) in f.h[..n].iter().enumerate() {
                let ang = w * j as f64;
                re += a * ang.cos();
                im -= a * ang.sin();
            }
            let mag = (re * re + im * im).sqrt() / norm;
            20.0 * mag.max(1e-12).log10()
        };

        let db_1k = eval_db(1000.0);
        let db_20k = eval_db(20000.0);
        let db_20_8k = eval_db(20800.0);
        let db_21_5k = eval_db(21500.0);
        let db_nyq = eval_db(22050.0);
        let db_23k = eval_db(23000.0);
        let db_30k = eval_db(30000.0);
        println!(
            "poly-ext2 (44.1k->176.4k, taps={}): 1kHz={:.4}dB, 20kHz={:.4}dB, 20.8kHz={:.4}dB, 21.5kHz={:.2}dB, 22.05kHz={:.2}dB, 23kHz={:.2}dB, 30kHz={:.2}dB",
            n, db_1k, db_20k, db_20_8k, db_21_5k, db_nyq, db_23k, db_30k
        );

        assert!(db_1k.abs() < 0.005, "1kHz gain should be ~0dB, got {}", db_1k);
        assert!(db_20k.abs() < 0.005, "20kHz passband should be dead flat, got {}", db_20k);
        assert!(db_20_8k.abs() < 0.01, "20.8kHz extended passband should be flat, got {}", db_20_8k);
        assert!(db_nyq < -160.0, "Nyquist (22.05kHz) attenuation should be < -160dB, got {}", db_nyq);
        assert!(db_23k < -165.0, "23kHz stopband should be < -165dB, got {}", db_23k);
        assert!(db_30k < -165.0, "30kHz stopband should be < -165dB, got {}", db_30k);
    }

    #[test]
    fn test_gauss_long_frequency_response() {
        let src_rate = 44100.0;
        let fs_out = 176400.0;
        let f = REGISTRY.build_filter(fs_out, src_rate, "gauss-long").expect("gauss-long filter");
        assert!(!f.minimum_phase);
        assert_eq!(f.lo, -f.hi);
        assert!(f.poly_phases.is_some());

        // Symmetry check (linear phase)
        let n = f.taps;
        for i in 0..(n / 2) {
            assert!(
                (f.h[i] - f.h[n - 1 - i]).abs() < 1e-12,
                "Non-symmetric tap at {}: {} vs {}",
                i,
                f.h[i],
                f.h[n - 1 - i]
            );
        }

        let eval_db = |freq_hz: f64| -> f64 {
            let s_g = f.s as f64;
            let norm = f.ratio * s_g;
            let w = 2.0 * std::f64::consts::PI * freq_hz / (fs_out * s_g);
            let mut re = 0.0;
            let mut im = 0.0;
            for (j, &a) in f.h[..n].iter().enumerate() {
                let ang = w * j as f64;
                re += a * ang.cos();
                im -= a * ang.sin();
            }
            let mag = (re * re + im * im).sqrt() / norm;
            20.0 * mag.max(1e-12).log10()
        };

        let db_1k = eval_db(1000.0);
        let db_20k = eval_db(20000.0);
        let db_nyq = eval_db(22050.0);
        let db_23k = eval_db(23000.0);
        let db_30k = eval_db(30000.0);
        println!(
            "gauss-long (44.1k->176.4k, taps={}): 1kHz={:.4}dB, 20kHz={:.4}dB, 22.05kHz={:.2}dB, 23kHz={:.2}dB, 30kHz={:.2}dB",
            n, db_1k, db_20k, db_nyq, db_23k, db_30k
        );

        assert!(db_1k.abs() < 0.01, "1kHz gain should be ~0dB, got {}", db_1k);
        assert!(db_20k.abs() < 0.1, "20kHz passband should be flat (<0.1dB), got {}", db_20k);
        assert!(db_nyq < -135.0, "Nyquist (22.05kHz) attenuation should be < -135dB, got {}", db_nyq);
        assert!(db_23k < -140.0, "23kHz stopband should be < -140dB, got {}", db_23k);
        assert!(db_30k < -140.0, "30kHz stopband should be < -140dB, got {}", db_30k);
    }

    #[test]
    fn test_convolution_conjugate_symmetry_accuracy() {
        // Multi-partition IR (> CONV_BLOCK_SIZE = 2048): impulse at tap 5 (0.75) and tap 2500 (-0.25)
        let mut ir = vec![0.0f64; 3000];
        ir[5] = 0.75;
        ir[2500] = -0.25;
        let mut eng = ConvolutionEngine::new(48000.0, &ir, &ir).expect("conv engine");

        // Feed unit impulse at sample 0, followed by zeros across 3 blocks (6144 samples)
        let n = 6144;
        let mut in_l = vec![0.0f64; n];
        in_l[0] = 1.0;
        let in_r = in_l.clone();
        let mut out_l = vec![0.0f64; n];
        let mut out_r = vec![0.0f64; n];
        eng.apply_arrays(&in_l, &in_r, &mut out_l, &mut out_r);

        // ConvolutionEngine has 1-block (CONV_BLOCK_SIZE - 1 = 2047) latency before first block drains
        // Find peaks in output matching 0.75 and -0.25 separated by 2495 samples
        let (p1_idx, &p1_val) = out_l
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        assert!((p1_val - 0.75).abs() < 1e-10, "Primary IR tap mismatch: {}", p1_val);
        let p2_val = out_l[p1_idx + 2495];
        assert!((p2_val - (-0.25)).abs() < 1e-10, "Second-partition IR tap mismatch: {}", p2_val);
        assert!((out_l[p1_idx + 10]).abs() < 1e-10, "Unexpected ringing: {}", out_l[p1_idx + 10]);
    }

    #[test]
    fn test_dither_gauss_cached_and_highrate_shaping() {
        let mut d44 = StereoPcmDitherer::with_sample_rate(44100.0);
        let mut d192 = StereoPcmDitherer::with_sample_rate(192000.0);
        for i in 0..4096 {
            let s = 0.5 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 44100.0).sin();
            let (gl, gr) = d44.quantize_24bit(s, -s, DitherType::Gauss);
            assert!(gl.abs() <= 8388607 && gr.abs() <= 8388607);
            let (nl, nr) = d192.quantize_24bit(s, -s, DitherType::NoiseShaped);
            assert!(nl.abs() <= 8388607 && nr.abs() <= 8388607);
        }
        assert!(d192.left.err_history[0].is_finite() && d192.left.err_history[0].abs() < 10.0);
    }

    #[test]
    fn test_fir_avx2_fma_and_isp_soft_knee() {
        let f = build_poly_ext2_filter(192000.0, 44100.0);
        let bank = f.poly_phases.as_ref().expect("polyphase bank");
        let entry = &bank.entries[0];
        let x: Vec<f64> = (0..entry.t).map(|i| ((i as f64) * 0.13).sin()).collect();

        let scalar: f64 = x.iter().zip(entry.g.iter()).map(|(a, b)| a * b).sum();
        let simd = polyphase_dot_product(&x, &entry.g);
        assert!(
            (scalar - simd).abs() < 1e-12,
            "AVX2+FMA polyphase mismatch: scalar={}, simd={}",
            scalar,
            simd
        );

        // Verify ISP soft-knee is bit-exact below 0.985 and smoothly saturates overs above 1.0
        assert_eq!(PcmChannelDither::soft_clip_isp(0.95), 0.95);
        assert_eq!(PcmChannelDither::soft_clip_isp(-0.985), -0.985);
        let clipped = PcmChannelDither::soft_clip_isp(1.12);
        assert!(clipped > 0.985 && clipped < 1.0, "ISP over should stay < 1.0, got {}", clipped);
    }
}
