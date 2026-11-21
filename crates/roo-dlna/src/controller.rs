use std::time::Duration;
use anyhow::{bail, Result};

pub struct UpnpController {
    client: reqwest::Client,
}

impl Default for UpnpController {
    fn default() -> Self {
        Self::new()
    }
}

impl UpnpController {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        Self { client }
    }

    pub async fn set_av_transport_uri(&self, control_url: &str, stream_url: &str, title: &str, artist: &str) -> Result<()> {
        let meta = format!(
            "&lt;DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\"&gt;\
            &lt;item id=\"0\" parentID=\"0\" restricted=\"1\"&gt;\
            &lt;dc:title&gt;{}&lt;/dc:title&gt;\
            &lt;dc:creator&gt;{}&lt;/dc:creator&gt;\
            &lt;upnp:class&gt;object.item.audioItem.musicTrack&lt;/upnp:class&gt;\
            &lt;res protocolInfo=\"http-get:*:audio/wav:*\"&gt;{}&lt;/res&gt;\
            &lt;/item&gt;\
            &lt;/DIDL-Lite&gt;",
            title, artist, stream_url
        );

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:SetAVTransportURI xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
            <InstanceID>0</InstanceID>\
            <CurrentURI>{}</CurrentURI>\
            <CurrentURIMetaData>{}</CurrentURIMetaData>\
            </u:SetAVTransportURI>\
            </s:Body>\
            </s:Envelope>",
            stream_url, meta
        );

        self.soap_action(control_url, "urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI", &body).await
    }

    pub async fn play(&self, control_url: &str) -> Result<()> {
        let body = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:Play xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
            <InstanceID>0</InstanceID>\
            <Speed>1</Speed>\
            </u:Play>\
            </s:Body>\
            </s:Envelope>";
        self.soap_action(control_url, "urn:schemas-upnp-org:service:AVTransport:1#Play", body).await
    }

    pub async fn pause(&self, control_url: &str) -> Result<()> {
        let body = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:Pause xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
            <InstanceID>0</InstanceID>\
            </u:Pause>\
            </s:Body>\
            </s:Envelope>";
        self.soap_action(control_url, "urn:schemas-upnp-org:service:AVTransport:1#Pause", body).await
    }

    pub async fn stop(&self, control_url: &str) -> Result<()> {
        let body = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:Stop xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
            <InstanceID>0</InstanceID>\
            </u:Stop>\
            </s:Body>\
            </s:Envelope>";
        self.soap_action(control_url, "urn:schemas-upnp-org:service:AVTransport:1#Stop", body).await
    }

    pub async fn seek(&self, control_url: &str, seconds: f64) -> Result<()> {
        let s = seconds.max(0.0).round() as u64;
        let hh = s / 3600;
        let mm = (s % 3600) / 60;
        let ss = s % 60;
        let target = format!("{:02}:{:02}:{:02}", hh, mm, ss);

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:Seek xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
            <InstanceID>0</InstanceID>\
            <Unit>REL_TIME</Unit>\
            <Target>{}</Target>\
            </u:Seek>\
            </s:Body>\
            </s:Envelope>",
            target
        );
        self.soap_action(control_url, "urn:schemas-upnp-org:service:AVTransport:1#Seek", &body).await
    }

    pub async fn set_volume(&self, control_url: &str, volume: u32) -> Result<()> {
        let vol = volume.clamp(0, 100);
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:SetVolume xmlns:u=\"urn:schemas-upnp-org:service:RenderingControl:1\">\
            <InstanceID>0</InstanceID>\
            <Channel>Master</Channel>\
            <DesiredVolume>{}</DesiredVolume>\
            </u:SetVolume>\
            </s:Body>\
            </s:Envelope>",
            vol
        );
        self.soap_action(control_url, "urn:schemas-upnp-org:service:RenderingControl:1#SetVolume", &body).await
    }

    pub async fn get_position_info(&self, control_url: &str) -> Result<(f64, f64)> {
        let body = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:GetPositionInfo xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\">\
            <InstanceID>0</InstanceID>\
            </u:GetPositionInfo>\
            </s:Body>\
            </s:Envelope>";

        let resp = self.client.post(control_url)
            .header("SOAPAction", "\"urn:schemas-upnp-org:service:AVTransport:1#GetPositionInfo\"")
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .body(body)
            .send()
            .await?
            .text()
            .await?;

        let rel_time = extract_xml_tag(&resp, "RelTime").unwrap_or_default();
        let track_dur = extract_xml_tag(&resp, "TrackDuration").unwrap_or_default();

        let pos_sec = parse_duration_hhmmss(&rel_time);
        let dur_sec = parse_duration_hhmmss(&track_dur);

        Ok((pos_sec, dur_sec))
    }

    async fn soap_action(&self, url: &str, action: &str, body: &str) -> Result<()> {
        let resp = self.client.post(url)
            .header("SOAPAction", format!("\"{}\"", action))
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .body(body.to_string())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_txt = resp.text().await.unwrap_or_default();
            bail!("SOAP action failed [{}]: {}", status, err_txt);
        }
        Ok(())
    }
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    let s = xml.find(&start_tag)? + start_tag.len();
    let e = xml[s..].find(&end_tag)? + s;
    Some(xml[s..e].trim().to_string())
}

fn parse_duration_hhmmss(s: &str) -> f64 {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h: f64 = parts[0].parse().unwrap_or(0.0);
        let m: f64 = parts[1].parse().unwrap_or(0.0);
        let sec: f64 = parts[2].parse().unwrap_or(0.0);
        h * 3600.0 + m * 60.0 + sec
    } else {
        0.0
    }
}
