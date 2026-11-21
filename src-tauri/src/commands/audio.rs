use serde::{Deserialize, Serialize};
use std::path::Path;
use lofty::file::AudioFile;
use roo_decoder::read_wav_pcm;
use roo_dsp::ir_response_peak;

#[derive(Debug, Serialize, Deserialize)]
pub struct IrInfo {
    pub ok: bool,
    pub path: String,
    pub taps: usize,
    #[serde(rename = "sampleRate")]
    pub sample_rate: u32,
    pub channels: usize,
    #[serde(rename = "peakGainLin")]
    pub peak_gain_lin: f64,
    #[serde(rename = "peakGainDb")]
    pub peak_gain_db: f64,
}

#[tauri::command]
pub async fn audio_get_dsd_stream_url(
    file_path: String,
    start_sec: Option<f64>,
    peq_json: Option<String>,
    conv_json: Option<String>,
) -> Result<String, String> {
    let start = start_sec.unwrap_or(0.0);
    let peq = peq_json.unwrap_or_default();
    let conv = conv_json.unwrap_or_default();
    let url = format!(
        "http://127.0.0.1:39840/dsd-stream?file_path={}&start_sec={}&peq_json={}&conv_json={}",
        urlencoding::encode(&file_path),
        start,
        urlencoding::encode(&peq),
        urlencoding::encode(&conv)
    );
    Ok(url)
}

#[tauri::command]
pub async fn audio_get_sox_stream_url(
    file_path: String,
    start_sec: Option<f64>,
    sample_rate: Option<u32>,
    dsd_mode: Option<String>,
    dsd_order: Option<u8>,
    filter_phase: Option<String>,
    peq_json: Option<String>,
    conv_json: Option<String>,
) -> Result<String, String> {
    let start = start_sec.unwrap_or(0.0);
    let rate = sample_rate.unwrap_or(192000);
    let dsd_m = dsd_mode.unwrap_or_default();
    let dsd_o = dsd_order.unwrap_or(5);
    let phase = filter_phase.unwrap_or_else(|| "linear".to_string());
    let peq = peq_json.unwrap_or_default();
    let conv = conv_json.unwrap_or_default();

    let url = format!(
        "http://127.0.0.1:39840/sox-stream?file_path={}&start_sec={}&target_rate={}&dsd_mode={}&dsd_order={}&filter_phase={}&peq_json={}&conv_json={}",
        urlencoding::encode(&file_path),
        start,
        rate,
        urlencoding::encode(&dsd_m),
        dsd_o,
        urlencoding::encode(&phase),
        urlencoding::encode(&peq),
        urlencoding::encode(&conv)
    );
    Ok(url)
}

#[tauri::command]
pub async fn audio_get_duration(file_path: String) -> Result<f64, String> {
    let p = Path::new(&file_path);
    if let Ok(tagged) = lofty::probe::Probe::open(p).and_then(|pr| pr.read().map_err(Into::into)) {
        return Ok(tagged.properties().duration().as_secs_f64());
    }
    Ok(0.0)
}

#[tauri::command]
pub async fn conv_get_ir_info(ir_path: String) -> Result<IrInfo, String> {
    let (sr, left, _right) = read_wav_pcm(&ir_path, 0.0, 30.0).map_err(|e| e.to_string())?;
    let taps = left.len();
    let peak_lin = ir_response_peak(&left);
    let peak_db = if peak_lin > 1e-7 { 20.0 * peak_lin.log10() } else { -120.0 };

    Ok(IrInfo {
        ok: true,
        path: ir_path,
        taps,
        sample_rate: sr,
        channels: 2,
        peak_gain_lin: peak_lin,
        peak_gain_db: peak_db,
    })
}

#[tauri::command]
pub async fn conv_pick_ir_file() -> Result<Option<String>, String> {
    // Return empty / dialog placeholder
    Ok(None)
}

#[tauri::command]
pub async fn audio_get_spectrum(_token: Option<String>, _time_sec: Option<f64>) -> Result<Vec<f64>, String> {
    Ok(vec![0.0; 64])
}
