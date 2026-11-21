use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use anyhow::{bail, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlnaDevice {
    pub id: String, // udn
    pub name: String,
    pub model: String,
    pub manufacturer: String,
    pub location: String,
    pub av_transport_url: String,
    pub rendering_control_url: String,
}

pub async fn discover_dlna_devices(timeout_sec: u64) -> Result<Vec<DlnaDevice>> {
    let bind_addr: SocketAddr = "0.0.0.0:0".parse()?;
    let socket = UdpSocket::bind(bind_addr).await?;
    socket.set_broadcast(true)?;

    let search_msg = "M-SEARCH * HTTP/1.1\r\n\
                      HOST: 239.255.255.250:1900\r\n\
                      MAN: \"ssdp:discover\"\r\n\
                      MX: 2\r\n\
                      ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\
                      \r\n";

    let dest: SocketAddr = "239.255.255.250:1900".parse()?;
    let _ = socket.send_to(search_msg.as_bytes(), dest).await;

    let mut devices = HashMap::new();
    let mut buf = [0u8; 4096];
    let end_time = tokio::time::Instant::now() + Duration::from_secs(timeout_sec);

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
                            devices.insert(location, dev);
                        }
                    }
                }
            }
            _ => break,
        }
    }

    Ok(devices.into_values().collect())
}

pub async fn probe_device_ip(ip: &str) -> Result<DlnaDevice> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    let candidate_ports = [8080, 49152, 49153, 49154, 80, 2869];
    for port in candidate_ports {
        let url = format!("http://{}:{}/description.xml", ip, port);
        if let Ok(dev) = fetch_device_desc(&client, &url).await {
            return Ok(dev);
        }
        let url = format!("http://{}:{}/devicedesc.xml", ip, port);
        if let Ok(dev) = fetch_device_desc(&client, &url).await {
            return Ok(dev);
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
    let base_url = if let Ok(u) = reqwest::Url::parse(location) {
        format!("{}://{}", u.scheme(), u.host_str().unwrap_or(""))
    } else {
        "".to_string()
    };

    let name = extract_xml_tag(&xml, "friendlyName").unwrap_or_else(|| "Unknown Renderer".to_string());
    let model = extract_xml_tag(&xml, "modelName").unwrap_or_default();
    let manufacturer = extract_xml_tag(&xml, "manufacturer").unwrap_or_default();
    let udn = extract_xml_tag(&xml, "UDN").unwrap_or_else(|| location.to_string());

    let av_transport_url = extract_service_url(&xml, "AVTransport", &base_url)
        .unwrap_or_else(|| format!("{}/AVTransport/control", base_url));
    let rendering_control_url = extract_service_url(&xml, "RenderingControl", &base_url)
        .unwrap_or_else(|| format!("{}/RenderingControl/control", base_url));

    Ok(DlnaDevice {
        id: udn,
        name,
        model,
        manufacturer,
        location: location.to_string(),
        av_transport_url,
        rendering_control_url,
    })
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    let s = xml.find(&start_tag)? + start_tag.len();
    let e = xml[s..].find(&end_tag)? + s;
    Some(xml[s..e].trim().to_string())
}

fn extract_service_url(xml: &str, service_id: &str, base_url: &str) -> Option<String> {
    let mut search_pos = 0;
    while let Some(pos) = xml[search_pos..].find(service_id) {
        let abs_pos = search_pos + pos;
        let block_start = xml[..abs_pos].rfind("<service>")?;
        let block_end = xml[abs_pos..].find("</service>")? + abs_pos;
        let block = &xml[block_start..block_end];
        if let Some(ctrl) = extract_xml_tag(block, "controlURL") {
            let full_url = if ctrl.starts_with("http://") || ctrl.starts_with("https://") {
                ctrl
            } else if ctrl.starts_with('/') {
                format!("{}{}", base_url, ctrl)
            } else {
                format!("{}/{}", base_url, ctrl)
            };
            return Some(full_url);
        }
        search_pos = abs_pos + service_id.len();
    }
    None
}
