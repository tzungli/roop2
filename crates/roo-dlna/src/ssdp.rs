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

    let mut seen_locations = std::collections::HashSet::new();
    let mut fetch_handles = Vec::new();
    let mut buf = [0u8; 4096];
    let end_time = tokio::time::Instant::now() + Duration::from_secs(timeout_sec.max(2));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;

    while tokio::time::Instant::now() < end_time {
        let remaining = end_time.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, _addr))) => {
                let text = String::from_utf8_lossy(&buf[..len]);
                if let Some(location) = parse_header_val(&text, "LOCATION") {
                    if seen_locations.insert(location.clone()) {
                        let c = client.clone();
                        fetch_handles.push(tokio::spawn(async move {
                            match fetch_device_desc(&c, &location).await {
                                Ok(dev) if dev.av_transport_url.starts_with("http") => Some(dev),
                                _ => None,
                            }
                        }));
                    }
                }
            }
            _ => break,
        }
    }

    // Collect all concurrently fetched descriptions and deduplicate by UDN
    let mut unique: HashMap<String, DlnaDevice> = HashMap::new();
    for handle in fetch_handles {
        if let Ok(Some(dev)) = handle.await {
            unique.entry(dev.id.clone()).or_insert(dev);
        }
    }

    Ok(unique.into_values().collect())
}

/// Fast direct HTTP probe for a previously known UPnP description URL (< 100ms on LAN).
pub async fn probe_device_location(location: &str, timeout_ms: u64) -> Result<DlnaDevice> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms.max(200)))
        .build()?;
    let dev = fetch_device_desc(&client, location).await?;
    if dev.av_transport_url.starts_with("http") {
        Ok(dev)
    } else {
        bail!("Device at {} has no valid AVTransport URL", location)
    }
}

/// Concurrently probe a list of remembered `(location, ip)` entries.
/// Tries exact `location` first (~20-80ms); if the port changed on reboot, falls back to fast parallel IP probe.
pub async fn probe_known_devices(entries: &[(String, String)]) -> Vec<DlnaDevice> {
    if entries.is_empty() {
        return Vec::new();
    }
    let mut handles = Vec::with_capacity(entries.len());
    for (loc, ip) in entries {
        let loc = loc.clone();
        let ip = ip.clone();
        handles.push(tokio::spawn(async move {
            if !loc.is_empty() {
                if let Ok(dev) = probe_device_location(&loc, 800).await {
                    return Some(dev);
                }
            }
            if !ip.is_empty() {
                if let Ok(dev) = probe_device_ip_fast(&ip, 700).await {
                    return Some(dev);
                }
            }
            None
        }));
    }

    let mut unique: HashMap<String, DlnaDevice> = HashMap::new();
    for h in handles {
        if let Ok(Some(dev)) = h.await {
            unique.entry(dev.id.clone()).or_insert(dev);
        }
    }
    unique.into_values().collect()
}

/// Fast parallel IP probe across common UPnP renderer ports/paths.
pub async fn probe_device_ip_fast(ip: &str, timeout_ms: u64) -> Result<DlnaDevice> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms.max(250)))
        .build()?;

    let candidate_ports = [49152, 49153, 49154, 8080, 1400, 49494, 50002, 80, 2869, 8008, 9000];
    let candidate_paths = [
        "description.xml",
        "devicedesc.xml",
        "device.xml",
        "upnp/desc.xml",
        "xml/device_description.xml",
        "dmr.xml",
    ];

    let mut handles = Vec::new();
    for port in candidate_ports {
        for path in candidate_paths {
            let url = format!("http://{}:{}/{}", ip, port, path);
            let c = client.clone();
            handles.push(tokio::spawn(async move {
                match fetch_device_desc(&c, &url).await {
                    Ok(dev) if dev.av_transport_url.starts_with("http") => Some(dev),
                    _ => None,
                }
            }));
        }
    }

    for h in handles {
        if let Ok(Some(dev)) = h.await {
            return Ok(dev);
        }
    }
    bail!("Device not found at IP: {}", ip);
}

pub async fn probe_device_ip(ip: &str) -> Result<DlnaDevice> {
    probe_device_ip_fast(ip, 1500).await
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

    let raw_name = extract_xml_value(&xml, "friendlyName").unwrap_or_else(|| "Unknown Renderer".to_string());
    let model = extract_xml_value(&xml, "modelName").unwrap_or_default();
    let manufacturer = extract_xml_value(&xml, "manufacturer").unwrap_or_default();
    let udn = extract_xml_value(&xml, "UDN").unwrap_or_else(|| location.to_string());
    let ip = loc_url.host_str().unwrap_or("").to_string();

    // Proactively exclude network equipment (routers, APs, WiFi nodes, gateways, printers)
    if is_network_or_non_audio_device(&xml, &raw_name, &model, &manufacturer) {
        bail!("Device '{}' ({}) at {} is network equipment and was excluded", raw_name, model, location);
    }

    let name = format_dlna_display_name(&raw_name, &model, &manufacturer);

    // Must have a valid AVTransport service to receive and play audio streams
    let (av_transport_url, av_transport_service_type) = extract_service(&xml, "AVTransport", &base_url)
        .ok_or_else(|| anyhow::anyhow!("Device '{}' at {} has no AVTransport service", name, location))?;

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

/// Build a clean human-friendly display name combining brand/manufacturer when `friendlyName`
/// only contains a short model code (e.g. `friendlyName="T8"`, `manufacturer="EVERSOLO"` -> `"EVERSOLO T8"`),
/// while avoiding duplicates when `friendlyName` already includes the brand (e.g. `"[LG] webOS G810SA"`).
pub fn format_dlna_display_name(friendly: &str, model: &str, manufacturer: &str) -> String {
    let f = friendly.trim();
    let m = model.trim();
    let mfr = manufacturer.trim();

    if f.is_empty() {
        return if !m.is_empty() { m.to_string() } else { "Unknown Renderer".to_string() };
    }

    let f_lower = f.to_lowercase();
    let m_lower = m.to_lowercase();

    // 1. If modelName already contains friendlyName and adds brand context (e.g. "EVERSOLO T8" vs "T8")
    if !m.is_empty() && m.len() > f.len() && m.len() <= 40 && m_lower.contains(&f_lower) {
        return m.to_string();
    }

    // 2. Otherwise check manufacturer for a clean brand prefix
    if !mfr.is_empty() {
        let mfr_lower = mfr.to_lowercase();
        let skip_mfr = [
            "http", ".com", ".net", ".org", "unknown", "default",
            "upnp", "dlna", "linux", "microsoft", "generic",
        ];
        if !skip_mfr.iter().any(|k| mfr_lower.contains(k)) {
            let brand_word = mfr
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
            if brand_word.len() >= 2 && brand_word.len() <= 20 && !f_lower.contains(&brand_word.to_lowercase()) {
                return format!("{} {}", brand_word, f);
            }
        }
    }

    f.to_string()
}

pub fn is_network_or_non_audio_device(xml: &str, name: &str, model: &str, manufacturer: &str) -> bool {
    let lower_xml = xml.to_lowercase();
    let lower_name = name.to_lowercase();
    let lower_model = model.to_lowercase();
    let lower_mfr = manufacturer.to_lowercase();

    // 1. UPnP deviceTypes representing non-audio network hardware
    let non_audio_types = [
        "internetgatewaydevice",
        "wandevice",
        "wanconnectiondevice",
        "landevice",
        "lanhostconfigmanagement",
        "device:basic",
        "device:printer",
        "device:scanner",
        "wfa-wsc",
        "wsc:1",
        "wps",
    ];
    for dt in &non_audio_types {
        if lower_xml.contains(dt) && !lower_xml.contains("device:mediarenderer") {
            return true;
        }
    }

    // 2. Pure MediaServer without MediaRenderer (e.g. NAS or router USB drive share)
    if lower_xml.contains("device:mediaserver") && !lower_xml.contains("device:mediarenderer") {
        return true;
    }

    // 3. Keywords identifying routers, wireless APs, mesh nodes, switches, modems
    let network_keywords = [
        "router", "gateway", "access point", "accesspoint", "wireless-ap",
        "無線分享器", "路由器", "交換器", "延伸器", "基地台", "無線路由器", "分享器",
        "mesh", "wi-fi node", "wifi node", "wifi-node", "repeater", "extender",
        "broadband", "modem", "switch", "ethernet",
        "rt-ax", "rt-ac", "archer", "deco", "orbi", "velop", "zenwifi",
        "asuswrt", "openwrt", "dd-wrt", "amplifi", "airies", "unifi",
        "synology router", "draytek", "vigor", "zyxel", "linksys", "fritz!box",
        "huawei home gateway", "smart wifi", "netgear nighthawk", "xiaomi router"
    ];

    for kw in &network_keywords {
        if lower_name.contains(kw) || lower_model.contains(kw) || lower_mfr.contains(kw) {
            if !lower_xml.contains("device:mediarenderer") && !lower_xml.contains("audiospeaker") {
                return true;
            }
        }
    }

    false
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

    #[test]
    fn test_is_network_or_non_audio_device() {
        // Router / Gateway XML
        let router_xml = r#"<device>
            <deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType>
            <friendlyName>RT-AX86U Wireless Router</friendlyName>
        </device>"#;
        assert!(is_network_or_non_audio_device(router_xml, "RT-AX86U", "RT-AX86U", "ASUSTeK"));

        // WiFi Mesh Node
        let node_xml = r#"<device>
            <deviceType>urn:schemas-upnp-org:device:Basic:1</deviceType>
            <friendlyName>Deco X60 Wi-Fi Node</friendlyName>
        </device>"#;
        assert!(is_network_or_non_audio_device(node_xml, "Deco X60 Wi-Fi Node", "Deco X60", "TP-Link"));

        // Legitimate Audio MediaRenderer
        let renderer_xml = r#"<device>
            <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
            <friendlyName>Eversolo DMP-A6</friendlyName>
        </device>"#;
        assert!(!is_network_or_non_audio_device(renderer_xml, "Eversolo DMP-A6", "DMP-A6", "Eversolo"));
    }

    #[test]
    fn test_format_dlna_display_name() {
        assert_eq!(format_dlna_display_name("T8", "T8", "EVERSOLO"), "EVERSOLO T8");
        assert_eq!(
            format_dlna_display_name("T8", "EVERSOLO T8", "EVERSOLO Audio Technology Co., Ltd."),
            "EVERSOLO T8"
        );
        assert_eq!(
            format_dlna_display_name("[LG] webOS G810SA", "LG TV", "LG Electronics"),
            "[LG] webOS G810SA"
        );
        assert_eq!(
            format_dlna_display_name("Eversolo DMP-A6", "DMP-A6", "EVERSOLO"),
            "Eversolo DMP-A6"
        );
    }

    #[tokio::test]
    async fn test_live_discover() {
        let devs = discover_dlna_devices(2).await.unwrap_or_default();
        for d in &devs {
            println!("DEVICE: name={}, ip={}, model={}, loc={}", d.name, d.ip, d.model, d.location);
        }
    }
}
