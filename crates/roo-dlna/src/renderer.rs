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
    active_device: Arc<RwLock<Option<DlnaDevice>>>,
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
            active_device: Arc::new(RwLock::new(None)),
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
        if device_id == "local" || device_id.is_empty() {
            *self.selected_device_id.write().unwrap() = None;
            return true;
        }
        let devs = self.devices.read().unwrap();
        if devs.iter().any(|d| d.id == device_id) {
            *self.selected_device_id.write().unwrap() = Some(device_id.to_string());
            true
        } else {
            false
        }
    }

    /// Stop whatever DLNA device is currently playing/selected (used during app shutdown
    /// so hardware renderers immediately mute & flush their audio buffer instead of playing it out).
    pub async fn stop_active(&self) {
        if let Some(tx) = self.stop_tx.write().unwrap().take() {
            let _ = tx.send(true);
        }
        let target_dev = self
            .active_device
            .write()
            .unwrap()
            .take()
            .or_else(|| self.get_selected_device());

        if let Some(dev) = target_dev {
            tracing::info!("Force-stopping active DLNA renderer on shutdown: {} ({})", dev.name, dev.ip);
            // 1. Pause first: instantly mutes DAC output on renderers that fade/drain on Stop
            let _ = tokio::time::timeout(
                Duration::from_millis(500),
                self.controller.pause(&dev.av_transport_url, &dev.av_transport_service_type),
            )
            .await;
            // 2. Stop transport
            let _ = tokio::time::timeout(
                Duration::from_millis(800),
                self.controller.stop(&dev.av_transport_url, &dev.av_transport_service_type),
            )
            .await;
            // 3. Clear AVTransport URI to force hardware pipeline teardown & ring-buffer flush
            let _ = tokio::time::timeout(
                Duration::from_millis(500),
                self.controller.clear_uri(&dev.av_transport_url, &dev.av_transport_service_type),
            )
            .await;
            tokio::time::sleep(Duration::from_millis(80)).await;
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
        dsd_hz: Option<u32>,
        duration_sec: Option<f64>,
        start_sec: f64,
        on_progress: impl Fn(f64, f64, &str) + Send + Sync + 'static,
        on_ended: impl Fn() + Send + Sync + 'static,
    ) -> Result<()> {
        *self.selected_device_id.write().unwrap() = Some(device_id.to_string());
        let dev = match self.get_device(device_id) {
            Some(d) => d,
            None => bail!("Device not found: {}", device_id),
        };
        *self.active_device.write().unwrap() = Some(dev.clone());

        // Stop any previous progress loop
        if let Some(tx) = self.stop_tx.write().unwrap().take() {
            let _ = tx.send(true);
        }

        // 1. Ensure transport is stopped first to avoid error 705 (Transport locked)
        let _ = self.controller.stop(&dev.av_transport_url, &dev.av_transport_service_type).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 2. Set AVTransport URI with proper metadata (including title, artist, album, cover art, duration, dsd_hz)
        let set_res = self.controller.set_av_transport_uri(
            &dev.av_transport_url,
            &dev.av_transport_service_type,
            stream_url,
            title,
            artist,
            album,
            cover_url,
            dsd_hz,
            duration_sec,
        ).await;
        set_res?;

        // 3. Wait for transport state to finish transitioning, then trigger Play
        for _ in 0..15 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(state) = self.controller.get_transport_info(&dev.av_transport_url, &dev.av_transport_service_type).await {
                let st = state.to_uppercase();
                if st != "TRANSITIONING" {
                    break;
                }
            } else {
                break;
            }
        }

        let mut play_res = self.controller.play(&dev.av_transport_url, &dev.av_transport_service_type).await;
        if play_res.is_err() {
            for _ in 0..3 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                play_res = self.controller.play(&dev.av_transport_url, &dev.av_transport_service_type).await;
                if play_res.is_ok() {
                    break;
                }
            }
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
            let mut near_end_count = 0;
            let start_time = std::time::Instant::now();

            while !*rx.borrow() {
                interval.tick().await;
                if let Ok((pos, dur)) = ctrl.get_position_info(&av_url, &svc_type).await {
                    let state = ctrl.get_transport_info(&av_url, &svc_type).await
                        .unwrap_or_else(|_| "PLAYING".to_string());

                    let state_upper = state.to_uppercase();
                    if state_upper == "PLAYING" || pos > 0.0 {
                        was_playing = true;
                        play_seconds += 1;
                    }

                    // Filter out stale near-end position from prior track during startup
                    if start_time.elapsed().as_secs_f64() < 2.5 && dur > 5.0 && pos >= (dur - 2.0) {
                        continue;
                    }

                    on_progress(pos, dur, &state);

                    // A track has finished only if it was genuinely playing AND:
                    // 1. Explicitly reported STOPPED or NO_MEDIA_PRESENT/COMPLETED and is actually near the end of track
                    // 2. Or reached duration for at least 2 consecutive ticks after playing for > 4s
                    let is_near_end = if dur > 5.0 {
                        pos >= (dur - 4.0)
                    } else {
                        play_seconds > 4
                    };

                    let finished_by_state = was_playing && (state_upper == "STOPPED" || state_upper == "NO_MEDIA_PRESENT" || state_upper == "COMPLETED") && is_near_end;
                    
                    if was_playing && play_seconds > 4 && dur > 5.0 && pos >= (dur - 1.5) {
                        near_end_count += 1;
                    } else {
                        near_end_count = 0;
                    }
                    let finished_by_time = near_end_count >= 2;

                    if finished_by_state || finished_by_time {
                        tracing::info!("Track finished on DLNA renderer (state={}, pos={}, dur={})", state, pos, dur);
                        on_ended();
                        break;
                    } else if was_playing && play_seconds > 4 && (state_upper == "STOPPED" || state_upper == "NO_MEDIA_PRESENT") {
                        tracing::warn!("DLNA renderer stopped prematurely midway (state={}, pos={}, dur={}). Ending poll without on_ended to prevent track skip loop.", state, pos, dur);
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
        *self.active_device.write().unwrap() = None;
        let dev = self.get_device_by_id(device_id)?;
        let res = self.controller.stop(&dev.av_transport_url, &dev.av_transport_service_type).await;
        let _ = self.controller.clear_uri(&dev.av_transport_url, &dev.av_transport_service_type).await;
        res
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
