pub mod fir;
pub mod phase;
pub mod dsd;
pub mod peq;
pub mod convolution;
pub mod registry;

pub use fir::*;
pub use phase::*;
pub use dsd::*;
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
        let filter = crate::fir::build_poly_ext2_filter(dsd_rate, src_rate);
        let mut modulator = DsdModulator::new(5, "poly-ext2", DitherType::Gauss1, Some(filter));

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
}
