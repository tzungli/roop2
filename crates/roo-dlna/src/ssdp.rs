use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use anyhow::{bail, Result};
use crate::controller::extract_xml_value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlnaDevice {
    pub id: String, // udn
    pub name: String,
    pub model: String,
    pub manufacturer: String,
    pub location: String,
    pub ip: String,
    pub av_transport_url: String,
    pub rendering_control_url: String,
    #[serde(default = "default_av_transport_service_type")]
    pub av_transport_service_type: String,
    #[serde(default = "default_rendering_control_service_type")]
    pub rendering_control_service_type: String,
}

fn default_av_transport_service_type() -> String {
    "urn:schemas-upnp-org:service:AVTransport:1".to_string()
}

fn default_rendering_control_service_type() -> String {
    "urn:schemas-upnp-org:service:RenderingControl:1".to_string()
}

pub async fn discover_dlna_devices(timeout_sec: u64) -> Result<Vec<DlnaDevice>> {
    let bind_addr: SocketAddr = "0.0.0.0:0".parse()?;
    let socket = UdpSocket::bind(bind_addr).await?;
    socket.set_broadcast(true)?;

    let search_targets = [
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        "urn:schemas-upnp-org:service:AVTransport:1",
        "ssdp:all",
    ];

    let dest: SocketAddr = "239.255.255.250:1900".parse()?;

    // Send first wave
    for st in &search_targets {
        let search_msg = format!(
            "M-SEARCH * HTTP/1.1\r\n\
            HOST: 239.255.255.250:1900\r\n\
            MAN: \"ssdp:discover\"\r\n\
            MX: 2\r\n\
            ST: {}\r\n\
            \r\n",
            st
        );
        let _ = socket.send_to(search_msg.as_bytes(), dest).await;
    }

    // Re-send after 150ms to overcome wireless packet loss
    tokio::time::sleep(Duration::from_millis(150)).await;
    for st in &search_targets {
        let search_msg = format!(
            "M-SEARCH * HTTP/1.1\r\n\
            HOST: 239.255.255.250:1900\r\n\
            MAN: \"ssdp:discover\"\r\n\
            MX: 2\r\n\
            ST: {}\r\n\
            \r\n",
            st
        );
        let _ = socket.send_to(search_msg.as_bytes(), dest).await;
    }

    let mut devices = HashMap::new();
    let mut buf = [0u8; 4096];
    let end_time = tokio::time::Instant::now() + Duration::from_secs(timeout_sec.max(2));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    while tokio::time::Instant::now() < end_time {
        let remaining = end_time.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, _addr))) => {
                let text = String::from_utf8_lossy(&buf[..len]);
                if let Some(location) = parse_header_val(&text, "LOCATION") {
                    if !devices.contains_key(&location) {
                        if let Ok(dev) = fetch_device_desc(&client, &location).await {
                            if dev.av_transport_url.starts_with("http") {
                                devices.insert(location, dev);
                            }
                        }
                    }
                }
            }
            _ => break,
        }
    }

    // Deduplicate by device UDN
    let mut unique: HashMap<String, DlnaDevice> = HashMap::new();
    for (_loc, dev) in devices {
        unique.entry(dev.id.clone()).or_insert(dev);
    }

    Ok(unique.into_values().collect())
}

pub async fn probe_device_ip(ip: &str) -> Result<DlnaDevice> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    let candidate_ports = [8080, 49152, 49153, 49154, 1400, 80, 2869, 50002, 49494, 8008, 9000];
    let candidate_paths = [
        "description.xml",
        "devicedesc.xml",
        "device.xml",
        "upnp/desc.xml",
        "xml/device_description.xml",
        "dmr.xml",
        "wps_device.xml",
    ];

    for port in candidate_ports {
        for path in candidate_paths {
            let url = format!("http://{}:{}/{}", ip, port, path);
            if let Ok(dev) = fetch_device_desc(&client, &url).await {
                if dev.av_transport_url.starts_with("http") {
                    return Ok(dev);
                }
            }
        }
    }

    bail!("Device not found at IP: {}", ip);
}

fn parse_header_val(resp: &str, header: &str) -> Option<String> {
    for line in resp.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.trim().eq_ignore_ascii_case(header) {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

async fn fetch_device_desc(client: &reqwest::Client, location: &str) -> Result<DlnaDevice> {
    let xml = client.get(location).send().await?.text().await?;
    let loc_url = reqwest::Url::parse(location)?;

    // UPnP 1.0 section 2.1: URLBase if present and non-empty, otherwise description URL
    let base_url = if let Some(url_base_str) = extract_xml_value(&xml, "URLBase") {
        if !url_base_str.trim().is_empty() {
            loc_url.join(url_base_str.trim()).unwrap_or_else(|_| loc_url.clone())
        } else {
            loc_url.clone()
        }
    } else {
        loc_url.clone()
    };

    let name = extract_xml_value(&xml, "friendlyName").unwrap_or_else(|| "Unknown Renderer".to_string());
    let model = extract_xml_value(&xml, "modelName").unwrap_or_default();
    let manufacturer = extract_xml_value(&xml, "manufacturer").unwrap_or_default();
    let udn = extract_xml_value(&xml, "UDN").unwrap_or_else(|| location.to_string());
    let ip = loc_url.host_str().unwrap_or("").to_string();

    let (av_transport_url, av_transport_service_type) = extract_service(&xml, "AVTransport", &base_url)
        .unwrap_or_else(|| {
            let fallback_url = resolve_url(&base_url, "/AVTransport/control");
            (fallback_url, default_av_transport_service_type())
        });

    let (rendering_control_url, rendering_control_service_type) = extract_service(&xml, "RenderingControl", &base_url)
        .unwrap_or_else(|| {
            let fallback_url = resolve_url(&base_url, "/RenderingControl/control");
            (fallback_url, default_rendering_control_service_type())
        });

    Ok(DlnaDevice {
        id: udn,
        name,
        model,
        manufacturer,
        location: location.to_string(),
        ip,
        av_transport_url,
        rendering_control_url,
        av_transport_service_type,
        rendering_control_service_type,
    })
}

pub fn resolve_url(base: &reqwest::Url, rel: &str) -> String {
    let rel_trimmed = rel.trim();
    if let Ok(u) = base.join(rel_trimmed) {
        u.to_string()
    } else {
        rel_trimmed.to_string()
    }
}

pub fn extract_service(xml: &str, service_keyword: &str, base_url: &reqwest::Url) -> Option<(String, String)> {
    let lower_xml = xml.to_lowercase();
    let lower_kw = service_keyword.to_lowercase();

    let mut cur = 0;
    while let Some(start_idx) = lower_xml[cur..].find("<service") {
        let abs_start = cur + start_idx;
        let end_tag = "</service>";
        let abs_end = match lower_xml[abs_start..].find(end_tag) {
            Some(e) => abs_start + e + end_tag.len(),
            None => break,
        };

        let block = &xml[abs_start..abs_end];
        let lower_block = &lower_xml[abs_start..abs_end];

        if lower_block.contains(&lower_kw) {
            let ctrl_url_rel = extract_xml_value(block, "controlURL")
                .or_else(|| extract_xml_value(block, "controlUrl"))?;
            let service_type = extract_xml_value(block, "serviceType")
                .unwrap_or_else(|| format!("urn:schemas-upnp-org:service:{}:1", service_keyword));

            let resolved = resolve_url(base_url, &ctrl_url_rel);
            return Some((resolved, service_type));
        }

        cur = abs_end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_url_preserves_port() {
        let base = reqwest::Url::parse("http://192.168.1.50:49152/description.xml").unwrap();
        assert_eq!(resolve_url(&base, "/AVTransport/control"), "http://192.168.1.50:49152/AVTransport/control");
        assert_eq!(resolve_url(&base, "AVTransport/control"), "http://192.168.1.50:49152/AVTransport/control");
        assert_eq!(resolve_url(&base, "http://192.168.1.50:49152/AVTransport/control"), "http://192.168.1.50:49152/AVTransport/control");
    }

    #[test]
    fn test_extract_service() {
        let xml = r#"<device>
            <serviceList>
                <service>
                    <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
                    <serviceId>urn:upnp-org:serviceId:AVTransport</serviceId>
                    <controlURL>/upnp/control/avtransport</controlURL>
                    <eventSubURL>/upnp/event/avtransport</eventSubURL>
                </service>
                <service>
                    <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
                    <serviceId>urn:upnp-org:serviceId:RenderingControl</serviceId>
                    <controlURL>/upnp/control/rendering</controlURL>
                    <eventSubURL>/upnp/event/rendering</eventSubURL>
                </service>
            </serviceList>
        </device>"#;

        let base = reqwest::Url::parse("http://192.168.1.88:8080/description.xml").unwrap();
        let (av_url, av_st) = extract_service(xml, "AVTransport", &base).unwrap();
        assert_eq!(av_url, "http://192.168.1.88:8080/upnp/control/avtransport");
        assert_eq!(av_st, "urn:schemas-upnp-org:service:AVTransport:1");

        let (rc_url, rc_st) = extract_service(xml, "RenderingControl", &base).unwrap();
        assert_eq!(rc_url, "http://192.168.1.88:8080/upnp/control/rendering");
        assert_eq!(rc_st, "urn:schemas-upnp-org:service:RenderingControl:1");
    }
}
