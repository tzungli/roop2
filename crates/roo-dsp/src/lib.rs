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
}
