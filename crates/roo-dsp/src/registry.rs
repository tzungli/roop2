use std::f64::consts::PI;
use std::collections::HashMap;
use std::sync::RwLock;
use serde::{Deserialize, Serialize};
use crate::fir::{
    build_hq_poly_sinc_gauss_long_filter, build_poly_ext2_filter, build_precise_linear_filter,
    build_precise_min_filter, build_smooth_linear_filter, build_smooth_min_filter,
    windowed_sinc_fir_ext, FirFilter,
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
#[serde(rename_all = "camelCase")]
pub struct UserFilterSpec {
    pub id: String,
    #[serde(alias = "label")]
    pub name: String,
    pub sound: Option<String>,
    pub window: String, // "blackmanHarris", "kaiser", "hann", "none"
    #[serde(alias = "cutoff_rel")]
    pub cutoff_rel: f64, // e.g. 1.15
    pub taps: usize,    // e.g. 96
    #[serde(alias = "as_db")]
    pub as_db: Option<f64>, // for Kaiser
    #[serde(alias = "sigma_rel")]
    pub sigma_rel: Option<f64>, // for Gauss
    pub phase: Option<String>,
    #[serde(alias = "minimum_phase")]
    pub minimum_phase: Option<bool>,
}

impl UserFilterSpec {
    pub fn is_minimum_phase(&self) -> bool {
        self.minimum_phase.unwrap_or(false) || self.phase.as_deref() == Some("minimum")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterPreviewResult {
    pub xs: Vec<f64>,
    #[serde(rename = "ysDb", alias = "ys_db")]
    pub ys_db: Vec<f64>,
    #[serde(rename = "srcNyq", alias = "src_nyq")]
    pub src_nyq: f64,
    #[serde(rename = "fsOut", alias = "fs_out")]
    pub fs_out: f64,
    #[serde(rename = "fcHz", alias = "fc_hz")]
    pub fc_hz: f64,
    #[serde(rename = "imageZeroHz", alias = "image_zero_hz")]
    pub image_zero_hz: f64,
    #[serde(rename = "transLoHz", alias = "trans_lo_hz")]
    pub trans_lo_hz: f64,
    #[serde(rename = "stopBandHz", alias = "stop_band_hz")]
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
                id: "precise-linear".to_string(),
                label: "精準，線性相位".to_string(),
                sound: "極陡峭滾降、-140dB 深度阻帶抑制，絕對零相位失真，最高保真度 (旗艦預設推薦)".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "precise-min".to_string(),
                label: "精準，最小相位".to_string(),
                sound: "極陡峭滾降，無預響鈴 (Zero Pre-ringing)，打擊樂瞬態衝擊力極強、結像鮮明凝聚".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "gauss-long".to_string(),
                label: "高斯長階，線性相位".to_string(),
                sound: "仿 HQPlayer poly-sinc-gauss-long：20kHz 內極致平坦、奈奎斯特 -140dB 深度阻帶，純高斯包絡極低自然響鈴，音場寬鬆立體".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "poly-ext2".to_string(),
                label: "延伸磚牆，線性相位 (Ext2)".to_string(),
                sound: "仿 HQPlayer poly-sinc-ext2：1281 階超陡峭磚牆，通帶延伸至 21kHz 零衰減、奈奎斯特 -170dB 徹底抑鏡，通透高密度微細節".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "sinc-m".to_string(),
                label: "百萬階磚牆，線性相位 (Sinc-M)".to_string(),
                sound: "仿 HQPlayer sinc-M / M-Scaler：FFT 分段多相卷積百萬階極致磚牆，22kHz 零衰減、奈奎斯特 -175dB 垂直截止，極致空間解析力".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "smooth-linear".to_string(),
                label: "平滑，線性相位".to_string(),
                sound: "高頻平緩平滑滾降，極低響鈴震盪，聲音寬鬆大氣、泛音自然流暢".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
            FilterDescriptor {
                id: "smooth-min".to_string(),
                label: "平滑，最小相位".to_string(),
                sound: "無預響鈴且極低後響鈴，最符合人耳聽覺物理掩蔽效應，類比黑膠般溫潤耐聽".to_string(),
                kind: "fir".to_string(),
                dsd_safe: true,
            },
        ];
        for c in cores {
            map.insert(c.id.clone(), c);
        }
    }

    pub fn list(&self) -> Vec<FilterDescriptor> {
        const CORE_ORDER: [&str; 7] = [
            "precise-linear",
            "precise-min",
            "smooth-linear",
            "smooth-min",
            "gauss-long",
            "poly-ext2",
            "sinc-m",
        ];
        let map = self.plugins.read().unwrap();
        let mut items: Vec<FilterDescriptor> = map.values().cloned().collect();
        items.sort_by(|a, b| {
            let ia = CORE_ORDER.iter().position(|&id| id == a.id).unwrap_or(usize::MAX);
            let ib = CORE_ORDER.iter().position(|&id| id == b.id).unwrap_or(usize::MAX);
            ia.cmp(&ib).then_with(|| a.id.cmp(&b.id))
        });
        items
    }

    pub fn get(&self, id: &str) -> Option<FilterDescriptor> {
        let map = self.plugins.read().unwrap();
        map.get(id).cloned()
    }

    pub fn build_filter(&self, fs_out: f64, src_rate: f64, phase_id: &str) -> Option<FirFilter> {
        match phase_id {
            "precise-linear" => Some(build_precise_linear_filter(fs_out, src_rate)),
            "precise-min" => Some(build_precise_min_filter(fs_out, src_rate)),
            "gauss-long" | "poly-sinc-gauss-long" => Some(build_hq_poly_sinc_gauss_long_filter(fs_out, src_rate)),
            "poly-ext2" | "hqp-ps-ext2" => Some(build_poly_ext2_filter(fs_out, src_rate)),
            "sinc-m" | "sinc-M" | "hqp-sinc-m" => Some(build_poly_ext2_filter(fs_out, src_rate)),
            "smooth-linear" => Some(build_smooth_linear_filter(fs_out, src_rate)),
            "smooth-min" => Some(build_smooth_min_filter(fs_out, src_rate)),
            // 舊版過渡別名相容
            "linear" | "pure-flat" => Some(build_precise_linear_filter(fs_out, src_rate)),
            "minimum" => Some(build_precise_min_filter(fs_out, src_rate)),
            "intermediate" => Some(build_smooth_linear_filter(fs_out, src_rate)),
            "gauss-min" => Some(build_smooth_min_filter(fs_out, src_rate)),
            id if id.starts_with("uf-") => {
                let slots = self.user_slots.read().unwrap();
                slots.get(id).and_then(|spec| self.build_user_filter(fs_out, src_rate, spec))
            }
            _ => Some(build_precise_linear_filter(fs_out, src_rate)),
        }
    }

    pub fn build_user_filter(&self, fs_out: f64, src_rate: f64, spec: &UserFilterSpec) -> Option<FirFilter> {
        let taps = spec.taps.clamp(32, 512);
        let cutoff_rel = spec.cutoff_rel.clamp(0.70, 0.995);
        let as_db = spec.as_db.unwrap_or(100.0).clamp(40.0, 180.0);
        let sigma_rel = spec.sigma_rel.unwrap_or(0.25).clamp(0.12, 0.50);

        let filter = windowed_sinc_fir_ext(fs_out, src_rate, cutoff_rel, taps, &spec.window, sigma_rel, as_db);
        if spec.is_minimum_phase() {
            Some(crate::phase::linear_to_minimum_phase(&filter))
        } else {
            Some(filter)
        }
    }

    pub fn save_user_slot(&self, spec: UserFilterSpec) {
        let desc = FilterDescriptor {
            id: spec.id.clone(),
            label: spec.name.clone(),
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
        let cutoff_rel = spec.cutoff_rel.clamp(0.70, 0.995);
        let fc = (cutoff_rel * src_rate / 2.0).min(0.485 * fs_out);
        let f_max = (fs_out / 2.0).min((1.3 * src_rate).max(fc + trans_hz));
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
            ys_db.push((-160.0f64).max(20.0 * (mag.max(1e-8)).log10()));
        }

        Some(FilterPreviewResult {
            xs,
            ys_db,
            src_nyq: src_rate / 2.0,
            fs_out,
            fc_hz: fc,
            image_zero_hz: (src_rate - fc).max(fc),
            trans_lo_hz: fc + trans_hz,
            stop_band_hz: fc + trans_hz,
        })
    }
}
