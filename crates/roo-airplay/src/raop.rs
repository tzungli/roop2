//! RAOP (AirPlay 1 Audio) RTSP + RTP/UDP streaming session.
//! Implements:
//! - RTSP handshake: OPTIONS, ANNOUNCE (SDP AppleLossless), SETUP, RECORD,
//!   SET_PARAMETER (volume, DAAP metadata, progress), FLUSH, TEARDOWN.
//! - UDP RTP Audio pacer (352 frames / 7.98ms monotonic clock, 256-packet retransmit ring).
//! - UDP Control socket: periodic RTP Sync (0x54) & retransmit responder (0x55 -> 0x56).
//! - UDP Timing socket: NTP timing responder (0x52 -> 0x53) for tight receiver clock lock.

use crate::alac::{encode_alac_verbatim_stereo, RAOP_FRAMES_PER_PACKET, RAOP_SAMPLE_RATE};
use anyhow::{bail, Context, Result};
use rand::Rng;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const NTP_EPOCH_OFFSET: u64 = 2_208_988_800; // Seconds from 1900-01-01 to 1970-01-01
const RAOP_LATENCY_FRAMES: u32 = 88200; // 2.0s standard AirPlay buffer latency

fn ntp_now() -> (u32, u32) {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = (dur.as_secs() + NTP_EPOCH_OFFSET) as u32;
    let frac = (((dur.subsec_nanos() as u64) << 32) / 1_000_000_000) as u32;
    (secs, frac)
}

/// Convert 0..=100 linear slider to AirPlay dB scale (`-144.0` mute, `-30.0..=0.0` active).
pub fn volume_pct_to_airplay_db(pct: u32) -> f64 {
    if pct == 0 {
        -144.0
    } else {
        let clamped = pct.min(100) as f64;
        -30.0 + (clamped / 100.0) * 30.0
    }
}

/// Build DAAP `mlit` metadata blob for AirPlay `SET_PARAMETER` (`application/x-dmap-tagged`).
fn build_dmap_metadata(title: &str, artist: &str, album: &str) -> Vec<u8> {
    let mut inner = Vec::new();
    let push_tag = |buf: &mut Vec<u8>, tag: &[u8; 4], val: &str| {
        if val.is_empty() {
            return;
        }
        let b = val.as_bytes();
        buf.extend_from_slice(tag);
        buf.extend_from_slice(&(b.len() as u32).to_be_bytes());
        buf.extend_from_slice(b);
    };
    push_tag(&mut inner, b"minm", title);
    push_tag(&mut inner, b"asar", artist);
    push_tag(&mut inner, b"asal", album);

    let mut out = Vec::with_capacity(inner.len() + 8);
    out.extend_from_slice(b"mlit");
    out.extend_from_slice(&(inner.len() as u32).to_be_bytes());
    out.extend_from_slice(&inner);
    out
}

pub struct RtspClient {
    stream: TcpStream,
    cseq: u32,
    dacp_id: String,
    active_remote: String,
    client_instance: String,
    session_id: Option<String>,
    rtsp_uri: String,
}

impl RtspClient {
    pub fn connect(ip: &str, port: u16) -> Result<(Self, Ipv4Addr)> {
        let target: SocketAddr = format!("{}:{}", ip, port)
            .parse()
            .with_context(|| format!("Invalid AirPlay address {}:{}", ip, port))?;
        let stream = TcpStream::connect_timeout(&target, Duration::from_secs(5))
            .with_context(|| format!("Failed to connect AirPlay RTSP {}:{}", ip, port))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_nodelay(true);

        let local_ip = match stream.local_addr()?.ip() {
            IpAddr::V4(v4) => v4,
            _ => Ipv4Addr::LOCALHOST,
        };

        let mut rng = rand::thread_rng();
        let sess_num: u32 = rng.gen_range(100_000_000..999_999_999);
        let dacp_id = format!("{:016X}", rng.gen::<u64>());
        let active_remote = format!("{}", rng.gen_range(100_000_000u32..u32::MAX));
        let client_instance = dacp_id.clone();
        let rtsp_uri = format!("rtsp://{}/{}", local_ip, sess_num);

        Ok((
            Self {
                stream,
                cseq: 0,
                dacp_id,
                active_remote,
                client_instance,
                session_id: None,
                rtsp_uri,
            },
            local_ip,
        ))
    }

    pub fn request(
        &mut self,
        method: &str,
        uri: &str,
        extra_headers: &[(&str, &str)],
        body: Option<(&str, &[u8])>,
    ) -> Result<(u16, HashMap<String, String>)> {
        self.cseq += 1;
        let mut req = format!(
            "{} {} RTSP/1.0\r\nCSeq: {}\r\nUser-Agent: iTunes/7.6.2 (Windows; N;)\r\nClient-Instance: {}\r\nDACP-ID: {}\r\nActive-Remote: {}\r\n",
            method, uri, self.cseq, self.client_instance, self.dacp_id, self.active_remote
        );
        if let Some(ref sid) = self.session_id {
            req.push_str(&format!("Session: {}\r\n", sid));
        }
        for (k, v) in extra_headers {
            req.push_str(&format!("{}: {}\r\n", k, v));
        }
        if let Some((ctype, bytes)) = body {
            req.push_str(&format!("Content-Type: {}\r\nContent-Length: {}\r\n", ctype, bytes.len()));
        }
        req.push_str("\r\n");

        self.stream.write_all(req.as_bytes())?;
        if let Some((_, bytes)) = body {
            self.stream.write_all(bytes)?;
        }
        self.stream.flush()?;

        // Read RTSP response headers
        let mut reader = BufReader::new(&self.stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line)?;
        let status_code: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let mut headers = HashMap::new();
        let mut content_len = 0usize;
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line)?;
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                let kl = k.trim().to_lowercase();
                let vv = v.trim().to_string();
                if kl == "content-length" {
                    content_len = vv.parse().unwrap_or(0);
                }
                headers.insert(kl, vv);
            }
        }
        if content_len > 0 && content_len < 1024 * 1024 {
            let mut discard = vec![0u8; content_len];
            let _ = reader.read_exact(&mut discard);
        }

        if let Some(sid) = headers.get("session") {
            let clean = sid.split(';').next().unwrap_or(sid).trim().to_string();
            if !clean.is_empty() {
                self.session_id = Some(clean);
            }
        }

        Ok((status_code, headers))
    }

    pub fn set_volume(&mut self, pct: u32) -> Result<bool> {
        let db = volume_pct_to_airplay_db(pct);
        let body = format!("volume: {:.6}\r\n", db);
        let uri = self.rtsp_uri.clone();
        let (status, _) = self.request(
            "SET_PARAMETER",
            &uri,
            &[],
            Some(("text/parameters", body.as_bytes())),
        )?;
        Ok(status < 400)
    }

    pub fn set_metadata(&mut self, rtptime: u32, title: &str, artist: &str, album: &str) -> Result<()> {
        let blob = build_dmap_metadata(title, artist, album);
        let uri = self.rtsp_uri.clone();
        let rtp_info = format!("rtptime={}", rtptime);
        let _ = self.request(
            "SET_PARAMETER",
            &uri,
            &[("RTP-Info", &rtp_info)],
            Some(("application/x-dmap-tagged", &blob)),
        )?;
        Ok(())
    }

    pub fn set_progress(&mut self, start_rtp: u32, cur_rtp: u32, end_rtp: u32) -> Result<()> {
        let body = format!("progress: {}/{}/{}\r\n", start_rtp, cur_rtp, end_rtp);
        let uri = self.rtsp_uri.clone();
        let _ = self.request(
            "SET_PARAMETER",
            &uri,
            &[],
            Some(("text/parameters", body.as_bytes())),
        )?;
        Ok(())
    }

    pub fn flush(&mut self, seq: u16, rtptime: u32) -> Result<()> {
        let uri = self.rtsp_uri.clone();
        let rtp_info = format!("seq={};rtptime={}", seq, rtptime);
        let _ = self.request("FLUSH", &uri, &[("RTP-Info", &rtp_info)], None)?;
        Ok(())
    }

    pub fn teardown(&mut self) {
        let uri = self.rtsp_uri.clone();
        let _ = self.request("TEARDOWN", &uri, &[], None);
    }
}

fn parse_transport_port(transport: &str, key: &str) -> Option<u16> {
    for part in transport.split(';') {
        if let Some((k, v)) = part.trim().split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                return v.trim().parse().ok();
            }
        }
    }
    None
}

pub struct RaopControlHandle {
    pub rtsp: Arc<Mutex<RtspClient>>,
    pub is_stopped: Arc<AtomicBool>,
    pub is_paused: Arc<AtomicBool>,
    pub seek_target_frame: Arc<AtomicU64>,
    pub seek_requested: Arc<AtomicBool>,
    pub current_seq: Arc<AtomicU32>,
    pub current_rtptime: Arc<AtomicU32>,
    pub volume_verified: Arc<AtomicBool>,
    pub volume_supported: Arc<AtomicBool>,
}

/// Start an active RAOP streaming session to `(ip, port)` for the provided 44.1kHz stereo `i16` PCM buffer.
pub fn start_raop_stream(
    ip: String,
    port: u16,
    left_i16: Arc<Vec<i16>>,
    right_i16: Arc<Vec<i16>>,
    start_sec: f64,
    initial_volume_pct: u32,
    title: String,
    artist: String,
    album: String,
    on_progress: Arc<dyn Fn(f64, f64, &str) + Send + Sync>,
    on_ended: Arc<dyn Fn() + Send + Sync>,
) -> Result<Arc<RaopControlHandle>> {
    let (mut rtsp, local_ip) = RtspClient::connect(&ip, port)?;

    // Bind local UDP sockets for audio, control (sync/retransmit), and timing (NTP)
    let audio_sock = UdpSocket::bind(SocketAddr::new(IpAddr::V4(local_ip), 0))?;
    let control_sock = UdpSocket::bind(SocketAddr::new(IpAddr::V4(local_ip), 0))?;
    let timing_sock = UdpSocket::bind(SocketAddr::new(IpAddr::V4(local_ip), 0))?;
    let _ = control_sock.set_read_timeout(Some(Duration::from_millis(80)));
    let _ = timing_sock.set_read_timeout(Some(Duration::from_millis(80)));

    let local_ctrl_port = control_sock.local_addr()?.port();
    let local_timing_port = timing_sock.local_addr()?.port();

    // 1. OPTIONS
    let (opt_code, _) = rtsp.request("OPTIONS", "*", &[], None)?;
    if opt_code >= 400 {
        bail!("AirPlay OPTIONS failed with RTSP {}", opt_code);
    }

    // 2. ANNOUNCE (SDP for lossless 44.1kHz / 16-bit ALAC)
    let sdp = format!(
        "v=0\r\n\
         o=iTunes {} 0 IN IP4 {}\r\n\
         s=rooPlayer\r\n\
         c=IN IP4 {}\r\n\
         t=0 0\r\n\
         m=audio 0 RTP/AVP 96\r\n\
         a=rtpmap:96 AppleLossless\r\n\
         a=fmtp:96 352 0 16 40 10 14 2 255 0 0 44100\r\n",
        rtsp.rtsp_uri.rsplit('/').next().unwrap_or("1"),
        local_ip,
        ip
    );
    let uri = rtsp.rtsp_uri.clone();
    let (ann_code, _) = rtsp.request("ANNOUNCE", &uri, &[], Some(("application/sdp", sdp.as_bytes())))?;
    if ann_code == 401 || ann_code == 403 {
        bail!("此 AirPlay 裝置要求配對密碼或 AirPlay 2 HomeKit 加密認證 (RTSP {})", ann_code);
    }
    if ann_code >= 400 {
        bail!("AirPlay ANNOUNCE failed with RTSP {}", ann_code);
    }

    // 3. SETUP
    let transport_req = format!(
        "RTP/AVP/UDP;unicast;interleaved=0-1;mode=record;control_port={};timing_port={}",
        local_ctrl_port, local_timing_port
    );
    let (setup_code, setup_hdrs) = rtsp.request("SETUP", &uri, &[("Transport", &transport_req)], None)?;
    if setup_code >= 400 {
        bail!("AirPlay SETUP failed with RTSP {}", setup_code);
    }
    let resp_transport = setup_hdrs.get("transport").cloned().unwrap_or_default();
    let server_audio_port = parse_transport_port(&resp_transport, "server_port")
        .ok_or_else(|| anyhow::anyhow!("AirPlay SETUP missing server_port in '{}'", resp_transport))?;
    let server_ctrl_port = parse_transport_port(&resp_transport, "control_port").unwrap_or(0);

    let mut rng = rand::thread_rng();
    let init_seq: u16 = rng.gen_range(1000..50000);
    let init_rtptime: u32 = rng.gen_range(100_000..900_000_000);
    let ssrc: u32 = rng.gen();

    // 4. RECORD
    let rtp_info = format!("seq={};rtptime={}", init_seq, init_rtptime);
    let (rec_code, _) = rtsp.request(
        "RECORD",
        &uri,
        &[("Range", "npt=0-"), ("RTP-Info", &rtp_info)],
        None,
    )?;
    if rec_code >= 400 {
        bail!("AirPlay RECORD failed with RTSP {}", rec_code);
    }

    // 5. Apply initial volume & track metadata
    let (vol_verified, vol_supported) = match rtsp.set_volume(initial_volume_pct) {
        Ok(ok) => (true, ok),
        Err(_) => (false, false),
    };
    let total_frames = left_i16.len().min(right_i16.len());
    let duration_sec = total_frames as f64 / RAOP_SAMPLE_RATE as f64;
    let _ = rtsp.set_metadata(init_rtptime, &title, &artist, &album);
    let _ = rtsp.set_progress(init_rtptime, init_rtptime, init_rtptime.wrapping_add(total_frames as u32));

    let is_stopped = Arc::new(AtomicBool::new(false));
    let is_paused = Arc::new(AtomicBool::new(false));
    let seek_target_frame = Arc::new(AtomicU64::new(0));
    let seek_requested = Arc::new(AtomicBool::new(false));
    let current_seq = Arc::new(AtomicU32::new(init_seq as u32));
    let current_rtptime = Arc::new(AtomicU32::new(init_rtptime));
    let volume_verified = Arc::new(AtomicBool::new(vol_verified));
    let volume_supported = Arc::new(AtomicBool::new(vol_supported));
    let rtsp_arc = Arc::new(Mutex::new(rtsp));

    let handle = Arc::new(RaopControlHandle {
        rtsp: rtsp_arc.clone(),
        is_stopped: is_stopped.clone(),
        is_paused: is_paused.clone(),
        seek_target_frame: seek_target_frame.clone(),
        seek_requested: seek_requested.clone(),
        current_seq: current_seq.clone(),
        current_rtptime: current_rtptime.clone(),
        volume_verified: volume_verified.clone(),
        volume_supported: volume_supported.clone(),
    });

    // Shared 256-slot packet history ring for RTP retransmit requests
    let retransmit_ring: Arc<Mutex<Vec<Option<(u16, Vec<u8>)>>>> =
        Arc::new(Mutex::new(vec![None; 256]));

    // Timing thread: answer NTP timing queries (0x52 -> 0x53)
    {
        let stop_flag = is_stopped.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            while !stop_flag.load(Ordering::Relaxed) {
                if let Ok((n, src)) = timing_sock.recv_from(&mut buf) {
                    if n >= 32 && (buf[1] & 0x7F) == 0x52 {
                        let (recv_s, recv_f) = ntp_now();
                        let mut reply = [0u8; 32];
                        reply[0] = 0x80;
                        reply[1] = 0xD3; // Timing response (0x53 | marker 0x80)
                        reply[2] = 0x00;
                        reply[3] = 0x07;
                        // Copy client's transmit timestamp (bytes 24..32) to reference timestamp (bytes 8..16)
                        reply[8..16].copy_from_slice(&buf[24..32]);
                        reply[16..20].copy_from_slice(&recv_s.to_be_bytes());
                        reply[20..24].copy_from_slice(&recv_f.to_be_bytes());
                        let (tx_s, tx_f) = ntp_now();
                        reply[24..28].copy_from_slice(&tx_s.to_be_bytes());
                        reply[28..32].copy_from_slice(&tx_f.to_be_bytes());
                        let _ = timing_sock.send_to(&reply, src);
                    }
                }
            }
        });
    }

    // Control thread: answer RTP retransmit requests (0x55 -> 0x56)
    let ctrl_sock_for_sync = control_sock.try_clone()?;
    {
        let stop_flag = is_stopped.clone();
        let ring = retransmit_ring.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            while !stop_flag.load(Ordering::Relaxed) {
                if let Ok((n, src)) = control_sock.recv_from(&mut buf) {
                    if n >= 8 && (buf[1] & 0x7F) == 0x55 {
                        let first_seq = u16::from_be_bytes([buf[4], buf[5]]);
                        let count = u16::from_be_bytes([buf[6], buf[7]]).min(64);
                        let guard = ring.lock().unwrap();
                        for i in 0..count {
                            let req_seq = first_seq.wrapping_add(i);
                            if let Some((stored_seq, ref orig_pkt)) = guard[(req_seq as usize) & 0xFF] {
                                if stored_seq == req_seq {
                                    let mut resend = Vec::with_capacity(orig_pkt.len() + 4);
                                    resend.push(0x80);
                                    resend.push(0xD6); // Retransmit response (0x56 | marker)
                                    resend.extend_from_slice(&req_seq.to_be_bytes());
                                    resend.extend_from_slice(orig_pkt);
                                    let _ = control_sock.send_to(&resend, src);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    // Audio pacing thread: monotonic 352-frame ALAC RTP transmitter
    let audio_dest: SocketAddr = format!("{}:{}", ip, server_audio_port).parse()?;
    let ctrl_dest: Option<SocketAddr> = if server_ctrl_port > 0 {
        format!("{}:{}", ip, server_ctrl_port).parse().ok()
    } else {
        None
    };

    std::thread::spawn(move || {
        let mut frame_pos = ((start_sec.max(0.0) * RAOP_SAMPLE_RATE as f64).round() as usize)
            .min(total_frames);
        let mut seq = init_seq;
        let mut rtptime = init_rtptime;
        let mut first_packet = true;
        let mut packets_sent = 0u64;
        let mut pace_origin = Instant::now();
        let mut packets_since_origin = 0u64;
        let mut last_progress_emit = Instant::now() - Duration::from_secs(1);

        let send_sync = |is_first: bool, cur_rtp: u32| {
            if let Some(cdest) = ctrl_dest {
                let (ntp_s, ntp_f) = ntp_now();
                let mut sync_pkt = [0u8; 20];
                sync_pkt[0] = if is_first { 0x90 } else { 0x80 };
                sync_pkt[1] = 0xD4; // Sync (0x54 | marker)
                sync_pkt[2] = 0x00;
                sync_pkt[3] = 0x07;
                let rtp_minus_lat = cur_rtp.wrapping_sub(RAOP_LATENCY_FRAMES);
                sync_pkt[4..8].copy_from_slice(&rtp_minus_lat.to_be_bytes());
                sync_pkt[8..12].copy_from_slice(&ntp_s.to_be_bytes());
                sync_pkt[12..16].copy_from_slice(&ntp_f.to_be_bytes());
                sync_pkt[16..20].copy_from_slice(&cur_rtp.to_be_bytes());
                let _ = ctrl_sock_for_sync.send_to(&sync_pkt, cdest);
            }
        };

        while !is_stopped.load(Ordering::Relaxed) {
            if seek_requested.swap(false, Ordering::AcqRel) {
                let target = (seek_target_frame.load(Ordering::Acquire) as usize).min(total_frames);
                frame_pos = target;
                if let Ok(mut r) = rtsp_arc.lock() {
                    let _ = r.flush(seq, rtptime);
                }
                first_packet = true;
                pace_origin = Instant::now();
                packets_since_origin = 0;
            }

            if is_paused.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(40));
                pace_origin = Instant::now();
                packets_since_origin = 0;
                continue;
            }

            if frame_pos >= total_frames {
                // Allow the 2-second hardware buffer to drain cleanly before firing on_ended
                let drain_end = Instant::now() + Duration::from_millis(1600);
                while Instant::now() < drain_end && !is_stopped.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                if !is_stopped.load(Ordering::Relaxed) {
                    on_progress(duration_sec, duration_sec, "STOPPED");
                    on_ended();
                }
                break;
            }

            // Send RTP Sync before first packet and every ~1 second (125 packets)
            if first_packet || packets_sent % 125 == 0 {
                send_sync(first_packet, rtptime);
            }

            let end_frame = (frame_pos + RAOP_FRAMES_PER_PACKET).min(total_frames);
            let payload = encode_alac_verbatim_stereo(&left_i16[frame_pos..end_frame], &right_i16[frame_pos..end_frame]);

            let mut rtp = Vec::with_capacity(12 + payload.len());
            rtp.push(0x80);
            rtp.push(if first_packet { 0xE0 } else { 0x60 }); // PT=96 (0x60), marker bit on first pkt
            rtp.extend_from_slice(&seq.to_be_bytes());
            rtp.extend_from_slice(&rtptime.to_be_bytes());
            rtp.extend_from_slice(&ssrc.to_be_bytes());
            rtp.extend_from_slice(&payload);

            let _ = audio_sock.send_to(&rtp, audio_dest);

            {
                let mut ring = retransmit_ring.lock().unwrap();
                ring[(seq as usize) & 0xFF] = Some((seq, rtp));
            }

            let frames_this_pkt = (end_frame - frame_pos) as u32;
            frame_pos = end_frame;
            seq = seq.wrapping_add(1);
            rtptime = rtptime.wrapping_add(frames_this_pkt);
            current_seq.store(seq as u32, Ordering::Relaxed);
            current_rtptime.store(rtptime, Ordering::Relaxed);
            first_packet = false;
            packets_sent += 1;
            packets_since_origin += 1;

            if last_progress_emit.elapsed() >= Duration::from_millis(500) {
                let pos_sec = frame_pos as f64 / RAOP_SAMPLE_RATE as f64;
                on_progress(pos_sec, duration_sec, "PLAYING");
                last_progress_emit = Instant::now();
            }

            // Monotonic drift-free pacing: 352 / 44100 s per packet
            let target_ns = (packets_since_origin * RAOP_FRAMES_PER_PACKET as u64 * 1_000_000_000)
                / RAOP_SAMPLE_RATE as u64;
            let deadline = pace_origin + Duration::from_nanos(target_ns);
            let now = Instant::now();
            if deadline > now {
                std::thread::sleep(deadline - now);
            }
        }

        if let Ok(mut r) = rtsp_arc.lock() {
            r.teardown();
        }
    });

    Ok(handle)
}
