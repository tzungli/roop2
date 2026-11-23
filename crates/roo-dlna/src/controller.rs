use std::time::Duration;
use anyhow::{bail, Result};

pub fn format_hms_ms(seconds: f64) -> String {
    if seconds <= 0.0 || seconds.is_nan() {
        return String::new();
    }
    let s = seconds.floor() as u64;
    let hrs = s / 3600;
    let mins = (s % 3600) / 60;
    let secs = s % 60;
    let ms = ((seconds - seconds.floor()) * 1000.0).floor() as u64;
    format!("{}:{:02}:{:02}.{:03}", hrs, mins, secs, ms)
}

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
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        Self { client }
    }

    pub async fn set_av_transport_uri(
        &self,
        control_url: &str,
        service_type: &str,
        stream_url: &str,
        title: &str,
        artist: &str,
        album: &str,
        cover_url: Option<&str>,
        dsd_hz: Option<u32>,
        duration_sec: Option<f64>,
    ) -> Result<()> {
        let st = if service_type.trim().is_empty() {
            "urn:schemas-upnp-org:service:AVTransport:1"
        } else {
            service_type.trim()
        };

        let title_esc = xml_escape(title);
        let artist_esc = xml_escape(artist);
        let album_esc = xml_escape(album);
        let stream_url_esc = xml_escape(stream_url);

        let dur_attr = duration_sec
            .map(format_hms_ms)
            .filter(|s| !s.is_empty())
            .map(|s| format!(" duration=\"{}\"", s))
            .unwrap_or_default();

        let res_tags = if let Some(hz) = dsd_hz {
            format!(
                "<res protocolInfo=\"http-get:*:audio/x-dsf:DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000\" sampleFrequency=\"{}\" bitsPerSample=\"1\" nrAudioChannels=\"2\"{}>{}</res>\
                 <res protocolInfo=\"http-get:*:audio/dsf:DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000\" sampleFrequency=\"{}\" bitsPerSample=\"1\" nrAudioChannels=\"2\"{}>{}</res>\
                 <res protocolInfo=\"http-get:*:audio/x-dsd:*\" sampleFrequency=\"{}\" bitsPerSample=\"1\" nrAudioChannels=\"2\"{}>{}</res>\
                 <res protocolInfo=\"http-get:*:audio/x-dsf:*\"{}>{}</res>",
                hz, dur_attr, stream_url_esc, hz, dur_attr, stream_url_esc, hz, dur_attr, stream_url_esc, dur_attr, stream_url_esc
            )
        } else {
            format!(
                "<res protocolInfo=\"http-get:*:audio/wav:DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000\"{}>{}</res>\
                 <res protocolInfo=\"http-get:*:audio/x-wav:*\"{}>{}</res>",
                dur_attr, stream_url_esc, dur_attr, stream_url_esc
            )
        };

        let mut cover_tags = String::new();
        if let Some(curl) = cover_url {
            if !curl.trim().is_empty() {
                let c_esc = xml_escape(curl.trim());
                cover_tags = format!(
                    "<upnp:albumArtURI dlna:profileID=\"JPEG_TN\">{}</upnp:albumArtURI>\
                     <upnp:albumArtURI dlna:profileID=\"JPEG_SM\">{}</upnp:albumArtURI>\
                     <upnp:albumArtURI dlna:profileID=\"JPEG_MED\">{}</upnp:albumArtURI>\
                     <upnp:albumArtURI dlna:profileID=\"JPEG_LRG\">{}</upnp:albumArtURI>\
                     <upnp:albumArtURI>{}</upnp:albumArtURI>\
                     <upnp:icon>{}</upnp:icon>",
                    c_esc, c_esc, c_esc, c_esc, c_esc, c_esc
                );
            }
        }

        let mut optional_meta = String::new();
        if !artist_esc.is_empty() {
            optional_meta.push_str(&format!("<upnp:artist role=\"Performer\">{}</upnp:artist>", artist_esc));
            optional_meta.push_str(&format!("<upnp:artist>{}</upnp:artist>", artist_esc));
        }
        if !album_esc.is_empty() {
            optional_meta.push_str(&format!("<upnp:album>{}</upnp:album>", album_esc));
            optional_meta.push_str(&format!("<upnp:albumArtist>{}</upnp:albumArtist>", album_esc));
        }
        optional_meta.push_str(&cover_tags);

        let didl_rich = format!(
            "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\" xmlns:dlna=\"urn:schemas-dlna-org:metadata-1-0/\">\
            <item id=\"1\" parentID=\"0\" restricted=\"1\">\
            <dc:title>{}</dc:title>\
            <dc:creator>{}</dc:creator>\
            {}\
            <upnp:class>object.item.audioItem.musicTrack</upnp:class>\
            {}\
            </item>\
            </DIDL-Lite>",
            title_esc, artist_esc, optional_meta, res_tags
        );

        // Attempt 1: Standard Rich Metadata (Title, Artist, Album, Cover Art)
        match self.send_set_av_transport_uri(control_url, st, stream_url, &didl_rich).await {
            Ok(_) => return Ok(()),
            Err(e1) => {
                eprintln!("[DLNA] SetAVTransportURI with rich metadata failed: {}. Retrying with basic metadata...", e1);
            }
        }

        // Attempt 2: Basic Metadata (Title + Creator only - verified working on all devices)
        let didl_basic = format!(
            "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">\
            <item id=\"1\" parentID=\"0\" restricted=\"1\">\
            <dc:title>{}</dc:title>\
            <dc:creator>{}</dc:creator>\
            <upnp:class>object.item.audioItem.musicTrack</upnp:class>\
            {}\
            </item>\
            </DIDL-Lite>",
            title_esc, artist_esc, res_tags
        );

        match self.send_set_av_transport_uri(control_url, st, stream_url, &didl_basic).await {
            Ok(_) => return Ok(()),
            Err(e2) => {
                eprintln!("[DLNA] SetAVTransportURI with basic metadata failed: {}. Retrying with empty metadata...", e2);
            }
        }

        // Attempt 3: Empty metadata (universal UPnP specification fallback)
        self.send_set_av_transport_uri(control_url, st, stream_url, "").await
    }

    async fn send_set_av_transport_uri(
        &self,
        control_url: &str,
        service_type: &str,
        stream_url: &str,
        didl_raw: &str,
    ) -> Result<()> {
        let didl_escaped = if didl_raw.is_empty() {
            String::new()
        } else {
            xml_escape(didl_raw)
        };
        let current_uri_escaped = xml_escape(stream_url);

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:SetAVTransportURI xmlns:u=\"{}\">\
            <InstanceID>0</InstanceID>\
            <CurrentURI>{}</CurrentURI>\
            <CurrentURIMetaData>{}</CurrentURIMetaData>\
            </u:SetAVTransportURI>\
            </s:Body>\
            </s:Envelope>",
            service_type, current_uri_escaped, didl_escaped
        );

        self.soap_action(control_url, &format!("{}#SetAVTransportURI", service_type), &body).await
    }

    pub async fn play(&self, control_url: &str, service_type: &str) -> Result<()> {
        let st = if service_type.trim().is_empty() {
            "urn:schemas-upnp-org:service:AVTransport:1"
        } else {
            service_type.trim()
        };

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:Play xmlns:u=\"{}\">\
            <InstanceID>0</InstanceID>\
            <Speed>1</Speed>\
            </u:Play>\
            </s:Body>\
            </s:Envelope>",
            st
        );
        self.soap_action(control_url, &format!("{}#Play", st), &body).await
    }

    pub async fn pause(&self, control_url: &str, service_type: &str) -> Result<()> {
        let st = if service_type.trim().is_empty() {
            "urn:schemas-upnp-org:service:AVTransport:1"
        } else {
            service_type.trim()
        };

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:Pause xmlns:u=\"{}\">\
            <InstanceID>0</InstanceID>\
            </u:Pause>\
            </s:Body>\
            </s:Envelope>",
            st
        );
        self.soap_action(control_url, &format!("{}#Pause", st), &body).await
    }

    pub async fn stop(&self, control_url: &str, service_type: &str) -> Result<()> {
        let st = if service_type.trim().is_empty() {
            "urn:schemas-upnp-org:service:AVTransport:1"
        } else {
            service_type.trim()
        };

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:Stop xmlns:u=\"{}\">\
            <InstanceID>0</InstanceID>\
            </u:Stop>\
            </s:Body>\
            </s:Envelope>",
            st
        );
        self.soap_action(control_url, &format!("{}#Stop", st), &body).await
    }

    pub async fn seek(&self, control_url: &str, service_type: &str, seconds: f64) -> Result<()> {
        let st = if service_type.trim().is_empty() {
            "urn:schemas-upnp-org:service:AVTransport:1"
        } else {
            service_type.trim()
        };

        let s = seconds.max(0.0).round() as u64;
        let hh = s / 3600;
        let mm = (s % 3600) / 60;
        let ss = s % 60;
        let target = format!("{:02}:{:02}:{:02}", hh, mm, ss);

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:Seek xmlns:u=\"{}\">\
            <InstanceID>0</InstanceID>\
            <Unit>REL_TIME</Unit>\
            <Target>{}</Target>\
            </u:Seek>\
            </s:Body>\
            </s:Envelope>",
            st, target
        );
        self.soap_action(control_url, &format!("{}#Seek", st), &body).await
    }

    pub async fn set_volume(&self, control_url: &str, service_type: &str, volume: u32) -> Result<()> {
        let st = if service_type.trim().is_empty() {
            "urn:schemas-upnp-org:service:RenderingControl:1"
        } else {
            service_type.trim()
        };

        let vol = volume.clamp(0, 100);
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:SetVolume xmlns:u=\"{}\">\
            <InstanceID>0</InstanceID>\
            <Channel>Master</Channel>\
            <DesiredVolume>{}</DesiredVolume>\
            </u:SetVolume>\
            </s:Body>\
            </s:Envelope>",
            st, vol
        );
        self.soap_action(control_url, &format!("{}#SetVolume", st), &body).await
    }

    pub async fn get_position_info(&self, control_url: &str, service_type: &str) -> Result<(f64, f64)> {
        let st = if service_type.trim().is_empty() {
            "urn:schemas-upnp-org:service:AVTransport:1"
        } else {
            service_type.trim()
        };

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:GetPositionInfo xmlns:u=\"{}\">\
            <InstanceID>0</InstanceID>\
            </u:GetPositionInfo>\
            </s:Body>\
            </s:Envelope>",
            st
        );

        let resp = self.client.post(control_url)
            .header("SOAPAction", format!("\"{}#GetPositionInfo\"", st))
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("User-Agent", "rooPlayer/0.1.0 UPnP/1.0 DLNADOC/1.50")
            .body(body)
            .send()
            .await?
            .text()
            .await?;

        let rel_time = extract_xml_value(&resp, "RelTime").unwrap_or_default();
        let track_dur = extract_xml_value(&resp, "TrackDuration").unwrap_or_default();

        let pos_sec = parse_duration_hhmmss(&rel_time);
        let dur_sec = parse_duration_hhmmss(&track_dur);

        Ok((pos_sec, dur_sec))
    }

    pub async fn get_transport_info(&self, control_url: &str, service_type: &str) -> Result<String> {
        let st = if service_type.trim().is_empty() {
            "urn:schemas-upnp-org:service:AVTransport:1"
        } else {
            service_type.trim()
        };

        let body = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
            <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
            <s:Body>\
            <u:GetTransportInfo xmlns:u=\"{}\">\
            <InstanceID>0</InstanceID>\
            </u:GetTransportInfo>\
            </s:Body>\
            </s:Envelope>",
            st
        );

        let resp = self.client.post(control_url)
            .header("SOAPAction", format!("\"{}#GetTransportInfo\"", st))
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("User-Agent", "rooPlayer/0.1.0 UPnP/1.0 DLNADOC/1.50")
            .body(body)
            .send()
            .await?
            .text()
            .await?;

        let state = extract_xml_value(&resp, "CurrentTransportState")
            .unwrap_or_else(|| "UNKNOWN".to_string());
        Ok(state)
    }

    async fn soap_action(&self, url: &str, action: &str, body: &str) -> Result<()> {
        let resp = self.client.post(url)
            .header("SOAPAction", format!("\"{}\"", action))
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("User-Agent", "rooPlayer/0.1.0 UPnP/1.0 DLNADOC/1.50")
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

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn extract_xml_value(xml: &str, tag_name: &str) -> Option<String> {
    let lower_xml = xml.to_lowercase();
    let lower_tag = tag_name.to_lowercase();

    let mut search_pos = 0;
    while let Some(start_bracket) = lower_xml[search_pos..].find('<') {
        let abs_start = search_pos + start_bracket;
        let rest = &lower_xml[abs_start + 1..];
        if rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('!') {
            search_pos = abs_start + 1;
            continue;
        }

        let tag_open_end = match rest.find('>') {
            Some(idx) => abs_start + 1 + idx,
            None => break,
        };

        let tag_content = &lower_xml[abs_start + 1..tag_open_end];
        let tag_ident = tag_content.split_whitespace().next().unwrap_or("");
        let bare_ident = tag_ident.split(':').last().unwrap_or(tag_ident);

        if bare_ident == lower_tag {
            let after_open = tag_open_end + 1;
            if after_open >= xml.len() {
                return None;
            }

            let mut close_pos = None;
            let mut cur_close = after_open;
            while let Some(c_idx) = lower_xml[cur_close..].find("</") {
                let abs_c = cur_close + c_idx;
                let c_rest = &lower_xml[abs_c + 2..];
                if let Some(c_end) = c_rest.find('>') {
                    let c_ident = lower_xml[abs_c + 2..abs_c + 2 + c_end].trim();
                    let c_bare = c_ident.split(':').last().unwrap_or(c_ident);
                    if c_bare == lower_tag {
                        close_pos = Some(abs_c);
                        break;
                    }
                    cur_close = abs_c + 2 + c_end;
                } else {
                    break;
                }
            }

            if let Some(end_idx) = close_pos {
                return Some(xml[after_open..end_idx].trim().to_string());
            }
        }

        search_pos = abs_start + 1;
    }
    None
}

pub fn parse_duration_hhmmss(s: &str) -> f64 {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h: f64 = parts[0].parse().unwrap_or(0.0);
        let m: f64 = parts[1].parse().unwrap_or(0.0);
        let sec: f64 = parts[2].parse().unwrap_or(0.0);
        h * 3600.0 + m * 60.0 + sec
    } else if parts.len() == 2 {
        let m: f64 = parts[0].parse().unwrap_or(0.0);
        let sec: f64 = parts[1].parse().unwrap_or(0.0);
        m * 60.0 + sec
    } else {
        s.parse().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("Rock & Roll <Live>"), "Rock &amp; Roll &lt;Live&gt;");
        assert_eq!(xml_escape("http://192.168.1.5:39840/sox?a=1&b=2"), "http://192.168.1.5:39840/sox?a=1&amp;b=2");
    }

    #[test]
    fn test_extract_xml_value() {
        let xml = "<s:Envelope><s:Body><u:GetPositionInfoResponse xmlns:u=\"urn:schemas-upnp-org:service:AVTransport:1\"><Track>1</Track><TrackDuration>00:04:15.5</TrackDuration><RelTime>00:01:23</RelTime></u:GetPositionInfoResponse></s:Body></s:Envelope>";
        assert_eq!(extract_xml_value(xml, "RelTime"), Some("00:01:23".to_string()));
        assert_eq!(extract_xml_value(xml, "TrackDuration"), Some("00:04:15.5".to_string()));
        assert_eq!(extract_xml_value(xml, "Track"), Some("1".to_string()));
        assert_eq!(extract_xml_value(xml, "NotFound"), None);

        let xml_prefixed = "<s:Envelope><s:Body><u:GetPositionInfoResponse><u:RelTime>00:02:45</u:RelTime></u:GetPositionInfoResponse></s:Body></s:Envelope>";
        assert_eq!(extract_xml_value(xml_prefixed, "RelTime"), Some("00:02:45".to_string()));
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration_hhmmss("01:02:03"), 3723.0);
        assert_eq!(parse_duration_hhmmss("00:01:23.5"), 83.5);
        assert_eq!(parse_duration_hhmmss("03:45"), 225.0);
    }

    #[test]
    fn test_format_hms_ms() {
        assert_eq!(format_hms_ms(0.0), "");
        assert_eq!(format_hms_ms(-5.0), "");
        assert_eq!(format_hms_ms(225.5), "0:03:45.500");
        assert_eq!(format_hms_ms(3661.123), "1:01:01.123");
    }
}
