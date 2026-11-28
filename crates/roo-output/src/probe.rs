#[cfg(target_os = "windows")]
use windows::Win32::Media::Audio::{
    IMMDeviceEnumerator, MMDeviceEnumerator, IAudioClient,
    eRender, eConsole, AUDCLNT_SHAREMODE_EXCLUSIVE,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{CoInitializeEx, CoCreateInstance, CLSCTX_ALL, COINIT_MULTITHREADED};
#[cfg(target_os = "windows")]
use windows::core::{HRESULT, GUID};
use serde::{Deserialize, Serialize};

#[cfg(target_os = "windows")]
pub const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
#[cfg(target_os = "windows")]
pub const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedFormat {
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub ext: bool,
    pub sub: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub ev: String,
    pub ok: bool,
    #[serde(rename = "deviceId")]
    pub device_id: String,
    #[serde(rename = "mixRate")]
    pub mix_rate: u32,
    #[serde(rename = "mixChannels")]
    pub mix_channels: u16,
    #[serde(rename = "mixBits")]
    pub mix_bits: u16,
    pub supported: Vec<SupportedFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

#[cfg(target_os = "windows")]
pub fn probe_exclusive_device(device_id: Option<&str>) -> ProbeResult {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
            Ok(en) => en,
            Err(e) => {
                return ProbeResult {
                    ev: "probe".to_string(),
                    ok: false,
                    device_id: "".to_string(),
                    mix_rate: 0,
                    mix_channels: 0,
                    mix_bits: 0,
                    supported: Vec::new(),
                    msg: Some(format!("Failed to create device enumerator: {}", e)),
                };
            }
        };

        let device = match device_id {
            Some(id) if !id.is_empty() => {
                let h_id: windows::core::HSTRING = id.into();
                match enumerator.GetDevice(&h_id) {
                    Ok(d) => d,
                    Err(_) => match enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                        Ok(d) => d,
                        Err(e) => return err_probe(&e.to_string()),
                    },
                }
            }
            _ => match enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                Ok(d) => d,
                Err(e) => return err_probe(&e.to_string()),
            },
        };

        let dev_id_pwstr = match device.GetId() {
            Ok(p) => p.to_string().unwrap_or_default(),
            Err(_) => "".to_string(),
        };

        let client: IAudioClient = match device.Activate(CLSCTX_ALL, None) {
            Ok(c) => c,
            Err(e) => return err_probe(&format!("Activate failed: {}", e)),
        };

        let mix_fmt_ptr = match client.GetMixFormat() {
            Ok(p) => p,
            Err(e) => return err_probe(&format!("GetMixFormat failed: {}", e)),
        };

        let mix_fmt = &*mix_fmt_ptr;
        let mix_rate = mix_fmt.nSamplesPerSec;
        let mix_channels = mix_fmt.nChannels;
        let mix_bits = mix_fmt.wBitsPerSample;

        let candidate_rates = [768000, 705600, 384000, 352800, 192000, 176400, 96000, 88200, 48000, 44100];
        let mut supported = Vec::new();

        for &rate in &candidate_rates {
            // Check 24-bit in 32-bit container or 32-bit float or 16-bit
            for &(bits, sub, is_ext) in &[(24u16, "pcm", true), (32u16, "float", true), (16u16, "pcm", false)] {
                let block_align = 2 * (if bits == 24 { 32 } else { bits } / 8);
                let mut wf_ext = WAVEFORMATEXTENSIBLE::default();
                wf_ext.Format.wFormatTag = if is_ext { 0xFFFE } else { 1 };
                wf_ext.Format.nChannels = 2;
                wf_ext.Format.nSamplesPerSec = rate;
                wf_ext.Format.wBitsPerSample = if bits == 24 { 32 } else { bits };
                wf_ext.Format.nBlockAlign = block_align;
                wf_ext.Format.nAvgBytesPerSec = rate * block_align as u32;
                wf_ext.Format.cbSize = if is_ext { 22 } else { 0 };

                if is_ext {
                    wf_ext.Samples.wValidBitsPerSample = bits;
                    wf_ext.dwChannelMask = 0x3;
                    wf_ext.SubFormat = if sub == "float" {
                        KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
                    } else {
                        KSDATAFORMAT_SUBTYPE_PCM
                    };
                }

                let p_fmt = &wf_ext as *const _ as *const WAVEFORMATEX;
                let hr = client.IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, p_fmt, None);
                if hr == HRESULT(0) {
                    supported.push(SupportedFormat {
                        rate,
                        channels: 2,
                        bits,
                        ext: is_ext,
                        sub: sub.to_string(),
                    });
                    break;
                }
            }
        }

        ProbeResult {
            ev: "probe".to_string(),
            ok: true,
            device_id: dev_id_pwstr,
            mix_rate,
            mix_channels,
            mix_bits,
            supported,
            msg: None,
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn probe_exclusive_device(_device_id: Option<&str>) -> ProbeResult {
    err_probe("WASAPI exclusive output is only supported on Windows (use System Output, DLNA, or AirPlay on macOS/Linux)")
}

fn err_probe(msg: &str) -> ProbeResult {
    ProbeResult {
        ev: "probe".to_string(),
        ok: false,
        device_id: "".to_string(),
        mix_rate: 0,
        mix_channels: 0,
        mix_bits: 0,
        supported: Vec::new(),
        msg: Some(msg.to_string()),
    }
}
