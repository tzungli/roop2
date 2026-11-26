use std::collections::HashMap;
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
    /// Per-zone active DLNA devices (supports independent multi-zone playback)
    active_devices: Arc<RwLock<HashMap<String, DlnaDevice>>>,
    /// Per-zone polling stop channels keyed by device_id
    stop_txs: Arc<RwLock<HashMap<String, watch::Sender<bool>>>>,
    /// Per-zone active progressive stream token keyed by device_id
    active_tokens: Arc<RwLock<HashMap<String, String>>>,
    /// Per-zone gapless next-track stream token keyed by device_id
    next_tokens: Arc<RwLock<HashMap<String, String>>>,
    /// Per-zone gapless next-track URI queued via SetNextAVTransportURI
    next_uris: Arc<RwLock<HashMap<String, String>>>,
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
            active_devices: Arc::new(RwLock::new(HashMap::new())),
            stop_txs: Arc::new(RwLock::new(HashMap::new())),
            active_tokens: Arc::new(RwLock::new(HashMap::new())),
            next_tokens: Arc::new(RwLock::new(HashMap::new())),
            next_uris: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record the new stream token for `device_id`, returning its previous token (if any)
    /// so the caller can cancel ONLY that device's old stream without affecting other zones.
    pub fn replace_device_token(&self, device_id: &str, new_token: String) -> Option<String> {
        self.next_uris.write().unwrap().remove(device_id);
        self.active_tokens
            .write()
            .unwrap()
            .insert(device_id.to_string(), new_token)
    }

    /// Record the gapless next-track token for `device_id`, returning any stale next token.
    pub fn replace_device_next_token(&self, device_id: &str, new_token: String) -> Option<String> {
        self.next_tokens
            .write()
            .unwrap()
            .insert(device_id.to_string(), new_token)
    }

    /// Remove and return the gapless next-track token for `device_id` (if any).
    pub fn take_device_next_token(&self, device_id: &str) -> Option<String> {
        self.next_uris.write().unwrap().remove(device_id);
        self.next_tokens.write().unwrap().remove(device_id)
    }

    /// Remove and return the active stream token for `device_id` (if any).
    pub fn take_device_token(&self, device_id: &str) -> Option<String> {
        self.active_tokens.write().unwrap().remove(device_id)
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

    /// Stop ALL active DLNA devices concurrently (used during app shutdown so every
    /// hardware renderer immediately mutes & flushes its audio buffer).
    pub async fn stop_active(&self) {
        let txs: Vec<watch::Sender<bool>> = {
            let mut map = self.stop_txs.write().unwrap();
            map.drain().map(|(_, tx)| tx).collect()
        };
        for tx in txs {
            let _ = tx.send(true);
        }

        let mut targets: Vec<DlnaDevice> = {
            let mut map = self.active_devices.write().unwrap();
            map.drain().map(|(_, dev)| dev).collect()
        };
        if let Some(sel_dev) = self.get_selected_device() {
            if !targets.iter().any(|d| d.id == sel_dev.id) {
                targets.push(sel_dev);
            }
        }
        self.active_tokens.write().unwrap().clear();
        self.next_tokens.write().unwrap().clear();
        self.next_uris.write().unwrap().clear();

        let mut handles = Vec::new();
        for dev in targets {
            let ctrl = self.controller.clone();
            handles.push(tokio::spawn(async move {
                tracing::info!("Force-stopping active DLNA renderer on shutdown: {} ({})", dev.name, dev.ip);
                let _ = tokio::time::timeout(
                    Duration::from_millis(500),
                    ctrl.pause(&dev.av_transport_url, &dev.av_transport_service_type),
                )
                .await;
                let _ = tokio::time::timeout(
                    Duration::from_millis(800),
                    ctrl.stop(&dev.av_transport_url, &dev.av_transport_service_type),
                )
                .await;
                let _ = tokio::time::timeout(
                    Duration::from_millis(500),
                    ctrl.clear_uri(&dev.av_transport_url, &dev.av_transport_service_type),
                )
                .await;
            }));
        }
        for h in handles {
            let _ = h.await;
        }
        if !self.devices.read().unwrap().is_empty() {
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

    /// Queue the next track on a DLNA renderer via UPnP SetNextAVTransportURI for true gapless playback.
    pub async fn queue_next(
        &self,
        device_id: &str,
        stream_url: &str,
        title: &str,
        artist: &str,
        album: &str,
        cover_url: Option<&str>,
        dsd_hz: Option<u32>,
        pcm_hz: Option<u32>,
        duration_sec: Option<f64>,
    ) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.set_next_av_transport_uri(
            &dev.av_transport_url,
            &dev.av_transport_service_type,
            stream_url,
            title,
            artist,
            album,
            cover_url,
            dsd_hz,
            pcm_hz,
            duration_sec,
        ).await?;
        self.next_uris
            .write()
            .unwrap()
            .insert(device_id.to_string(), stream_url.to_string());
        tracing::info!("[DLNA Gapless] Queued SetNextAVTransportURI on {}: {}", dev.name, stream_url);
        Ok(())
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
        pcm_hz: Option<u32>,
        duration_sec: Option<f64>,
        start_sec: f64,
        on_progress: impl Fn(f64, f64, &str) + Send + Sync + 'static,
        on_ended: impl Fn() + Send + Sync + 'static,
        on_gapless_advanced: impl Fn() + Send + Sync + 'static,
    ) -> Result<()> {
        let dev = match self.get_device(device_id) {
            Some(d) => d,
            None => bail!("Device not found: {}", device_id),
        };
        self.active_devices
            .write()
            .unwrap()
            .insert(device_id.to_string(), dev.clone());

        // Stop only THIS device's previous progress loop (leave other zones running!)
        if let Some(tx) = self.stop_txs.write().unwrap().remove(device_id) {
            let _ = tx.send(true);
        }

        // 1. Ensure transport is stopped first to avoid error 705 (Transport locked)
        let _ = self.controller.stop(&dev.av_transport_url, &dev.av_transport_service_type).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 2. Set AVTransport URI with proper metadata (including title, artist, album, cover art, duration, dsd_hz/pcm_hz)
        let set_res = self.controller.set_av_transport_uri(
            &dev.av_transport_url,
            &dev.av_transport_service_type,
            stream_url,
            title,
            artist,
            album,
            cover_url,
            dsd_hz,
            pcm_hz,
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

        // 5. Start adaptive low-chatter position & gapless transition polling loop
        let (tx, rx) = watch::channel(false);
        self.stop_txs
            .write()
            .unwrap()
            .insert(device_id.to_string(), tx);

        let ctrl = self.controller.clone();
        let av_url = dev.av_transport_url.clone();
        let svc_type = dev.av_transport_service_type.clone();
        let next_uris = self.next_uris.clone();
        let next_tokens = self.next_tokens.clone();
        let active_tokens = self.active_tokens.clone();
        let dev_id_owned = device_id.to_string();

        tokio::spawn(async move {
            let mut was_playing = false;
            let mut play_ticks = 0u32;
            let mut near_end_count = 0u32;
            let mut was_near_end = false;
            let mut start_time = std::time::Instant::now();
            let mut last_dur = 0.0f64;
            let mut last_pos = 0.0f64;

            while !*rx.borrow() {
                // Adaptive low-chatter polling:
                // 500ms during startup (first 3s) or near track end (last 12s),
                // 1000ms during steady mid-track playback to minimize network/CPU chatter on hardware streamers.
                let is_fast_window = start_time.elapsed().as_secs_f64() < 3.5
                    || (last_dur > 5.0 && last_pos >= (last_dur - 12.0));
                tokio::time::sleep(Duration::from_millis(if is_fast_window { 500 } else { 1000 })).await;
                if *rx.borrow() {
                    break;
                }

                if let Ok((pos, dur, track_uri)) = ctrl.get_position_info_full(&av_url, &svc_type).await {
                    let state = ctrl.get_transport_info(&av_url, &svc_type).await
                        .unwrap_or_else(|_| "PLAYING".to_string());

                    let state_upper = state.to_uppercase();

                    // Check if renderer seamlessly transitioned to our queued Gapless NextURI!
                    let queued_next = next_uris.read().unwrap().get(&dev_id_owned).cloned();
                    if let Some(expected_next_uri) = queued_next {
                        let uri_matched = !track_uri.is_empty() && track_uri == expected_next_uri;
                        let pos_wrapped = was_near_end && pos < 3.0 && state_upper == "PLAYING";
                        if uri_matched || pos_wrapped {
                            tracing::info!(
                                "[DLNA Gapless] Seamless track transition detected on {} (uri_matched={}, pos={:.1})",
                                dev_id_owned, uri_matched, pos
                            );
                            next_uris.write().unwrap().remove(&dev_id_owned);
                            if let Some(promoted_tok) = next_tokens.write().unwrap().remove(&dev_id_owned) {
                                active_tokens.write().unwrap().insert(dev_id_owned.clone(), promoted_tok);
                            }
                            was_playing = true;
                            play_ticks = 0;
                            near_end_count = 0;
                            was_near_end = false;
                            start_time = std::time::Instant::now();
                            last_pos = pos;
                            last_dur = dur;
                            on_gapless_advanced();
                            on_progress(pos, dur, &state);
                            continue;
                        }
                    }

                    if state_upper == "PLAYING" || pos > 0.0 {
                        was_playing = true;
                        play_ticks += 1;
                    }

                    // Filter out stale near-end position from prior track during startup
                    if start_time.elapsed().as_secs_f64() < 2.5 && dur > 5.0 && pos >= (dur - 2.0) {
                        continue;
                    }

                    last_pos = pos;
                    if dur > 0.0 {
                        last_dur = dur;
                    }
                    if dur > 5.0 && pos >= (dur - 5.0) {
                        was_near_end = true;
                    }

                    on_progress(pos, dur, &state);

                    let is_near_end = if dur > 5.0 {
                        pos >= (dur - 4.0)
                    } else {
                        play_ticks > 4
                    };

                    let finished_by_state = was_playing && (state_upper == "STOPPED" || state_upper == "NO_MEDIA_PRESENT" || state_upper == "COMPLETED") && is_near_end;

                    if was_playing && play_ticks > 4 && dur > 5.0 && pos >= (dur - 1.5) {
                        near_end_count += 1;
                    } else {
                        near_end_count = 0;
                    }
                    // If a gapless next URI is queued, give the renderer 1 extra tick to seamlessly wrap before firing on_ended
                    let has_gapless_queued = next_uris.read().unwrap().contains_key(&dev_id_owned);
                    let end_ticks_needed = if has_gapless_queued { 4 } else { 2 };
                    let finished_by_time = near_end_count >= end_ticks_needed;

                    if finished_by_state || finished_by_time {
                        tracing::info!("Track finished on DLNA renderer (state={}, pos={}, dur={})", state, pos, dur);
                        on_ended();
                        break;
                    } else if was_playing && play_ticks > 4 && (state_upper == "STOPPED" || state_upper == "NO_MEDIA_PRESENT") {
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
        if let Some(tx) = self.stop_txs.write().unwrap().remove(device_id) {
            let _ = tx.send(true);
        }
        self.active_devices.write().unwrap().remove(device_id);
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

    pub async fn get_volume(&self, device_id: &str) -> Result<u32> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.get_volume(&dev.rendering_control_url, &dev.rendering_control_service_type).await
    }

    fn get_device_by_id(&self, device_id: &str) -> Result<DlnaDevice> {
        let devs = self.devices.read().unwrap();
        devs.iter()
            .find(|d| d.id == device_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))
    }
}
