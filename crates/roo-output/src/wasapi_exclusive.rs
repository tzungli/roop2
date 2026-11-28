use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use windows::Win32::Media::Audio::{
    IMMDeviceEnumerator, MMDeviceEnumerator, IAudioClient, IAudioRenderClient,
    eRender, eConsole, AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{CoInitializeEx, CoCreateInstance, CLSCTX_ALL, COINIT_MULTITHREADED};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, SetThreadPriority, WaitForSingleObject,
    THREAD_PRIORITY_TIME_CRITICAL,
};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HANDLE;
#[cfg(target_os = "windows")]
use windows::core::HRESULT;
#[cfg(target_os = "windows")]
use crate::probe::KSDATAFORMAT_SUBTYPE_PCM;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExclusiveStartOptions {
    pub url: String,
    pub rate: u32,
    #[serde(default, rename = "startByte")]
    pub start_byte: u64,
    #[serde(default, rename = "deviceId")]
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExclusiveState {
    pub busy: bool,
    pub rate: u32,
    pub paused: bool,
}

pub struct ExclusiveController {
    running: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    seek_byte: Arc<AtomicU64>,
    current_rate: Arc<Mutex<u32>>,
    stop_signal: Arc<AtomicBool>,
}

impl Default for ExclusiveController {
    fn default() -> Self {
        Self::new()
    }
}

impl ExclusiveController {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            seek_byte: Arc::new(AtomicU64::new(0)),
            current_rate: Arc::new(Mutex::new(0)),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_busy(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn state(&self) -> ExclusiveState {
        ExclusiveState {
            busy: self.running.load(Ordering::SeqCst),
            rate: *self.current_rate.lock().unwrap(),
            paused: self.paused.load(Ordering::SeqCst),
        }
    }

    pub fn pause(&self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            self.paused.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub fn resume(&self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            self.paused.store(false, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub fn seek(&self, stream_seconds: f64) -> bool {
        let rate = *self.current_rate.lock().unwrap();
        if rate == 0 || !self.running.load(Ordering::SeqCst) {
            return false;
        }
        // 24-bit stereo block align = 6 bytes, 44 bytes WAV header
        let byte_pos = 44 + (stream_seconds.max(0.0) * rate as f64 * 6.0).round() as u64;
        self.seek_byte.store(byte_pos, Ordering::SeqCst);
        true
    }

    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        *self.current_rate.lock().unwrap() = 0;
    }

    pub fn start<F>(&self, opts: ExclusiveStartOptions, emit_event: F) -> Result<(), String>
    where
        F: Fn(serde_json::Value) + Send + Sync + 'static,
    {
        self.stop();

        self.running.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        self.stop_signal.store(false, Ordering::SeqCst);
        *self.current_rate.lock().unwrap() = opts.rate;

        let running = self.running.clone();
        let paused = self.paused.clone();
        let seek_byte = self.seek_byte.clone();
        let stop_signal = self.stop_signal.clone();

        thread::spawn(move || {
            let res = run_wasapi_playback_loop(&opts, &running, &paused, &seek_byte, &stop_signal, &emit_event);
            running.store(false, Ordering::SeqCst);
            if let Err(e) = res {
                emit_event(serde_json::json!({
                    "ev": "fail",
                    "code": "RP-WASAPI-ERR",
                    "msg": e
                }));
            }
        });

        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn run_wasapi_playback_loop<F>(
    opts: &ExclusiveStartOptions,
    _running: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
    seek_byte: &Arc<AtomicU64>,
    stop_signal: &Arc<AtomicBool>,
    emit_event: &F,
) -> Result<(), String>
where
    F: Fn(serde_json::Value),
{
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| format!("CoCreateInstance MMDeviceEnumerator failed: {}", e))?;

        let device = if !opts.device_id.is_empty() {
            let h_id: windows::core::HSTRING = opts.device_id.as_str().into();
            enumerator.GetDevice(&h_id).unwrap_or_else(|_| {
                enumerator.GetDefaultAudioEndpoint(eRender, eConsole).unwrap()
            })
        } else {
            enumerator.GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| format!("GetDefaultAudioEndpoint failed: {}", e))?
        };

        let mut client: IAudioClient = device.Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Activate IAudioClient failed: {}", e))?;

        let rate = opts.rate;
        let bits = 24u16;
        let block_align = 2 * (32 / 8); // 24-bit in 32-bit container

        let mut wf = WAVEFORMATEXTENSIBLE::default();
        wf.Format.wFormatTag = 0xFFFE;
        wf.Format.nChannels = 2;
        wf.Format.nSamplesPerSec = rate;
        wf.Format.wBitsPerSample = 32;
        wf.Format.nBlockAlign = block_align;
        wf.Format.nAvgBytesPerSec = rate * block_align as u32;
        wf.Format.cbSize = 22;
        wf.Samples.wValidBitsPerSample = bits;
        wf.dwChannelMask = 0x3;
        wf.SubFormat = KSDATAFORMAT_SUBTYPE_PCM;

        let p_fmt = &wf as *const _ as *const WAVEFORMATEX;
        let mut buffer_duration_hns: i64 = 500000; // 50 ms

        let init_res = client.Initialize(
            AUDCLNT_SHAREMODE_EXCLUSIVE,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            buffer_duration_hns,
            buffer_duration_hns,
            p_fmt,
            None,
        );

        if let Err(e) = init_res {
            if e.code() == HRESULT(0x88890019u32 as i32) {
                let actual_frames = client.GetBufferSize().unwrap_or(2048);
                buffer_duration_hns = ((actual_frames as i64 * 10_000_000) + (rate as i64 / 2)) / rate as i64;
                let client2: IAudioClient = device.Activate(CLSCTX_ALL, None)
                    .map_err(|e| format!("Re-activate failed: {}", e))?;
                client2.Initialize(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    buffer_duration_hns,
                    buffer_duration_hns,
                    p_fmt,
                    None,
                ).map_err(|e| format!("WASAPI aligned init failed: {}", e))?;
                client = client2;
            } else {
                return Err(format!("Initialize exclusive mode failed: {}", e));
            }
        }

        let audio_event: HANDLE = CreateEventW(None, false, false, None)
            .map_err(|e| format!("CreateEventW failed: {}", e))?;

        client.SetEventHandle(audio_event)
            .map_err(|e| format!("SetEventHandle failed: {}", e))?;

        let buffer_frame_count = client.GetBufferSize()
            .map_err(|e| format!("GetBufferSize failed: {}", e))?;

        let render_client: IAudioRenderClient = client.GetService()
            .map_err(|e| format!("GetService IAudioRenderClient failed: {}", e))?;

        emit_event(serde_json::json!({
            "ev": "ready",
            "rate": rate,
            "bufferMs": buffer_duration_hns / 10000
        }));

        // HTTP streaming client
        let http_client = reqwest::blocking::Client::new();
        let mut stream_resp = http_client.get(&opts.url)
            .header("Range", format!("bytes={}-", opts.start_byte))
            .send()
            .map_err(|e| format!("Failed to connect to audio stream: {}", e))?;

        client.Start().map_err(|e| format!("client.Start failed: {}", e))?;

        let mut total_bytes_streamed = opts.start_byte;
        let mut last_progress_report = std::time::Instant::now();
        let bytes_per_frame = 6usize; // 24-bit stereo in stream
        let mut raw_buf = vec![0u8; buffer_frame_count as usize * bytes_per_frame + bytes_per_frame];
        let mut leftover_len = 0usize;

        while !stop_signal.load(Ordering::SeqCst) {
            // Check if seek requested
            let req_seek = seek_byte.swap(0, Ordering::SeqCst);
            if req_seek > 0 {
                let aligned_seek = 44 + ((req_seek.saturating_sub(44)) / bytes_per_frame as u64) * bytes_per_frame as u64;
                if let Ok(new_resp) = http_client.get(&opts.url)
                    .header("Range", format!("bytes={}-", aligned_seek))
                    .send()
                {
                    stream_resp = new_resp;
                    total_bytes_streamed = aligned_seek;
                    leftover_len = 0;
                }
            }

            if paused.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(50));
                continue;
            }

            let wait_res = WaitForSingleObject(audio_event, 2000);
            if wait_res.0 != 0 {
                // Timeout or error
                continue;
            }

            let dest_buf_ptr = match render_client.GetBuffer(buffer_frame_count) {
                Ok(ptr) => ptr,
                Err(_) => continue,
            };

            let bytes_needed = buffer_frame_count as usize * bytes_per_frame;
            if raw_buf.len() < bytes_needed + bytes_per_frame {
                raw_buf.resize(bytes_needed + bytes_per_frame, 0);
            }

            // Fill full WASAPI buffer without leaving partial (< 6 byte) frame splits across TCP reads
            let mut total_read = leftover_len;
            while total_read < bytes_needed {
                match std::io::Read::read(&mut stream_resp, &mut raw_buf[total_read..bytes_needed]) {
                    Ok(0) => break, // EOF
                    Ok(n) => total_read += n,
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }

            let valid_bytes = (total_read / bytes_per_frame) * bytes_per_frame;
            leftover_len = total_read - valid_bytes;

            if valid_bytes == 0 {
                let _ = render_client.ReleaseBuffer(buffer_frame_count, 0x2); // AUDCLNT_BUFFERFLAGS_SILENT
                emit_event(serde_json::json!({ "ev": "eof" }));
                break;
            }

            let frames_read = (valid_bytes / bytes_per_frame) as u32;

            // Pack 24-bit into 32-bit container (Left 3 bytes -> 4 bytes, Right 3 bytes -> 4 bytes)
            let dest_slice = std::slice::from_raw_parts_mut(dest_buf_ptr, buffer_frame_count as usize * 8);
            for f in 0..frames_read as usize {
                let src_base = f * 6;
                let dst_base = f * 8;

                // Left: 24-bit aligned in 32-bit (byte 0 zeroed or lower 8 bits, 1,2,3 data)
                dest_slice[dst_base] = 0;
                dest_slice[dst_base + 1] = raw_buf[src_base];
                dest_slice[dst_base + 2] = raw_buf[src_base + 1];
                dest_slice[dst_base + 3] = raw_buf[src_base + 2];

                // Right
                dest_slice[dst_base + 4] = 0;
                dest_slice[dst_base + 5] = raw_buf[src_base + 3];
                dest_slice[dst_base + 6] = raw_buf[src_base + 4];
                dest_slice[dst_base + 7] = raw_buf[src_base + 5];
            }

            // Preserve any partial trailing bytes (< 6) for the next WASAPI period
            if leftover_len > 0 {
                raw_buf.copy_within(valid_bytes..total_read, 0);
            }

            // Zero fill remainder
            if frames_read < buffer_frame_count {
                for i in (frames_read as usize * 8)..(buffer_frame_count as usize * 8) {
                    dest_slice[i] = 0;
                }
            }

            let _ = render_client.ReleaseBuffer(buffer_frame_count, 0);
            total_bytes_streamed += valid_bytes as u64;

            if last_progress_report.elapsed() >= Duration::from_millis(250) {
                let sec = total_bytes_streamed.saturating_sub(44) as f64 / (rate as f64 * bytes_per_frame as f64);
                emit_event(serde_json::json!({
                    "ev": "pos",
                    "sec": sec,
                    "bytes": total_bytes_streamed,
                    "underruns": 0
                }));
                last_progress_report = std::time::Instant::now();
            }
        }

        let _ = client.Stop();
        let _ = client.Reset();
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn run_wasapi_playback_loop<F>(
    _opts: &ExclusiveStartOptions,
    _running: &Arc<AtomicBool>,
    _paused: &Arc<AtomicBool>,
    _seek_byte: &Arc<AtomicU64>,
    _stop_signal: &Arc<AtomicBool>,
    _emit_event: &F,
) -> Result<(), String>
where
    F: Fn(serde_json::Value),
{
    Err("WASAPI exclusive mode is only available on Windows".to_string())
}
