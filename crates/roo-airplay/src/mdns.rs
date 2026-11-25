//! Pure-Rust RFC 6762 mDNS discovery for AirPlay / RAOP audio receivers
//! (`_raop._tcp.local.` and `_airplay._tcp.local.`).
//! Uses unicast-response (QU) queries from an ephemeral UDP socket so it works
//! reliably on Windows without requiring Bonjour or exclusive port 5353 binding.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirPlayDevice {
    pub id: String,
    pub name: String,
    pub model: String,
    pub ip: String,
    pub port: u16,
    pub protocol: String,
    #[serde(rename = "supportsUnencrypted")]
    pub supports_unencrypted: bool,
    #[serde(rename = "requiresPassword")]
    pub requires_password: bool,
}

/// Build a DNS query packet for PTR records with the mDNS QU (unicast response) bit set.
fn build_mdns_ptr_query(service_names: &[&str]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    // Header: ID=0, Flags=0 (standard query), QDCOUNT=len, ANCOUNT=0, NSCOUNT=0, ARCOUNT=0
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&(service_names.len() as u16).to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());

    for svc in service_names {
        for label in svc.trim_end_matches('.').split('.') {
            if label.is_empty() || label.len() > 63 {
                continue;
            }
            buf.push(label.len() as u8);
            buf.extend_from_slice(label.as_bytes());
        }
        buf.push(0); // root label
        buf.extend_from_slice(&12u16.to_be_bytes()); // QTYPE = PTR (12)
        buf.extend_from_slice(&0x8001u16.to_be_bytes()); // QCLASS = IN (1) | QU bit (0x8000)
    }
    buf
}

/// Read a DNS domain name with RFC 1035 pointer compression (`0xC0`).
fn read_dns_name(packet: &[u8], mut offset: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut jumped = false;
    let mut next_offset = offset;
    let mut hops = 0;

    while offset < packet.len() && hops < 32 {
        let len = packet[offset];
        if len == 0 {
            if !jumped {
                next_offset = offset + 1;
            }
            return Some((labels.join("."), next_offset));
        }
        if (len & 0xC0) == 0xC0 {
            if offset + 1 >= packet.len() {
                return None;
            }
            let ptr = (((len & 0x3F) as usize) << 8) | (packet[offset + 1] as usize);
            if !jumped {
                next_offset = offset + 2;
                jumped = true;
            }
            offset = ptr;
            hops += 1;
        } else {
            let l = len as usize;
            offset += 1;
            if offset + l > packet.len() {
                return None;
            }
            let s = String::from_utf8_lossy(&packet[offset..offset + l]).to_string();
            labels.push(s);
            offset += l;
        }
    }
    None
}

fn parse_txt_rdata(rdata: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    while i < rdata.len() {
        let len = rdata[i] as usize;
        i += 1;
        if i + len > rdata.len() {
            break;
        }
        let entry = String::from_utf8_lossy(&rdata[i..i + len]);
        if let Some((k, v)) = entry.split_once('=') {
            map.insert(k.trim().to_lowercase(), v.trim().to_string());
        } else if !entry.trim().is_empty() {
            map.insert(entry.trim().to_lowercase(), "true".to_string());
        }
        i += len;
    }
    map
}

#[derive(Default)]
struct MdnsAccum {
    /// instance_lower -> (full_instance_name, host_lower, port)
    srv: HashMap<String, (String, String, u16)>,
    /// instance_lower -> txt map
    txt: HashMap<String, HashMap<String, String>>,
    /// host_lower -> ipv4
    a_records: HashMap<String, Ipv4Addr>,
}

fn ingest_mdns_packet(pkt: &[u8], acc: &mut MdnsAccum) {
    if pkt.len() < 12 {
        return;
    }
    let qdcount = u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
    let ancount = u16::from_be_bytes([pkt[6], pkt[7]]) as usize;
    let nscount = u16::from_be_bytes([pkt[8], pkt[9]]) as usize;
    let arcount = u16::from_be_bytes([pkt[10], pkt[11]]) as usize;

    let mut off = 12usize;
    for _ in 0..qdcount {
        let Some((_, next)) = read_dns_name(pkt, off) else { return };
        off = next + 4;
        if off > pkt.len() {
            return;
        }
    }

    let total_rr = ancount + nscount + arcount;
    for _ in 0..total_rr {
        let Some((name, next)) = read_dns_name(pkt, off) else { break };
        off = next;
        if off + 10 > pkt.len() {
            break;
        }
        let rtype = u16::from_be_bytes([pkt[off], pkt[off + 1]]);
        let rdlen = u16::from_be_bytes([pkt[off + 8], pkt[off + 9]]) as usize;
        off += 10;
        if off + rdlen > pkt.len() {
            break;
        }
        let rdata = &pkt[off..off + rdlen];
        let name_lower = name.to_lowercase();

        match rtype {
            // A record (1)
            1 if rdlen == 4 => {
                let ip = Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]);
                if !ip.is_loopback() && !ip.is_unspecified() {
                    acc.a_records.insert(name_lower, ip);
                }
            }
            // TXT record (16)
            16 => {
                let kv = parse_txt_rdata(rdata);
                acc.txt.entry(name_lower).or_default().extend(kv);
            }
            // SRV record (33)
            33 if rdlen >= 6 => {
                let port = u16::from_be_bytes([rdata[4], rdata[5]]);
                if let Some((target, _)) = read_dns_name(pkt, off + 6) {
                    acc.srv.insert(name_lower, (name, target.to_lowercase(), port));
                }
            }
            _ => {}
        }
        off += rdlen;
    }
}

/// Extract friendly name from a RAOP instance like `AABBCCDDEEFF@Living Room._raop._tcp.local`.
pub fn parse_raop_friendly_name(instance_name: &str) -> String {
    let base = instance_name
        .split("._raop._tcp")
        .next()
        .unwrap_or(instance_name)
        .split("._airplay._tcp")
        .next()
        .unwrap_or(instance_name);
    if let Some((_, friendly)) = base.split_once('@') {
        let trimmed = friendly.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    base.trim().to_string()
}

/// Discover AirPlay / RAOP audio receivers on the local LAN via mDNS.
pub fn discover_airplay_devices(timeout_ms: u64) -> Vec<AirPlayDevice> {
    let Ok(sock) = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)) else {
        return Vec::new();
    };
    let _ = sock.set_multicast_ttl_v4(255);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(120)));

    let query = build_mdns_ptr_query(&["_raop._tcp.local", "_airplay._tcp.local"]);
    let dest = SocketAddr::V4(SocketAddrV4::new(MDNS_ADDR, MDNS_PORT));

    let _ = sock.send_to(&query, dest);

    let mut acc = MdnsAccum::default();
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(400));
    let mut resent = false;
    let mut buf = [0u8; 4096];

    while Instant::now() < deadline {
        if !resent && deadline.saturating_duration_since(Instant::now()) < Duration::from_millis(timeout_ms / 2) {
            let _ = sock.send_to(&query, dest);
            resent = true;
        }
        match sock.recv_from(&mut buf) {
            Ok((n, src)) => {
                let prev_a_len = acc.a_records.len();
                ingest_mdns_packet(&buf[..n], &mut acc);
                // If an SRV arrived without an A record, remember responder's IPv4 as fallback
                if let SocketAddr::V4(v4) = src {
                    if acc.a_records.len() == prev_a_len {
                        let src_ip = *v4.ip();
                        if !src_ip.is_loopback() && !src_ip.is_unspecified() {
                            for (_, (_, host_lower, _)) in acc.srv.iter() {
                                acc.a_records.entry(host_lower.clone()).or_insert(src_ip);
                            }
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }

    // Build deduplicated AirPlayDevice list, preferring `_raop._tcp` (audio streaming port)
    let mut by_ip: HashMap<String, AirPlayDevice> = HashMap::new();

    for (inst_lower, (orig_name, host_lower, port)) in &acc.srv {
        let is_raop = inst_lower.contains("._raop._tcp");
        let is_airplay = inst_lower.contains("._airplay._tcp");
        if !is_raop && !is_airplay {
            continue;
        }
        let Some(ip) = acc.a_records.get(host_lower) else {
            continue;
        };
        let ip_str = ip.to_string();
        let txt = acc.txt.get(inst_lower);

        let friendly = parse_raop_friendly_name(orig_name);
        let model = txt
            .and_then(|t| t.get("am").or_else(|| t.get("model")).cloned())
            .unwrap_or_else(|| "AirPlay Audio Receiver".to_string());

        // `et` (encryption types): 0 = no encryption, 1 = RSA (AirPlay 1), 3 = FairPlay, 4 = MFiSAP
        let et_str = txt.and_then(|t| t.get("et")).map(|s| s.as_str()).unwrap_or("0");
        let supports_unencrypted = et_str.split(',').any(|tok| tok.trim() == "0");
        let requires_password = txt
            .and_then(|t| t.get("pw"))
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let dev = AirPlayDevice {
            id: format!("airplay:{}:{}", ip_str, port),
            name: friendly,
            model,
            ip: ip_str.clone(),
            port: *port,
            protocol: "airplay".to_string(),
            supports_unencrypted,
            requires_password,
        };

        // Prefer `_raop._tcp` entry over `_airplay._tcp` for the same IP
        match by_ip.get(&ip_str) {
            Some(_) if !is_raop => {}
            _ => {
                by_ip.insert(ip_str, dev);
            }
        }
    }

    let mut list: Vec<AirPlayDevice> = by_ip.into_values().collect();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_raop_friendly_name() {
        assert_eq!(
            parse_raop_friendly_name("A45E60C12345@EVERSOLO T8._raop._tcp.local"),
            "EVERSOLO T8"
        );
        assert_eq!(
            parse_raop_friendly_name("Living Room HomePod._airplay._tcp.local"),
            "Living Room HomePod"
        );
    }

    #[test]
    fn test_build_mdns_ptr_query_valid_header() {
        let q = build_mdns_ptr_query(&["_raop._tcp.local"]);
        assert!(q.len() > 20);
        // QDCOUNT = 1
        assert_eq!(u16::from_be_bytes([q[4], q[5]]), 1);
        // Ends with QTYPE=PTR(12) and QCLASS=0x8001 (IN | QU)
        let tail = &q[q.len() - 4..];
        assert_eq!(tail, &[0x00, 0x0C, 0x80, 0x01]);
    }
}
