use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use anyhow::{bail, Result};
use crate::controller::extract_xml_value;

const MAX_DIAG_LINES: usize = 200;
const SSDP_MULTICAST_V4: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const SSDP_PORT: u16 = 1900;

static DLNA_DIAG_BUFFER: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
static PASSIVE_LISTENER_STATUS: OnceLock<Mutex<String>> = OnceLock::new();

fn diag_buffer() -> &'static Mutex<VecDeque<String>> {
    DLNA_DIAG_BUFFER.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_DIAG_LINES)))
}

fn listener_status_lock() -> &'static Mutex<String> {
    PASSIVE_LISTENER_STATUS.get_or_init(|| Mutex::new("not started".to_string()))
}

pub fn set_passive_listener_status(status: impl Into<String>) {
    if let Ok(mut g) = listener_status_lock().lock() {
        *g = status.into();
    }
}

pub fn get_passive_listener_status() -> String {
    listener_status_lock()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "unknown".to_string())
}

pub fn push_dlna_diag(level: &str, msg: impl AsRef<str>) {
    let text = msg.as_ref();
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let line = format!("[{}][{}] {}", secs, level, text);
    if level == "WARN" || level == "ERR" {
        tracing::warn!("[DLNA] {}", text);
    } else {
        tracing::info!("[DLNA] {}", text);
    }
    if let Ok(mut buf) = diag_buffer().lock() {
        if buf.len() >= MAX_DIAG_LINES {
            buf.pop_front();
        }
        buf.push_back(line);
    }
}

pub fn get_dlna_diag_logs() -> Vec<String> {
    diag_buffer()
        .lock()
        .map(|b| b.iter().cloned().collect())
        .unwrap_or_default()
}

pub fn clear_dlna_diag_logs() {
    if let Ok(mut b) = diag_buffer().lock() {
        b.clear();
    }
}

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

fn push_unique_ipv4(ips: &mut Vec<Ipv4Addr>, ip: Ipv4Addr) {
    if !ip.is_unspecified()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_multicast()
        && !ips.contains(&ip)
    {
        ips.push(ip);
    }
}

/// Enumerate active non-loopback, non-link-local IPv4 addresses across all local network interfaces.
pub fn list_local_ipv4_interfaces() -> Vec<Ipv4Addr> {
    let mut ips = Vec::new();

    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        #[allow(non_snake_case)]
        struct MibIpAddrRow {
            dwAddr: u32,
            dwIndex: u32,
            dwMask: u32,
            dwBCastAddr: u32,
            dwReasmSize: u32,
            unused1: u16,
            wType: u16,
        }
        #[link(name = "iphlpapi")]
        extern "system" {
            fn GetIpAddrTable(pIpAddrTable: *mut u8, pdwSize: *mut u32, bOrder: i32) -> u32;
        }
        unsafe {
            let mut size: u32 = 0;
            let _ = GetIpAddrTable(std::ptr::null_mut(), &mut size, 0);
            if size >= 4 && size <= 256 * 1024 {
                let mut buf = vec![0u8; size as usize];
                let ret = GetIpAddrTable(buf.as_mut_ptr(), &mut size, 0);
                if ret == 0 && buf.len() >= 4 {
                    let count = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
                    let row_size = std::mem::size_of::<MibIpAddrRow>();
                    let table_ptr = buf.as_ptr().add(4) as *const MibIpAddrRow;
                    let max_rows = (buf.len().saturating_sub(4)) / row_size;
                    for i in 0..count.min(max_rows) {
                        let row = &*table_ptr.add(i);
                        // Convert network-order DWORD to Ipv4Addr
                        let ip = Ipv4Addr::from(u32::from_be(row.dwAddr));
                        push_unique_ipv4(&mut ips, ip);
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) == 0 && !ifap.is_null() {
            let mut cur = ifap;
            while !cur.is_null() {
                let ifa = &*cur;
                if !ifa.ifa_addr.is_null() && (*ifa.ifa_addr).sa_family as i32 == libc::AF_INET {
                    let is_up = (ifa.ifa_flags as i32 & libc::IFF_UP) != 0;
                    if is_up {
                        let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                        let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                        push_unique_ipv4(&mut ips, ip);
                    }
                }
                cur = ifa.ifa_next;
            }
            libc::freeifaddrs(ifap);
        }
    }

    // Fallback outbound route probes if OS enumeration returned nothing
    if ips.is_empty() {
        for target in ["8.8.8.8:80", "192.168.1.1:80", "192.168.50.1:80", "192.168.0.1:80", "10.0.0.1:80"] {
            if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
                if sock.connect(target).is_ok() {
                    if let Ok(SocketAddr::V4(v4)) = sock.local_addr() {
                        push_unique_ipv4(&mut ips, *v4.ip());
                    }
                }
            }
        }
    }

    ips
}

fn build_msearch_packet(st: &str, mx: u8) -> Vec<u8> {
    format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: {}\r\nST: {}\r\nUSER-AGENT: rooPlayer/1.0 UPnP/1.1\r\n\r\n",
        mx, st
    )
    .into_bytes()
}

fn bind_multicast_sender_socket(bind_ip: Ipv4Addr) -> Result<UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    let _ = sock.set_reuse_address(true);
    let _ = sock.set_broadcast(true);
    let _ = sock.set_multicast_ttl_v4(4);
    if !bind_ip.is_unspecified() {
        let _ = sock.set_multicast_if_v4(&bind_ip);
    }
    sock.set_nonblocking(true)?;
    let addr = SocketAddr::V4(SocketAddrV4::new(bind_ip, 0));
    sock.bind(&addr.into())?;
    let std_sock: std::net::UdpSocket = sock.into();
    Ok(UdpSocket::from_std(std_sock)?)
}

fn bind_ssdp_notify_socket(ifaces: &[Ipv4Addr]) -> Result<UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    #[cfg(unix)]
    unsafe {
        use std::os::unix::io::AsRawFd;
        let optval: libc::c_int = 1;
        let _ = libc::setsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &optval as *const _ as *const libc::c_void,
            std::mem::size_of_val(&optval) as libc::socklen_t,
        );
    }
    sock.set_nonblocking(true)?;
    let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, SSDP_PORT));
    sock.bind(&bind_addr.into())?;

    if ifaces.is_empty() {
        let _ = sock.join_multicast_v4(&SSDP_MULTICAST_V4, &Ipv4Addr::UNSPECIFIED);
    } else {
        for &ip in ifaces {
            if let Err(e) = sock.join_multicast_v4(&SSDP_MULTICAST_V4, &ip) {
                push_dlna_diag("WARN", format!("SSDP NOTIFY join_multicast_v4 failed on {}: {}", ip, e));
            }
        }
    }

    let std_sock: std::net::UdpSocket = sock.into();
    Ok(UdpSocket::from_std(std_sock)?)
}

fn build_dlna_http_client(timeout_ms: u64) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms.max(400)))
        .connect_timeout(Duration::from_millis(timeout_ms.min(2000).max(400)))
        .build()?)
}

pub async fn discover_dlna_devices(timeout_sec: u64) -> Result<Vec<DlnaDevice>> {
    let search_targets = [
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        "urn:schemas-upnp-org:device:MediaRenderer:2",
        "urn:schemas-upnp-org:service:AVTransport:1",
        "upnp:rootdevice",
        "ssdp:all",
    ];

    let mut bind_ips = list_local_ipv4_interfaces();
    if bind_ips.is_empty() {
        bind_ips.push(Ipv4Addr::UNSPECIFIED);
    }
    push_dlna_diag(
        "INFO",
        format!(
            "Starting SSDP M-SEARCH across {} interface(s): {:?} | targets={:?}",
            bind_ips.len(),
            bind_ips,
            search_targets
        ),
    );

    let dest = SocketAddr::V4(SocketAddrV4::new(SSDP_MULTICAST_V4, SSDP_PORT));
    let mut sockets: Vec<(Ipv4Addr, Arc<UdpSocket>)> = Vec::new();
    for &ip in &bind_ips {
        match bind_multicast_sender_socket(ip) {
            Ok(s) => sockets.push((ip, Arc::new(s))),
            Err(e) => push_dlna_diag("WARN", format!("Failed to bind SSDP socket on {}: {}", ip, e)),
        }
    }
    if sockets.is_empty() {
        let fallback = Arc::new(bind_multicast_sender_socket(Ipv4Addr::UNSPECIFIED)?);
        sockets.push((Ipv4Addr::UNSPECIFIED, fallback));
    }

    // Wave 1
    for (_ip, sock) in &sockets {
        for st in &search_targets {
            let pkt = build_msearch_packet(st, 2);
            let _ = sock.send_to(&pkt, dest).await;
        }
    }

    // Wave 2 after 160ms to overcome Wi-Fi / IGMP loss
    tokio::time::sleep(Duration::from_millis(160)).await;
    for (_ip, sock) in &sockets {
        for st in &search_targets {
            let pkt = build_msearch_packet(st, 2);
            let _ = sock.send_to(&pkt, dest).await;
        }
    }

    let listen_dur = Duration::from_secs(timeout_sec.max(3));
    let seen_locations = Arc::new(Mutex::new(HashSet::<String>::new()));
    let fetch_handles = Arc::new(Mutex::new(Vec::new()));
    let client = build_dlna_http_client(3500)?;

    let mut recv_tasks = Vec::with_capacity(sockets.len());
    for (iface_ip, sock) in sockets {
        let seen = seen_locations.clone();
        let handles_ref = fetch_handles.clone();
        let c = client.clone();
        recv_tasks.push(tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            let deadline = tokio::time::Instant::now() + listen_dur;
            while tokio::time::Instant::now() < deadline {
                let rem = deadline.saturating_duration_since(tokio::time::Instant::now());
                match tokio::time::timeout(rem, sock.recv_from(&mut buf)).await {
                    Ok(Ok((len, src_addr))) => {
                        let text = String::from_utf8_lossy(&buf[..len]);
                        if let Some(location) = parse_header_val(&text, "LOCATION") {
                            let is_new = seen
                                .lock()
                                .map(|mut s| s.insert(location.clone()))
                                .unwrap_or(false);
                            if is_new {
                                let st_val = parse_header_val(&text, "ST")
                                    .or_else(|| parse_header_val(&text, "NT"))
                                    .unwrap_or_default();
                                let server_val = parse_header_val(&text, "SERVER").unwrap_or_default();
                                push_dlna_diag(
                                    "INFO",
                                    format!(
                                        "SSDP response via {} from {} | LOCATION={} | ST={} | SERVER={}",
                                        iface_ip, src_addr, location, st_val, server_val
                                    ),
                                );
                                let hc = c.clone();
                                let loc_clone = location.clone();
                                let h = tokio::spawn(async move {
                                    match fetch_device_desc(&hc, &loc_clone).await {
                                        Ok(dev) if dev.av_transport_url.starts_with("http") => Some(dev),
                                        Ok(dev) => {
                                            push_dlna_diag(
                                                "WARN",
                                                format!("Rejected '{}' ({}): invalid AVTransport URL '{}'", dev.name, loc_clone, dev.av_transport_url),
                                            );
                                            None
                                        }
                                        Err(e) => {
                                            push_dlna_diag("WARN", format!("Ignored {}: {}", loc_clone, e));
                                            None
                                        }
                                    }
                                });
                                if let Ok(mut g) = handles_ref.lock() {
                                    g.push(h);
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
        }));
    }

    for rt in recv_tasks {
        let _ = rt.await;
    }

    let handles = {
        let mut g = fetch_handles.lock().unwrap();
        std::mem::take(&mut *g)
    };

    // Collect all concurrently fetched descriptions and deduplicate by UDN
    let mut unique: HashMap<String, DlnaDevice> = HashMap::new();
    for handle in handles {
        if let Ok(Some(dev)) = handle.await {
            unique.entry(dev.id.clone()).or_insert(dev);
        }
    }

    push_dlna_diag(
        "INFO",
        format!("SSDP discovery finished: {} valid audio renderer(s) found", unique.len()),
    );
    Ok(unique.into_values().collect())
}

#[derive(Debug, Clone)]
pub enum SsdpNotifyEvent {
    Alive(DlnaDevice),
    ByeBye { usn: String },
}

/// Spawn a passive SSDP NOTIFY listener on `239.255.255.250:1900` (`ssdp:alive` / `ssdp:byebye`).
/// Automatically invokes `on_event` whenever an audio renderer boots up, wakes from sleep,
/// or announces itself on the LAN after rooPlayer has already started.
pub fn spawn_ssdp_notify_listener<F>(on_event: F)
where
    F: Fn(SsdpNotifyEvent) + Send + Sync + 'static,
{
    tokio::spawn(async move {
        let ifaces = list_local_ipv4_interfaces();
        let sock = match bind_ssdp_notify_socket(&ifaces) {
            Ok(s) => {
                let status = format!("listening on 0.0.0.0:1900 (ifaces={:?})", ifaces);
                set_passive_listener_status(&status);
                push_dlna_diag("INFO", format!("SSDP passive NOTIFY listener active: {}", status));
                s
            }
            Err(e) => {
                let status = format!("unavailable ({})", e);
                set_passive_listener_status(&status);
                push_dlna_diag("WARN", format!("SSDP passive NOTIFY listener failed to bind :1900: {}", e));
                return;
            }
        };

        let client = match build_dlna_http_client(3500) {
            Ok(c) => c,
            Err(_) => reqwest::Client::new(),
        };
        let on_event = Arc::new(on_event);
        // Throttle repeat probes of the same LOCATION to at most once every 30s
        let recent_probes: Arc<Mutex<HashMap<String, std::time::Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let mut buf = [0u8; 4096];
        loop {
            let Ok((len, src_addr)) = sock.recv_from(&mut buf).await else {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            };
            let text = String::from_utf8_lossy(&buf[..len]).to_string();
            let first_line = text.lines().next().unwrap_or("").trim().to_ascii_uppercase();
            if !first_line.starts_with("NOTIFY ") {
                continue;
            }
            let nts = parse_header_val(&text, "NTS").unwrap_or_default().to_ascii_lowercase();
            let nt = parse_header_val(&text, "NT").unwrap_or_default();

            if nts == "ssdp:byebye" {
                let usn = parse_header_val(&text, "USN").unwrap_or_default();
                if !usn.is_empty() {
                    push_dlna_diag("INFO", format!("SSDP NOTIFY ssdp:byebye from {} (NT={}, USN={})", src_addr, nt, usn));
                    on_event(SsdpNotifyEvent::ByeBye { usn });
                }
                continue;
            }

            if nts == "ssdp:alive" {
                let nt_lower = nt.to_ascii_lowercase();
                let interesting = nt_lower.contains("mediarenderer")
                    || nt_lower.contains("avtransport")
                    || nt_lower == "upnp:rootdevice"
                    || nt_lower.contains("openhome");
                if !interesting {
                    continue;
                }
                if let Some(location) = parse_header_val(&text, "LOCATION") {
                    let should_probe = {
                        let mut g = recent_probes.lock().unwrap();
                        if g.len() > 256 {
                            let cutoff = std::time::Instant::now() - Duration::from_secs(60);
                            g.retain(|_, ts| *ts > cutoff);
                        }
                        let now = std::time::Instant::now();
                        match g.get(&location) {
                            Some(prev) if now.duration_since(*prev) < Duration::from_secs(30) => false,
                            _ => {
                                g.insert(location.clone(), now);
                                true
                            }
                        }
                    };
                    if !should_probe {
                        continue;
                    }
                    push_dlna_diag(
                        "INFO",
                        format!("SSDP NOTIFY ssdp:alive from {} | LOCATION={} | NT={}", src_addr, location, nt),
                    );
                    let c = client.clone();
                    let cb = on_event.clone();
                    tokio::spawn(async move {
                        match fetch_device_desc(&c, &location).await {
                            Ok(dev) if dev.av_transport_url.starts_with("http") => {
                                cb(SsdpNotifyEvent::Alive(dev));
                            }
                            Ok(_) => {}
                            Err(e) => {
                                push_dlna_diag("WARN", format!("SSDP NOTIFY ignored {}: {}", location, e));
                            }
                        }
                    });
                }
            }
        }
    });
}

/// Fast direct HTTP probe for a previously known UPnP description URL (< 100ms on LAN).
pub async fn probe_device_location(location: &str, timeout_ms: u64) -> Result<DlnaDevice> {
    let client = build_dlna_http_client(timeout_ms)?;
    let dev = fetch_device_desc(&client, location).await?;
    if dev.av_transport_url.starts_with("http") {
        Ok(dev)
    } else {
        bail!("Device at {} has no valid AVTransport URL", location)
    }
}

/// Concurrently probe a list of remembered `(location, probe_target_or_ip)` entries.
/// Tries exact `location` first (~20-120ms); if the port changed on reboot, falls back to fast TCP/SSDP target probe.
pub async fn probe_known_devices(entries: &[(String, String)]) -> Vec<DlnaDevice> {
    if entries.is_empty() {
        return Vec::new();
    }
    let mut handles = Vec::with_capacity(entries.len());
    for (loc, target) in entries {
        let loc = loc.clone();
        let target = target.clone();
        handles.push(tokio::spawn(async move {
            if !loc.is_empty() {
                if let Ok(dev) = probe_device_location(&loc, 1200).await {
                    return Some(dev);
                }
            }
            if !target.is_empty() {
                if let Ok(dev) = probe_device_ip_fast(&target, 1800).await {
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

/// Normalize and parse user input such as:
/// - `192.168.50.10`
/// - `192.168.50.10:49152`
/// - `192.168.50.10:49152/description.xml`
/// - `http://192.168.50.10:49152/description.xml`
/// - `holored.local`
fn parse_probe_target(raw_input: &str) -> (String, Option<u16>, Option<String>) {
    let normalized = raw_input
        .trim()
        .replace('：', ":")
        .replace(" :", ":")
        .replace(": ", ":");
    let s = normalized.trim();
    if s.is_empty() {
        return (String::new(), None, None);
    }

    let with_scheme = if s.starts_with("http://") || s.starts_with("https://") {
        Some(s.to_string())
    } else if s.contains('/') {
        Some(format!("http://{}", s.trim_start_matches('/')))
    } else {
        None
    };

    if let Some(candidate_url) = with_scheme {
        if let Ok(u) = reqwest::Url::parse(&candidate_url) {
            let host = u.host_str().unwrap_or("").trim_matches(['[', ']']).to_string();
            let port = u.port_or_known_default();
            let path_non_empty = u.path().trim_matches('/');
            if !path_non_empty.is_empty() {
                return (host, port, Some(u.to_string()));
            }
            return (host, port, None);
        }
    }

    if let Some((h, p)) = s.rsplit_once(':') {
        let host_part = h.trim();
        let port_part = p.trim();
        // Avoid splitting bare IPv6 without brackets unless bracketed like `[::1]:49152`
        let is_bracketed_v6 = host_part.starts_with('[') && host_part.ends_with(']');
        if !host_part.contains(':') || is_bracketed_v6 {
            if let Ok(port) = port_part.parse::<u16>() {
                return (
                    host_part.trim_matches(['[', ']']).to_string(),
                    Some(port),
                    None,
                );
            }
        }
    }

    (s.trim_matches(['[', ']']).to_string(), None, None)
}

async fn resolve_probe_ipv4(host: &str) -> Option<Ipv4Addr> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Some(ip);
    }
    let lookup = tokio::time::timeout(
        Duration::from_millis(600),
        tokio::net::lookup_host((host, SSDP_PORT)),
    )
    .await
    .ok()?
    .ok()?;
    for addr in lookup {
        if let SocketAddr::V4(v4) = addr {
            return Some(*v4.ip());
        }
    }
    None
}

/// Send unicast SSDP `M-SEARCH` directly to `<host>:1900` to obtain the exact `LOCATION` URL
/// without guessing port numbers (works even on arbitrary ports like 1858, 1772, 49155, etc.).
async fn unicast_ssdp_locations(host_ip: Ipv4Addr, timeout_ms: u64) -> Vec<String> {
    let mut locs = Vec::new();
    let targets = [
        "upnp:rootdevice",
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        "urn:schemas-upnp-org:device:MediaRenderer:2",
        "urn:schemas-upnp-org:service:AVTransport:1",
        "ssdp:all",
    ];

    // Pick the local NIC that routes to `host_ip` so Windows sends unicast via the right interface
    let bind_ip = if let Ok(probe_sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if probe_sock
            .connect(SocketAddr::V4(SocketAddrV4::new(host_ip, SSDP_PORT)))
            .is_ok()
        {
            match probe_sock.local_addr() {
                Ok(SocketAddr::V4(v4)) if !v4.ip().is_loopback() && !v4.ip().is_unspecified() => {
                    *v4.ip()
                }
                _ => Ipv4Addr::UNSPECIFIED,
            }
        } else {
            Ipv4Addr::UNSPECIFIED
        }
    } else {
        Ipv4Addr::UNSPECIFIED
    };

    let Ok(sock) = bind_multicast_sender_socket(bind_ip) else {
        return locs;
    };
    let dest = SocketAddr::V4(SocketAddrV4::new(host_ip, SSDP_PORT));
    for st in &targets {
        let pkt = build_msearch_packet(st, 1);
        let _ = sock.send_to(&pkt, dest).await;
    }

    let mut seen = HashSet::new();
    let mut buf = [0u8; 4096];
    let mut deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms.max(400));
    let mut got_first = false;
    while tokio::time::Instant::now() < deadline {
        let rem = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(rem, sock.recv_from(&mut buf)).await {
            Ok(Ok((len, src))) => {
                let text = String::from_utf8_lossy(&buf[..len]);
                if let Some(location) = parse_header_val(&text, "LOCATION") {
                    if seen.insert(location.clone()) {
                        push_dlna_diag(
                            "INFO",
                            format!("Unicast SSDP reply from {} => LOCATION={}", src, location),
                        );
                        locs.push(location);
                        if !got_first {
                            got_first = true;
                            // Short grace window to collect any remaining LOCATION headers from the same device
                            let early = tokio::time::Instant::now() + Duration::from_millis(220);
                            if early < deadline {
                                deadline = early;
                            }
                        }
                    }
                }
            }
            _ => break,
        }
    }
    locs
}

/// Fast concurrent TCP port reachability pre-check so HTTP XML probing only hits open ports
/// and fails fast on closed/firewalled ports instead of wasting seconds timing out 150+ URLs.
async fn discover_open_tcp_ports(host: &str, candidate_ports: &[u16], timeout_ms: u64) -> Vec<u16> {
    let mut handles = Vec::with_capacity(candidate_ports.len());
    for &port in candidate_ports {
        let addr = format!("{}:{}", host, port);
        handles.push(tokio::spawn(async move {
            let ok = tokio::time::timeout(
                Duration::from_millis(timeout_ms),
                tokio::net::TcpStream::connect(&addr),
            )
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false);
            if ok {
                Some(port)
            } else {
                None
            }
        }));
    }

    let mut open = Vec::new();
    for h in handles {
        if let Ok(Some(p)) = h.await {
            if !open.contains(&p) {
                open.push(p);
            }
        }
    }
    open
}

/// Fast, reliable IP probe supporting plain IP, `IP:Port`, `IP:Port/path`, hostname, or full XML URL:
/// 1) If a full XML URL was given, fetches it first (and falls back to same port paths on 404);
/// 2) Runs unicast SSDP `M-SEARCH` + concurrent TCP open-port check in parallel;
/// 3) Probes only open TCP ports across common UPnP XML paths using `JoinSet` for immediate first-hit return.
pub async fn probe_device_ip_fast(ip_or_url: &str, timeout_ms: u64) -> Result<DlnaDevice> {
    let (host, explicit_port, explicit_url) = parse_probe_target(ip_or_url);
    push_dlna_diag(
        "INFO",
        format!(
            "Manual IP probe started: input='{}' parsed host='{}' port={:?} url={:?}",
            ip_or_url, host, explicit_port, explicit_url
        ),
    );

    if host.is_empty() {
        bail!("請輸入有效的 IP 位址、IP:Port 或 XML URL");
    }

    let client = build_dlna_http_client(timeout_ms.max(1500))?;
    let mut best_rejection: Option<String> = None;

    // 1. Direct full URL if provided
    if let Some(full_url) = explicit_url {
        match fetch_device_desc(&client, &full_url).await {
            Ok(dev) if dev.av_transport_url.starts_with("http") => return Ok(dev),
            Ok(dev) => {
                let msg = format!("設備 '{}' 回傳無效的 AVTransport URL", dev.name);
                push_dlna_diag("WARN", format!("Manual URL rejected ({}): {}", full_url, msg));
                best_rejection = Some(msg);
            }
            Err(e) => {
                let msg = e.to_string();
                push_dlna_diag("WARN", format!("Manual URL probe failed ({}): {}", full_url, msg));
                best_rejection = Some(msg);
            }
        }
        // If user entered a full URL that didn't yield an audio renderer (and explicit_port wasn't an HTTP-only miss),
        // fall through to check unicast SSDP and open-port description paths on that host/port.
    }

    let default_ports: [u16; 29] = [
        49152, 49153, 49154, 49155, 49156, 49157, 49158, 49159, 49160,
        49149, 49150, 49151,
        1858, 1772, 8080, 1400, 49494, 50002, 50001, 8200, 9000, 8008,
        80, 2869, 4000, 49000, 59152, 60000, 9790,
    ];
    let mut candidate_ports: Vec<u16> = Vec::with_capacity(30);
    if let Some(p) = explicit_port {
        candidate_ports.push(p);
    } else {
        candidate_ports.extend_from_slice(&default_ports);
    }

    // 2. Run unicast SSDP M-SEARCH and TCP open-port scan concurrently
    let host_ip_opt = resolve_probe_ipv4(&host).await;
    let (unicast_locs, open_ports) = tokio::join!(
        async {
            if let Some(ip_v4) = host_ip_opt {
                unicast_ssdp_locations(ip_v4, 850).await
            } else {
                Vec::new()
            }
        },
        discover_open_tcp_ports(&host, &candidate_ports, 450)
    );

    push_dlna_diag(
        "INFO",
        format!(
            "Manual IP pre-check for '{}': resolved_v4={:?} | unicast_locs={:?} | open_tcp_ports={:?}",
            host, host_ip_opt, unicast_locs, open_ports
        ),
    );

    for loc in &unicast_locs {
        match fetch_device_desc(&client, loc).await {
            Ok(dev) if dev.av_transport_url.starts_with("http") => return Ok(dev),
            Ok(dev) => {
                best_rejection.get_or_insert_with(|| format!("設備 '{}' 回傳無效的 AVTransport URL", dev.name));
            }
            Err(e) => {
                let msg = e.to_string();
                push_dlna_diag("WARN", format!("Unicast location {} rejected: {}", loc, msg));
                best_rejection.get_or_insert(msg);
            }
        }
    }

    if open_ports.is_empty() {
        let port_desc = if let Some(p) = explicit_port {
            format!("連接埠 {}", p)
        } else {
            format!("{} 個常見 UPnP 連接埠", candidate_ports.len())
        };
        let reason = best_rejection.unwrap_or_else(|| {
            format!(
                "無法建立 TCP 連線至 {}（{} 無回應，請確認 IP 正確、設備已開機且已開啟 UPnP/DLNA）",
                host, port_desc
            )
        });
        push_dlna_diag("WARN", format!("Manual IP probe failed for '{}': {}", ip_or_url, reason));
        bail!("{}", reason);
    }

    // 3. Probe common UPnP XML description paths ONLY on actually open TCP ports, returning on first hit
    let candidate_paths = [
        "description.xml",
        "devicedesc.xml",
        "device.xml",
        "rootDesc.xml",
        "upnp/desc.xml",
        "upnp/description.xml",
        "xml/device_description.xml",
        "dmr.xml",
        "desc.xml",
        "MediaRenderer/desc.xml",
        "dlna/Render/desc.xml",
        "DeviceDescription.xml",
        "",
    ];

    let mut join_set: tokio::task::JoinSet<Result<DlnaDevice>> = tokio::task::JoinSet::new();
    for port in &open_ports {
        for path in candidate_paths {
            let url = if path.is_empty() {
                format!("http://{}:{}/", host, port)
            } else {
                format!("http://{}:{}/{}", host, port, path)
            };
            if unicast_locs.iter().any(|u| u == &url) {
                continue;
            }
            let c = client.clone();
            join_set.spawn(async move {
                let dev = fetch_device_desc(&c, &url).await?;
                if dev.av_transport_url.starts_with("http") {
                    Ok(dev)
                } else {
                    bail!("Device '{}' at {} has invalid AVTransport URL", dev.name, url)
                }
            });
        }
    }

    while let Some(res) = join_set.join_next().await {
        match res {
            Ok(Ok(dev)) => {
                join_set.abort_all();
                return Ok(dev);
            }
            Ok(Err(e)) => {
                let msg = e.to_string();
                // Prefer preserving rich XML/AVTransport/exclusion errors over plain 404s
                let is_rich = msg.contains("AVTransport") || msg.contains("excluded");
                if is_rich || best_rejection.is_none() {
                    best_rejection = Some(msg);
                }
            }
            Err(_) => {}
        }
    }

    let detail = best_rejection.unwrap_or_else(|| {
        format!(
            "已連通連接埠 {:?}，但未找到包含 AVTransport 的 UPnP 描述檔",
            open_ports
        )
    });
    push_dlna_diag("WARN", format!("Manual IP probe exhausted open ports {:?} for '{}': {}", open_ports, ip_or_url, detail));
    bail!("在 {} 未找到可用的 UPnP/DLNA MediaRenderer（{}）", ip_or_url, detail);
}

pub async fn probe_device_ip(ip: &str) -> Result<DlnaDevice> {
    probe_device_ip_fast(ip, 2200).await
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

/// Extract all `<serviceType>` strings present in a Device Description XML for diagnostics.
fn list_xml_service_types(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = xml.to_lowercase();
    let mut cur = 0usize;
    while let Some(idx) = lower[cur..].find("<service") {
        let abs_start = cur + idx;
        let after_kw = abs_start + "<service".len();
        let next_ch = lower.as_bytes().get(after_kw).copied();
        if !matches!(next_ch, Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')) {
            cur = after_kw;
            continue;
        }
        let Some(end_rel) = lower[abs_start..].find("</service>") else { break };
        let abs_end = abs_start + end_rel + "</service>".len();
        if let Some(st) = extract_xml_value(&xml[abs_start..abs_end], "serviceType") {
            if !out.contains(&st) {
                out.push(st);
            }
        }
        cur = abs_end;
    }
    out
}

fn find_next_device_tag(lower: &str, from: usize) -> Option<usize> {
    let mut scan = from;
    while scan < lower.len() {
        let Some(rel) = lower[scan..].find("<device") else {
            return None;
        };
        let pos = scan + rel;
        let next = lower.as_bytes().get(pos + 7).copied();
        if matches!(next, Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')) {
            return Some(pos);
        }
        scan = pos + 7;
    }
    None
}

/// If the XML has nested `<deviceList><device>...</device></deviceList>` blocks, prefer the innermost
/// `<device>` block that actually represents a `MediaRenderer` (or hosts `AVTransport`), so metadata
/// fields like `friendlyName` and `UDN` aren't taken from an unrelated outer root/bridge device.
fn select_renderer_device_xml<'a>(xml: &'a str) -> &'a str {
    let lower = xml.to_lowercase();
    let mut best_mr: Option<&'a str> = None;
    let mut best_av: Option<&'a str> = None;
    let mut pos = 0usize;
    while let Some(start) = find_next_device_tag(&lower, pos) {
        let mut depth = 0i32;
        let mut scan = start;
        let mut block_end = None;
        while scan < lower.len() {
            let open = find_next_device_tag(&lower, scan);
            let close = lower[scan..].find("</device>").map(|p| scan + p);
            match (open, close) {
                (Some(o), Some(c)) if o < c => {
                    depth += 1;
                    scan = o + 7;
                }
                (_, Some(c)) => {
                    depth -= 1;
                    scan = c + "</device>".len();
                    if depth <= 0 {
                        block_end = Some(scan);
                        break;
                    }
                }
                _ => break,
            }
        }
        let Some(end) = block_end else { break };
        let block = &xml[start..end];
        let b_lower = &lower[start..end];
        if b_lower.contains("device:mediarenderer") && b_lower.contains("avtransport") {
            if best_mr.map_or(true, |cur| block.len() < cur.len()) {
                best_mr = Some(block);
            }
        } else if b_lower.contains("avtransport") {
            if best_av.map_or(true, |cur| block.len() < cur.len()) {
                best_av = Some(block);
            }
        }
        pos = start + 7;
    }
    best_mr.or(best_av).unwrap_or(xml)
}

async fn fetch_device_desc(client: &reqwest::Client, location: &str) -> Result<DlnaDevice> {
    let resp = client
        .get(location)
        .header(reqwest::header::ACCEPT, "text/xml, application/xml, */*")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP GET failed: {}", e))?;

    let status = resp.status();
    let xml = resp
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP {} body read failed: {}", status.as_u16(), e))?;

    if !status.is_success() {
        let preview: String = xml.chars().take(180).collect();
        bail!("HTTP {} (preview: {})", status.as_u16(), preview.replace(['\r', '\n'], " "));
    }

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

    let dev_xml = select_renderer_device_xml(&xml);
    let device_type = extract_xml_value(dev_xml, "deviceType").unwrap_or_default();
    let raw_name = extract_xml_value(dev_xml, "friendlyName")
        .or_else(|| extract_xml_value(&xml, "friendlyName"))
        .unwrap_or_else(|| "Unknown Renderer".to_string());
    let model = extract_xml_value(dev_xml, "modelName")
        .or_else(|| extract_xml_value(&xml, "modelName"))
        .unwrap_or_default();
    let manufacturer = extract_xml_value(dev_xml, "manufacturer")
        .or_else(|| extract_xml_value(&xml, "manufacturer"))
        .unwrap_or_default();
    let udn = extract_xml_value(dev_xml, "UDN")
        .or_else(|| extract_xml_value(&xml, "UDN"))
        .unwrap_or_else(|| location.to_string());
    let ip = loc_url.host_str().unwrap_or("").to_string();

    // Proactively exclude network equipment (routers, APs, WiFi nodes, gateways, printers)
    if let Some(reason) = non_audio_exclusion_reason(&xml, &raw_name, &model, &manufacturer) {
        push_dlna_diag(
            "WARN",
            format!(
                "Excluded '{}' (model='{}', mfr='{}', type='{}') at {}: {}",
                raw_name, model, manufacturer, device_type, location, reason
            ),
        );
        bail!("Device '{}' ({}) at {} excluded: {}", raw_name, model, location, reason);
    }

    let name = format_dlna_display_name(&raw_name, &model, &manufacturer);

    // Must have a valid AVTransport service to receive and play audio streams
    let (av_transport_url, av_transport_service_type) = match extract_service(dev_xml, "AVTransport", &base_url)
        .or_else(|| extract_service(&xml, "AVTransport", &base_url))
    {
        Some(v) => v,
        None => {
            let found_services = list_xml_service_types(&xml);
            push_dlna_diag(
                "WARN",
                format!(
                    "HTTP {} OK but missing AVTransport on '{}' (type='{}') at {} | services={:?}",
                    status.as_u16(),
                    name,
                    device_type,
                    location,
                    found_services
                ),
            );
            bail!(
                "Device '{}' (type='{}') at {} has no AVTransport service (found services: {:?})",
                name,
                device_type,
                location,
                found_services
            );
        }
    };

    let (rendering_control_url, rendering_control_service_type) = extract_service(dev_xml, "RenderingControl", &base_url)
        .or_else(|| extract_service(&xml, "RenderingControl", &base_url))
        .unwrap_or_else(|| {
            let fallback_url = resolve_url(&base_url, "/RenderingControl/control");
            (fallback_url, default_rendering_control_service_type())
        });

    push_dlna_diag(
        "INFO",
        format!(
            "ACCEPTED '{}' (model='{}', mfr='{}', ip={}) at {} | AVTransport={} | RenderingControl={}",
            name, model, manufacturer, ip, location, av_transport_url, rendering_control_url
        ),
    );

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

pub fn non_audio_exclusion_reason(xml: &str, name: &str, model: &str, manufacturer: &str) -> Option<String> {
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
            return Some(format!("non-audio deviceType '{}'", dt));
        }
    }

    // 2. Pure MediaServer without MediaRenderer (e.g. NAS or router USB drive share)
    if lower_xml.contains("device:mediaserver")
        && !lower_xml.contains("device:mediarenderer")
        && !lower_xml.contains("service:avtransport")
    {
        return Some("pure MediaServer without MediaRenderer or AVTransport".to_string());
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
            if !lower_xml.contains("device:mediarenderer")
                && !lower_xml.contains("audiospeaker")
                && !lower_xml.contains("service:avtransport")
            {
                return Some(format!("matched network keyword '{}'", kw));
            }
        }
    }

    None
}

pub fn is_network_or_non_audio_device(xml: &str, name: &str, model: &str, manufacturer: &str) -> bool {
    non_audio_exclusion_reason(xml, name, model, manufacturer).is_some()
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
        let after_kw = abs_start + "<service".len();
        let next_ch = lower_xml.as_bytes().get(after_kw).copied();
        // Ensure exact `<service>` or `<service ...>` tag (not `<serviceList>` or `<serviceId>`)
        if !matches!(next_ch, Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')) {
            cur = after_kw;
            continue;
        }
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
            let ctrl_clean = ctrl_url_rel.trim();
            if ctrl_clean.is_empty() {
                cur = abs_end;
                continue;
            }
            let service_type = extract_xml_value(block, "serviceType")
                .unwrap_or_else(|| format!("urn:schemas-upnp-org:service:{}:1", service_keyword));

            let resolved = resolve_url(base_url, ctrl_clean);
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
    fn test_extract_service_handles_tags_and_upmpdcli() {
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

        let base = reqwest::Url::parse("http://192.168.1.88:49152/description.xml").unwrap();
        let (av_url, av_st) = extract_service(xml, "AVTransport", &base).unwrap();
        assert_eq!(av_url, "http://192.168.1.88:49152/upnp/control/avtransport");
        assert_eq!(av_st, "urn:schemas-upnp-org:service:AVTransport:1");

        let (rc_url, rc_st) = extract_service(xml, "RenderingControl", &base).unwrap();
        assert_eq!(rc_url, "http://192.168.1.88:49152/upnp/control/rendering");
        assert_eq!(rc_st, "urn:schemas-upnp-org:service:RenderingControl:1");
    }

    #[test]
    fn test_nested_device_selection_and_probe_parser() {
        let nested_xml = r#"<root>
            <device>
                <deviceType>urn:schemas-upnp-org:device:Basic:1</deviceType>
                <friendlyName>Outer Bridge</friendlyName>
                <UDN>uuid:outer</UDN>
                <deviceList>
                    <device>
                        <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
                        <friendlyName>Holo Audio RED</friendlyName>
                        <UDN>uuid:holo-red</UDN>
                        <serviceList>
                            <service>
                                <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
                                <controlURL>/ctl/AVTransport</controlURL>
                            </service>
                        </serviceList>
                    </device>
                </deviceList>
            </device>
        </root>"#;
        let selected = select_renderer_device_xml(nested_xml);
        assert_eq!(extract_xml_value(selected, "friendlyName").as_deref(), Some("Holo Audio RED"));
        assert_eq!(extract_xml_value(selected, "UDN").as_deref(), Some("uuid:holo-red"));

        assert_eq!(parse_probe_target("192.168.50.88"), ("192.168.50.88".to_string(), None, None));
        assert_eq!(parse_probe_target("192.168.50.88:49152"), ("192.168.50.88".to_string(), Some(49152), None));
        assert_eq!(parse_probe_target(" 192.168.50.88 ： 49152 "), ("192.168.50.88".to_string(), Some(49152), None));
        assert_eq!(
            parse_probe_target("192.168.50.88:49152/description.xml"),
            ("192.168.50.88".to_string(), Some(49152), Some("http://192.168.50.88:49152/description.xml".to_string()))
        );
        assert_eq!(
            parse_probe_target("http://192.168.50.88:49152/description.xml"),
            ("192.168.50.88".to_string(), Some(49152), Some("http://192.168.50.88:49152/description.xml".to_string()))
        );
        assert_eq!(parse_probe_target("holored.local"), ("holored.local".to_string(), None, None));
    }

    #[tokio::test]
    async fn test_manual_probe_fast_tcp_and_xml() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::spawn(async move {
            let xml = r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
    <friendlyName>Holo Audio RED</friendlyName>
    <manufacturer>Holo Audio</manufacturer>
    <modelName>RED</modelName>
    <UDN>uuid:holo-red-test</UDN>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
        <controlURL>/upnp/control/AVTransport</controlURL>
      </service>
    </serviceList>
  </device>
</root>"#;
            for _ in 0..20 {
                let Ok((mut stream, _)) = listener.accept().await else { break };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if req.starts_with("GET /description.xml") {
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            xml.len(),
                            xml
                        );
                        let _ = stream.write_all(resp.as_bytes()).await;
                    } else if req.starts_with("GET ") {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                    }
                });
            }
        });

        let target = format!("127.0.0.1:{}", port);
        let dev = probe_device_ip_fast(&target, 1500).await.unwrap();
        assert_eq!(dev.name, "Holo Audio RED");
        assert_eq!(dev.av_transport_url, format!("http://127.0.0.1:{}/upnp/control/AVTransport", port));
        drop(server);

        // Closed port should fail fast without hanging
        let closed_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_port = closed_listener.local_addr().unwrap().port();
        drop(closed_listener);
        let t0 = std::time::Instant::now();
        let err = probe_device_ip_fast(&format!("127.0.0.1:{}", closed_port), 1500)
            .await
            .unwrap_err();
        assert!(t0.elapsed() < Duration::from_secs(2));
        assert!(err.to_string().contains("無法建立 TCP 連線"));
    }

    #[test]
    fn test_is_network_or_non_audio_device() {
        let router_xml = r#"<device>
            <deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType>
            <friendlyName>RT-AX86U Wireless Router</friendlyName>
        </device>"#;
        assert!(is_network_or_non_audio_device(router_xml, "RT-AX86U", "RT-AX86U", "ASUSTeK"));

        let node_xml = r#"<device>
            <deviceType>urn:schemas-upnp-org:device:Basic:1</deviceType>
            <friendlyName>Deco X60 Wi-Fi Node</friendlyName>
        </device>"#;
        assert!(is_network_or_non_audio_device(node_xml, "Deco X60 Wi-Fi Node", "Deco X60", "TP-Link"));

        let renderer_xml = r#"<device>
            <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
            <friendlyName>Holo Audio RED</friendlyName>
        </device>"#;
        assert!(!is_network_or_non_audio_device(renderer_xml, "Holo Audio RED", "RED", "Holo Audio"));
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

    #[test]
    fn test_list_local_ipv4_interfaces() {
        let ifaces = list_local_ipv4_interfaces();
        println!("LOCAL_IFACES={:?}", ifaces);
        assert!(!ifaces.is_empty());
    }

    #[tokio::test]
    async fn test_live_discover() {
        let devs = discover_dlna_devices(3).await.unwrap_or_default();
        for d in &devs {
            println!("DEVICE: name={}, ip={}, model={}, loc={}", d.name, d.ip, d.model, d.location);
        }
        for log in get_dlna_diag_logs() {
            println!("{}", log);
        }
    }
}
