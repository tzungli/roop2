use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::watch;
use crate::controller::UpnpController;
use crate::ssdp::DlnaDevice;
use anyhow::{bail, Result};

pub struct DlnaManager {
    controller: Arc<UpnpController>,
    devices: Arc<RwLock<Vec<DlnaDevice>>>,
    selected_device_id: Arc<RwLock<Option<String>>>,
    stop_tx: Arc<RwLock<Option<watch::Sender<bool>>>>,
}

impl Default for DlnaManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DlnaManager {
    pub fn new() -> Self {
        Self {
            controller: Arc::new(UpnpController::new()),
            devices: Arc::new(RwLock::new(Vec::new())),
            selected_device_id: Arc::new(RwLock::new(None)),
            stop_tx: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set_devices(&self, list: Vec<DlnaDevice>) {
        *self.devices.write().unwrap() = list;
    }

    pub fn get_devices(&self) -> Vec<DlnaDevice> {
        self.devices.read().unwrap().clone()
    }

    pub fn select_device(&self, device_id: &str) -> bool {
        let devs = self.devices.read().unwrap();
        if devs.iter().any(|d| d.id == device_id) {
            *self.selected_device_id.write().unwrap() = Some(device_id.to_string());
            true
        } else {
            false
        }
    }

    pub fn get_selected_device(&self) -> Option<DlnaDevice> {
        let sel = self.selected_device_id.read().unwrap().clone()?;
        let devs = self.devices.read().unwrap();
        devs.iter().find(|d| d.id == sel).cloned()
    }

    pub fn get_device(&self, device_id: &str) -> Option<DlnaDevice> {
        let devs = self.devices.read().unwrap();
        devs.iter().find(|d| d.id == device_id).cloned()
    }

    pub async fn play(
        &self,
        device_id: &str,
        stream_url: &str,
        title: &str,
        artist: &str,
        album: &str,
        cover_url: Option<&str>,
        start_sec: f64,
        on_progress: impl Fn(f64, f64, &str) + Send + Sync + 'static,
        on_ended: impl Fn() + Send + Sync + 'static,
    ) -> Result<()> {
        let dev = match self.get_device(device_id) {
            Some(d) => d,
            None => bail!("Device not found: {}", device_id),
        };

        // Stop any previous progress loop
        if let Some(tx) = self.stop_tx.write().unwrap().take() {
            let _ = tx.send(true);
        }

        let is_raw_dsf = (stream_url.contains("dsd-stream") || stream_url.contains("dsd_mode=dsd")) && stream_url.contains("dop=false");

        // 1. Ensure transport is stopped first to avoid error 705 (Transport locked)
        let _ = self.controller.stop(&dev.av_transport_url, &dev.av_transport_service_type).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 2. Set AVTransport URI with proper metadata (including title, artist, album, cover art)
        let mut set_res = self.controller.set_av_transport_uri(
            &dev.av_transport_url,
            &dev.av_transport_service_type,
            stream_url,
            title,
            artist,
            album,
            cover_url,
            is_raw_dsf,
        ).await;

        if let Err(ref e) = set_res {
            if is_raw_dsf {
                eprintln!("[DLNA] Device rejected raw DSF SetAVTransportURI: {}. Retrying with DoP WAV fallback...", e);
                let dop_url = stream_url.replace("dop=false", "dop=true");
                set_res = self.controller.set_av_transport_uri(
                    &dev.av_transport_url,
                    &dev.av_transport_service_type,
                    &dop_url,
                    title,
                    artist,
                    album,
                    cover_url,
                    false,
                ).await;
            }
            set_res?;
        }

        tokio::time::sleep(Duration::from_millis(80)).await;

        // 3. Trigger Play (with brief retry if transport is still transitioning)
        let mut play_res = self.controller.play(&dev.av_transport_url, &dev.av_transport_service_type).await;
        if play_res.is_err() {
            tokio::time::sleep(Duration::from_millis(150)).await;
            play_res = self.controller.play(&dev.av_transport_url, &dev.av_transport_service_type).await;
        }
        play_res?;

        // 4. Seek if starting at offset
        if start_sec > 0.0 {
            let ctrl = self.controller.clone();
            let av_url = dev.av_transport_url.clone();
            let svc_type = dev.av_transport_service_type.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let _ = ctrl.seek(&av_url, &svc_type, start_sec).await;
            });
        }

        // 5. Start position and state polling loop
        let (tx, rx) = watch::channel(false);
        *self.stop_tx.write().unwrap() = Some(tx);

        let ctrl = self.controller.clone();
        let av_url = dev.av_transport_url.clone();
        let svc_type = dev.av_transport_service_type.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            let mut was_playing = false;
            let mut play_seconds = 0;

            while !*rx.borrow() {
                interval.tick().await;
                if let Ok((pos, dur)) = ctrl.get_position_info(&av_url, &svc_type).await {
                    let state = ctrl.get_transport_info(&av_url, &svc_type).await
                        .unwrap_or_else(|_| "PLAYING".to_string());

                    if state == "PLAYING" || pos > 0.0 {
                        was_playing = true;
                        play_seconds += 1;
                    }

                    on_progress(pos, dur, &state);

                    let state_upper = state.to_uppercase();
                    let finished_by_state = was_playing && play_seconds > 4 && (state_upper == "STOPPED" || state_upper == "NO_MEDIA_PRESENT");
                    let finished_by_time = dur > 5.0 && pos >= (dur - 1.0);

                    if finished_by_state || finished_by_time {
                        on_ended();
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn pause(&self, device_id: &str) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.pause(&dev.av_transport_url, &dev.av_transport_service_type).await
    }

    pub async fn resume(&self, device_id: &str) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.play(&dev.av_transport_url, &dev.av_transport_service_type).await
    }

    pub async fn stop(&self, device_id: &str) -> Result<()> {
        if let Some(tx) = self.stop_tx.write().unwrap().take() {
            let _ = tx.send(true);
        }
        let dev = self.get_device_by_id(device_id)?;
        self.controller.stop(&dev.av_transport_url, &dev.av_transport_service_type).await
    }

    pub async fn seek(&self, device_id: &str, seconds: f64) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.seek(&dev.av_transport_url, &dev.av_transport_service_type, seconds).await
    }

    pub async fn set_volume(&self, device_id: &str, volume: u32) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.set_volume(&dev.rendering_control_url, &dev.rendering_control_service_type, volume).await
    }

    fn get_device_by_id(&self, device_id: &str) -> Result<DlnaDevice> {
        let devs = self.devices.read().unwrap();
        devs.iter()
            .find(|d| d.id == device_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))
    }
}
