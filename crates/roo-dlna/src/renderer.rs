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

    pub async fn play(
        &self,
        device_id: &str,
        stream_url: &str,
        title: &str,
        artist: &str,
        start_sec: f64,
        on_progress: impl Fn(f64, f64) + Send + Sync + 'static,
    ) -> Result<()> {
        let dev = {
            let devs = self.devices.read().unwrap();
            devs.iter().find(|d| d.id == device_id).cloned()
        };
        let dev = match dev {
            Some(d) => d,
            None => bail!("Device not found: {}", device_id),
        };

        // Stop any previous progress loop
        if let Some(tx) = self.stop_tx.write().unwrap().take() {
            let _ = tx.send(true);
        }

        self.controller.set_av_transport_uri(&dev.av_transport_url, stream_url, title, artist).await?;
        if start_sec > 0.0 {
            let _ = self.controller.seek(&dev.av_transport_url, start_sec).await;
        }
        self.controller.play(&dev.av_transport_url).await?;

        // Start position poll loop
        let (tx, rx) = watch::channel(false);
        *self.stop_tx.write().unwrap() = Some(tx);

        let ctrl = self.controller.clone();
        let av_url = dev.av_transport_url.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            while !*rx.borrow() {
                interval.tick().await;
                if let Ok((pos, dur)) = ctrl.get_position_info(&av_url).await {
                    on_progress(pos, dur);
                }
            }
        });

        Ok(())
    }

    pub async fn pause(&self, device_id: &str) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.pause(&dev.av_transport_url).await
    }

    pub async fn resume(&self, device_id: &str) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.play(&dev.av_transport_url).await
    }

    pub async fn stop(&self, device_id: &str) -> Result<()> {
        if let Some(tx) = self.stop_tx.write().unwrap().take() {
            let _ = tx.send(true);
        }
        let dev = self.get_device_by_id(device_id)?;
        self.controller.stop(&dev.av_transport_url).await
    }

    pub async fn seek(&self, device_id: &str, seconds: f64) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.seek(&dev.av_transport_url, seconds).await
    }

    pub async fn set_volume(&self, device_id: &str, volume: u32) -> Result<()> {
        let dev = self.get_device_by_id(device_id)?;
        self.controller.set_volume(&dev.rendering_control_url, volume).await
    }

    fn get_device_by_id(&self, device_id: &str) -> Result<DlnaDevice> {
        let devs = self.devices.read().unwrap();
        devs.iter()
            .find(|d| d.id == device_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))
    }
}
