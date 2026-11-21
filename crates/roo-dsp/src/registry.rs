use std::f64::consts::PI;
use std::collections::HashMap;
use std::sync::RwLock;
use serde::{Deserialize, Serialize};
use crate::fir::{
    build_gauss_filter, build_hq_poly_sinc_gauss_long_filter, build_hqp_ps_ext2_filter,
    build_poly_ext2_filter, kaiser_sinc_fir, windowed_sinc_fir, FirFilter,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterDescriptor {
    pub id: String,
    pub label: String,
    pub sound: String,
    pub kind: String, // "fir" or "interp"
    #[serde(default = "default_true")]
    pub dsd_safe: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFilterSpec {
    pub id: String,
    pub label: String,
    pub sound: Option<String>,
    pub window: String, // "blackmanHarris", "kaiser", "hann", "none"
    pub cutoff_rel: f64, // e.g. 1.15
    pub taps: usize,    // e.g. 96
    pub as_db: Option<f64>, // for Kaiser
    pub minimum_phase: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterPreviewResult {
    pub xs: Vec<f64>,
    pub ys_db: Vec<f64>,
    pub src_nyq: f64,
    pub fs_out: f64,
    pub fc_hz: f64,
    pub image_zero_hz: f64,
    pub trans_lo_hz: f64,
    pub stop_band_hz: f64,
}

pub struct FilterRegistry {
    plugins: RwLock<HashMap<String, FilterDescriptor>>,
    user_slots: RwLock<HashMap<String, UserFilterSpec>>,
}

pub static REGISTRY: std::sync::LazyLock<FilterRegistry> = std::sync::LazyLock::new(FilterRegistry::new);

impl FilterRegistry {
    pub fn new() -> Self {
        let registry = Self {
            plugins: RwLock::new(HashMap::new()),
            user_slots: RwLock::new(HashMap::new()),
        };
        registry.register_core_filters();
        registry
    }

    fn register_core_filters(&self) {
        let mut map = self.plugins.write().unwrap();
        let cores = vec![
            FilterDescriptor {
                id: "linear".to_string(),
                label: "線性相位 Hermite 插值".to_string(),
                sound: "經典自然平衡，音場開闊、細節豐富".to_string(),
                kind: "interp".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "minimum".to_string(),
                label: "最小相位 Hermite 插值".to_string(),
                sound: "零預響 (Pre-ringing)，打擊樂暫態衝擊力極強、結像鮮明凝聚".to_string(),
                kind: "interp".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "intermediate".to_string(),
                label: "中間相位 (35% 線性 / 65% 最小)".to_string(),
                sound: "兼具線性開闊與最小相位的扎實低頻，音樂性極佳".to_string(),
                kind: "interp".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "poly-sinc-gauss-long".to_string(),
                label: "poly-sinc-gauss-long (511 Taps)".to_string(),
                sound: "旗艦級高斯多相 FIR：每相 511 Taps，極致瞬態還原、真實音色與宏大空間感".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "poly-ext2".to_string(),
                label: "poly-sinc-ext2 (Kaiser 140dB)".to_string(),
                sound: "超長 Kaiser 濾波器：止帶衰減高達 140dB，背景極致深黑寧靜".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "hqp-ps-ext2".to_string(),
                label: "hqp-ps-ext2 (Kaiser 180dB)".to_string(),
                sound: "HQPlayer 級別究極多相 FIR：As=180dB 極限抑噪，細膩絲滑無匹".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "pure-flat".to_string(),
                label: "純真還原 FIR (極致平坦)".to_string(),
                sound: "通帶 0~21.5kHz 絕對平坦（漣波 < 0.0001dB），零染色純淨還原，呈現錄音室母帶真實原貌".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "gauss-long".to_string(),
                label: "高斯加窗 sinc (長脈衝線性相位)".to_string(),
                sound: "高斯柔和滾降，泛音自然流暢，極具類比韻味".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "gauss-min".to_string(),
                label: "高斯最小相位 (超快暫態零預響)".to_string(),
                sound: "經希爾伯特變換的最小相位高斯濾波，瞬態迅猛果斷".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
        ];
        for c in cores {
            map.insert(c.id.clone(), c);
        }
    }

    pub fn list(&self) -> Vec<FilterDescriptor> {
        let map = self.plugins.read().unwrap();
        map.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<FilterDescriptor> {
        let map = self.plugins.read().unwrap();
        map.get(id).cloned()
    }

    pub fn build_filter(&self, fs_out: f64, src_rate: f64, phase_id: &str) -> Option<FirFilter> {
        match phase_id {
            "poly-sinc-gauss-long" => Some(build_hq_poly_sinc_gauss_long_filter(fs_out, src_rate)),
            "poly-ext2" => Some(build_poly_ext2_filter(fs_out, src_rate)),
            "hqp-ps-ext2" => Some(build_hqp_ps_ext2_filter(fs_out, src_rate)),
            "pure-flat" => Some(kaiser_sinc_fir(fs_out, src_rate, 160.0, 1.07, 0.095)),
            "gauss-long" => Some(build_gauss_filter(fs_out, src_rate, false)),
            "gauss-min" => Some(build_gauss_filter(fs_out, src_rate, true)),
            id if id.starts_with("uf-") => {
                let slots = self.user_slots.read().unwrap();
                slots.get(id).and_then(|spec| self.build_user_filter(fs_out, src_rate, spec))
            }
            _ => None, // "linear", "minimum", "intermediate" are polynomial interpolators
        }
    }

    pub fn build_user_filter(&self, fs_out: f64, src_rate: f64, spec: &UserFilterSpec) -> Option<FirFilter> {
        if spec.window == "kaiser" {
            let as_db = spec.as_db.unwrap_or(120.0).clamp(60.0, 180.0);
            let fc_factor = spec.cutoff_rel.clamp(0.9, 1.4);
            let trans_factor = 0.1;
            Some(kaiser_sinc_fir(fs_out, src_rate, as_db, fc_factor, trans_factor))
        } else {
            let taps = spec.taps.clamp(32, 256);
            let cutoff_rel = spec.cutoff_rel.clamp(0.9, 1.4);
            let filter = windowed_sinc_fir(fs_out, src_rate, cutoff_rel, taps, &spec.window);
            if spec.minimum_phase.unwrap_or(false) {
                let fc = (cutoff_rel * src_rate / 2.0).min(0.45 * fs_out);
                let l = (taps as i32 / 2).max(8);
                let s = filter.s;
                let tap_fn = |t: f64| -> f64 {
                    let x = 2.0 * fc * t / fs_out;
                    let sinc = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
                    (2.0 * fc / fs_out) * sinc
                };
                Some(crate::phase::minimum_phase_from_tap_fn(fs_out, src_rate, l, s, &tap_fn))
            } else {
                Some(filter)
            }
        }
    }

    pub fn save_user_slot(&self, spec: UserFilterSpec) {
        let desc = FilterDescriptor {
            id: spec.id.clone(),
            label: spec.label.clone(),
            sound: spec.sound.clone().unwrap_or_else(|| "使用者自訂濾波器".to_string()),
            kind: "fir".to_string(),
            dsd_safe: true,
        };
        self.plugins.write().unwrap().insert(spec.id.clone(), desc);
        self.user_slots.write().unwrap().insert(spec.id.clone(), spec);
    }

    pub fn delete_user_slot(&self, slot_id: &str) {
        self.plugins.write().unwrap().remove(slot_id);
        self.user_slots.write().unwrap().remove(slot_id);
    }

    pub fn list_user_slots(&self) -> Vec<UserFilterSpec> {
        let map = self.user_slots.read().unwrap();
        map.values().cloned().collect()
    }

    pub fn preview_filter(&self, spec: &UserFilterSpec, src_rate: f64, fs_out: f64) -> Option<FilterPreviewResult> {
        let gf = self.build_user_filter(fs_out, src_rate, spec)?;
        let n = 512;
        let h_len = gf.h.len();
        let trans_hz = if h_len > 0 { 4.0 * fs_out / h_len.max(1) as f64 } else { 0.0 };
        let fc_for_span = 1.135 * src_rate / 2.0;
        let f_max = (fs_out / 2.0).min((1.3 * src_rate).max(fc_for_span + trans_hz));
        let h = &gf.h;
        let s_g = gf.s as f64;
        let norm = gf.ratio * s_g;

        let mut xs = Vec::with_capacity(n);
        let mut ys_db = Vec::with_capacity(n);

        for i in 0..n {
            let f = f_max * i as f64 / (n - 1) as f64;
            let w = 2.0 * PI * f / (fs_out * s_g);
            let mut re = 0.0;
            let mut im = 0.0;
            for (j, &a) in h.iter().enumerate() {
                if a == 0.0 { continue; }
                let ang = w * j as f64;
                re += a * ang.cos();
                im -= a * ang.sin();
            }
            let mag = (re * re + im * im).sqrt() / norm;
            xs.push(f);
            ys_db.push((-120.0f64).max(20.0 * (mag.max(1e-7)).log10()));
        }

        Some(FilterPreviewResult {
            xs,
            ys_db,
            src_nyq: src_rate / 2.0,
            fs_out,
            fc_hz: fc_for_span,
            image_zero_hz: 0.0,
            trans_lo_hz: 0.0,
            stop_band_hz: 0.0,
        })
    }
}
