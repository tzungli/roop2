use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use axum::{
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DisplayLyricLine {
    pub time_sec: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDisplayState {
    pub enabled: bool,
    pub is_playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    pub zone_name: String,
    pub quality_badge: String,
    pub cover_url: String,
    pub current_sec: f64,
    pub duration_sec: f64,
    pub updated_at_ms: u64,
    pub lyrics: Vec<DisplayLyricLine>,
    pub revision: u64,
}

impl Default for WebDisplayState {
    fn default() -> Self {
        Self {
            enabled: true,
            is_playing: false,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            year: String::new(),
            zone_name: "System Output".to_string(),
            quality_badge: String::new(),
            cover_url: String::new(),
            current_sec: 0.0,
            duration_sec: 0.0,
            updated_at_ms: now_ms(),
            lyrics: Vec::new(),
            revision: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDisplayUpdate {
    #[serde(default)]
    pub is_playing: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub year: String,
    #[serde(default)]
    pub zone_name: String,
    #[serde(default)]
    pub quality_badge: String,
    #[serde(default)]
    pub cover_src: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub current_sec: f64,
    #[serde(default)]
    pub duration_sec: f64,
    #[serde(default)]
    pub lyrics: Vec<DisplayLyricLine>,
}

static DISPLAY_STATE: std::sync::LazyLock<RwLock<WebDisplayState>> =
    std::sync::LazyLock::new(|| RwLock::new(WebDisplayState::default()));

static LAST_COVER_SIG: std::sync::LazyLock<RwLock<(String, u64)>> =
    std::sync::LazyLock::new(|| RwLock::new((String::new(), 1)));

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn is_web_display_enabled() -> bool {
    DISPLAY_STATE.read().map(|s| s.enabled).unwrap_or(true)
}

pub fn set_web_display_enabled(enabled: bool) -> bool {
    if let Ok(mut w) = DISPLAY_STATE.write() {
        w.enabled = enabled;
        w.revision = w.revision.saturating_add(1);
        w.updated_at_ms = now_ms();
    }
    enabled
}

pub fn update_web_display_state(upd: WebDisplayUpdate) {
    let cover_src_trimmed = upd.cover_src.trim().to_string();
    let file_path_trimmed = upd.file_path.trim().to_string();

    // If cover is a public http(s) URL (e.g. TIDAL), use it directly;
    // otherwise register it under token "web_display" so any LAN browser can load /cover/web_display/cover.jpg!
    let resolved_cover_url = if cover_src_trimmed.starts_with("http://") || cover_src_trimmed.starts_with("https://") {
        cover_src_trimmed.clone()
    } else if !cover_src_trimmed.is_empty() || !file_path_trimmed.is_empty() {
        let sig = format!("{}|{}", cover_src_trimmed, file_path_trimmed);
        let rev = {
            let mut guard = LAST_COVER_SIG.write().unwrap();
            if guard.0 != sig {
                guard.0 = sig;
                guard.1 = guard.1.saturating_add(1);
                let cover_opt = if cover_src_trimmed.is_empty() || cover_src_trimmed.starts_with("data:") {
                    None
                } else {
                    Some(cover_src_trimmed.clone())
                };
                let file_opt = if file_path_trimmed.is_empty() {
                    None
                } else {
                    Some(file_path_trimmed.clone())
                };
                crate::server::register_cover_for_token("web_display", cover_opt, file_opt);
            }
            guard.1
        };
        format!("/cover/web_display/cover.jpg?v={}", rev)
    } else {
        String::new()
    };

    if let Ok(mut w) = DISPLAY_STATE.write() {
        w.is_playing = upd.is_playing;
        w.title = upd.title;
        w.artist = upd.artist;
        w.album = upd.album;
        w.year = upd.year;
        w.zone_name = if upd.zone_name.is_empty() {
            "System Output".to_string()
        } else {
            upd.zone_name
        };
        w.quality_badge = upd.quality_badge;
        w.cover_url = resolved_cover_url;
        w.current_sec = upd.current_sec.max(0.0);
        w.duration_sec = upd.duration_sec.max(0.0);
        w.updated_at_ms = now_ms();
        w.lyrics = upd.lyrics;
        w.revision = w.revision.saturating_add(1);
    }
}

pub async fn handle_display_page() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store, must-revalidate")],
        Html(WEB_DISPLAY_HTML),
    )
}

pub async fn handle_display_state() -> Response {
    let snapshot = DISPLAY_STATE.read().map(|s| s.clone()).unwrap_or_default();
    let mut resp = Json(snapshot).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    resp
}

const WEB_DISPLAY_HTML: &str = r##"<!DOCTYPE html>
<html lang="zh-TW">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>rooPlayer Display</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    html, body {
      width: 100%;
      height: 100%;
      overflow: hidden;
      background: #0b0d14;
      color: #ffffff;
      font-family: "Segoe UI", "PingFang TC", "Microsoft JhengHei", -apple-system, BlinkMacSystemFont, sans-serif;
      -webkit-font-smoothing: antialiased;
    }
    body.idle-cursor { cursor: none; }
    .bg-wrap {
      position: fixed;
      inset: -40px;
      z-index: 0;
      overflow: hidden;
    }
    .bg-img {
      width: 100%;
      height: 100%;
      background-size: cover;
      background-position: center;
      filter: blur(48px) saturate(1.25) brightness(0.42);
      transform: scale(1.12);
      transition: background-image 0.9s ease;
    }
    .bg-scrim {
      position: fixed;
      inset: 0;
      z-index: 1;
      background: radial-gradient(circle at 28% 45%, rgba(15, 18, 28, 0.25) 0%, rgba(10, 12, 20, 0.78) 72%, rgba(8, 10, 16, 0.92) 100%);
    }
    .top-bar {
      position: fixed;
      top: 24px;
      left: 36px;
      right: 36px;
      z-index: 10;
      display: flex;
      align-items: center;
      justify-content: space-between;
      pointer-events: none;
    }
    .brand-pill {
      display: inline-flex;
      align-items: center;
      gap: 10px;
      font-size: 14px;
      font-weight: 700;
      letter-spacing: 0.04em;
      color: rgba(255, 255, 255, 0.85);
    }
    .brand-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: #7c5cff;
      box-shadow: 0 0 12px #7c5cff;
    }
    .zone-pill {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 6px 14px;
      border-radius: 999px;
      background: rgba(255, 255, 255, 0.1);
      backdrop-filter: blur(12px);
      font-size: 13px;
      font-weight: 600;
      color: rgba(255, 255, 255, 0.9);
      pointer-events: auto;
    }
    .fs-btn {
      margin-left: 10px;
      padding: 6px 12px;
      border-radius: 999px;
      border: 1px solid rgba(255, 255, 255, 0.22);
      background: rgba(255, 255, 255, 0.1);
      color: #ffffff;
      font-size: 12px;
      cursor: pointer;
      pointer-events: auto;
      transition: opacity 0.3s ease, background 0.18s ease;
    }
    .fs-btn:hover { background: rgba(124, 92, 255, 0.45); }
    body.idle-cursor .fs-btn { opacity: 0; pointer-events: none; }

    .stage {
      position: relative;
      z-index: 2;
      width: 100%;
      height: 100%;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 76px 6vw 88px;
      gap: 5vw;
    }
    .art-col {
      flex: 0 0 auto;
      display: flex;
      flex-direction: column;
      align-items: center;
    }
    .cover-frame {
      width: min(44vh, 38vw, 460px);
      height: min(44vh, 38vw, 460px);
      border-radius: 12px;
      overflow: hidden;
      background: rgba(255, 255, 255, 0.06);
      box-shadow: 0 28px 70px rgba(0, 0, 0, 0.65);
    }
    .cover-frame img {
      width: 100%;
      height: 100%;
      object-fit: cover;
      display: block;
    }
    .info-col {
      flex: 1 1 0;
      min-width: 0;
      max-width: 680px;
      display: flex;
      flex-direction: column;
      gap: 12px;
    }
    .track-title {
      font-size: clamp(28px, 3.8vw, 52px);
      font-weight: 700;
      line-height: 1.15;
      color: #ffffff;
      text-shadow: 0 4px 20px rgba(0, 0, 0, 0.45);
      word-break: break-word;
    }
    .track-artist {
      font-size: clamp(18px, 2.2vw, 30px);
      font-weight: 500;
      color: rgba(255, 255, 255, 0.86);
    }
    .track-album {
      font-size: clamp(14px, 1.4vw, 20px);
      color: rgba(255, 255, 255, 0.62);
    }
    .badge-row {
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;
      margin-top: 4px;
    }
    .quality-badge {
      display: inline-flex;
      align-items: center;
      padding: 4px 12px;
      border-radius: 999px;
      background: rgba(124, 92, 255, 0.24);
      border: 1px solid rgba(167, 139, 250, 0.45);
      color: #e9d5ff;
      font-size: 13px;
      font-weight: 600;
      letter-spacing: 0.03em;
    }
    .lyrics-box {
      margin-top: 18px;
      height: clamp(140px, 28vh, 260px);
      overflow-y: auto;
      scrollbar-width: none;
      -ms-overflow-style: none;
      mask-image: linear-gradient(180deg, transparent 0%, #000 18%, #000 82%, transparent 100%);
      -webkit-mask-image: linear-gradient(180deg, transparent 0%, #000 18%, #000 82%, transparent 100%);
      padding: 48px 0;
    }
    .lyrics-box::-webkit-scrollbar { display: none; }
    .lyric-line {
      font-size: clamp(16px, 1.8vw, 24px);
      line-height: 1.65;
      color: rgba(255, 255, 255, 0.36);
      padding: 4px 0;
      transition: color 0.25s ease, transform 0.25s ease;
      transform-origin: left center;
    }
    .lyric-line.active {
      color: #ffffff;
      font-weight: 700;
      transform: scale(1.04);
    }

    .progress-dock {
      position: fixed;
      left: 6vw;
      right: 6vw;
      bottom: 28px;
      z-index: 10;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .progress-track {
      width: 100%;
      height: 5px;
      border-radius: 999px;
      background: rgba(255, 255, 255, 0.16);
      overflow: hidden;
    }
    .progress-fill {
      height: 100%;
      width: 0%;
      border-radius: 999px;
      background: linear-gradient(90deg, #7c5cff 0%, #a78bfa 100%);
      transition: width 0.25s linear;
    }
    .time-row {
      display: flex;
      justify-content: space-between;
      font-size: 13px;
      font-variant-numeric: tabular-nums;
      color: rgba(255, 255, 255, 0.68);
    }

    .idle-screen {
      position: fixed;
      inset: 0;
      z-index: 5;
      display: none;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 14px;
      text-align: center;
      padding: 24px;
    }
    .idle-logo {
      font-size: 42px;
      font-weight: 800;
      letter-spacing: -0.02em;
      background: linear-gradient(135deg, #c4b5fd, #7c5cff);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
    }
    .idle-msg {
      font-size: 18px;
      color: rgba(255, 255, 255, 0.68);
    }

    @media (max-width: 820px) {
      .stage { flex-direction: column; text-align: center; padding: 72px 24px 84px; gap: 22px; }
      .info-col { align-items: center; }
      .badge-row { justify-content: center; }
      .lyric-line { transform-origin: center center; }
    }
  </style>
</head>
<body>
  <div class="bg-wrap"><div class="bg-img" id="bgImg"></div></div>
  <div class="bg-scrim"></div>

  <div class="top-bar">
    <div class="brand-pill"><span class="brand-dot"></span><span>rooPlayer</span></div>
    <div style="display:flex;align-items:center;">
      <div class="zone-pill" id="zonePill">System Output</div>
      <button class="fs-btn" id="btnFullscreen" type="button">全螢幕</button>
    </div>
  </div>

  <div class="stage" id="stage">
    <div class="art-col">
      <div class="cover-frame"><img id="coverImg" src="" alt=""></div>
    </div>
    <div class="info-col">
      <div class="track-title" id="trackTitle"></div>
      <div class="track-artist" id="trackArtist"></div>
      <div class="track-album" id="trackAlbum"></div>
      <div class="badge-row" id="badgeRow"></div>
      <div class="lyrics-box" id="lyricsBox" style="display:none;"></div>
    </div>
  </div>

  <div class="progress-dock" id="progressDock">
    <div class="progress-track"><div class="progress-fill" id="progressFill"></div></div>
    <div class="time-row"><span id="timeElapsed">0:00</span><span id="timeTotal">0:00</span></div>
  </div>

  <div class="idle-screen" id="idleScreen">
    <div class="idle-logo">rooPlayer</div>
    <div class="idle-msg" id="idleMsg">待命中，等待播放音樂...</div>
  </div>

  <script>
    let latestState = null;
    let lastLyricsSig = '';
    let lastCoverUrl = '';
    let lastActiveIdx = -1;

    function fmtTime(sec) {
      const s = Math.max(0, Math.floor(sec || 0));
      const m = Math.floor(s / 60);
      const r = s % 60;
      return `${m}:${r < 10 ? '0' : ''}${r}`;
    }

    function computeNowSec() {
      if (!latestState) return 0;
      let cur = Number(latestState.currentSec) || 0;
      if (latestState.isPlaying && latestState.updatedAtMs) {
        const dt = Math.max(0, (Date.now() - latestState.updatedAtMs) / 1000);
        cur += Math.min(dt, 4.0);
      }
      if (latestState.durationSec > 0) cur = Math.min(cur, latestState.durationSec);
      return cur;
    }

    function renderLyricsDom(lyrics) {
      const box = document.getElementById('lyricsBox');
      if (!Array.isArray(lyrics) || lyrics.length === 0) {
        box.style.display = 'none';
        box.innerHTML = '';
        lastLyricsSig = '';
        lastActiveIdx = -1;
        return;
      }
      const sig = lyrics.length + '|' + (lyrics[0]?.text || '') + '|' + (lyrics[lyrics.length - 1]?.text || '');
      if (sig === lastLyricsSig) return;
      lastLyricsSig = sig;
      lastActiveIdx = -1;
      box.style.display = 'block';
      box.innerHTML = '';
      lyrics.forEach((line, i) => {
        const div = document.createElement('div');
        div.className = 'lyric-line';
        div.dataset.idx = String(i);
        div.textContent = line.text || '';
        box.appendChild(div);
      });
    }

    function applyState(st) {
      latestState = st;
      const stage = document.getElementById('stage');
      const dock = document.getElementById('progressDock');
      const idle = document.getElementById('idleScreen');
      const idleMsg = document.getElementById('idleMsg');
      const zonePill = document.getElementById('zonePill');

      if (zonePill) zonePill.textContent = st.zoneName || 'System Output';

      if (!st.enabled) {
        stage.style.display = 'none';
        dock.style.display = 'none';
        idle.style.display = 'flex';
        idleMsg.textContent = '網頁顯示器已於 rooPlayer 設定中停用';
        return;
      }

      if (!st.title) {
        stage.style.display = 'none';
        dock.style.display = 'none';
        idle.style.display = 'flex';
        idleMsg.textContent = '待命中，等待播放音樂...';
        return;
      }

      idle.style.display = 'none';
      stage.style.display = 'flex';
      dock.style.display = 'flex';

      document.getElementById('trackTitle').textContent = st.title || '';
      document.getElementById('trackArtist').textContent = st.artist || '';
      const albumLine = [st.album, st.year].filter(Boolean).join(' • ');
      document.getElementById('trackAlbum').textContent = albumLine;

      const badgeRow = document.getElementById('badgeRow');
      badgeRow.innerHTML = st.qualityBadge
        ? `<span class="quality-badge">${st.qualityBadge.replace(/</g, '&lt;')}</span>`
        : '';

      if (st.coverUrl && st.coverUrl !== lastCoverUrl) {
        lastCoverUrl = st.coverUrl;
        document.getElementById('coverImg').src = st.coverUrl;
        document.getElementById('bgImg').style.backgroundImage = `url("${st.coverUrl}")`;
      }

      renderLyricsDom(st.lyrics);
    }

    function tickSmooth() {
      if (latestState && latestState.enabled && latestState.title) {
        const cur = computeNowSec();
        const dur = Number(latestState.durationSec) || 0;
        const pct = dur > 0 ? Math.min(100, Math.max(0, (cur / dur) * 100)) : 0;
        document.getElementById('progressFill').style.width = `${pct.toFixed(2)}%`;
        document.getElementById('timeElapsed').textContent = fmtTime(cur);
        document.getElementById('timeTotal').textContent = fmtTime(dur);

        const lyrics = latestState.lyrics;
        if (Array.isArray(lyrics) && lyrics.length > 0) {
          let activeIdx = -1;
          for (let i = 0; i < lyrics.length; i++) {
            if (cur + 0.15 >= lyrics[i].timeSec) activeIdx = i;
            else break;
          }
          if (activeIdx !== lastActiveIdx) {
            lastActiveIdx = activeIdx;
            const box = document.getElementById('lyricsBox');
            const nodes = box.querySelectorAll('.lyric-line');
            nodes.forEach((n, i) => n.classList.toggle('active', i === activeIdx));
            if (activeIdx >= 0 && nodes[activeIdx]) {
              nodes[activeIdx].scrollIntoView({ behavior: 'smooth', block: 'center' });
            }
          }
        }
      }
      requestAnimationFrame(tickSmooth);
    }

    async function poll() {
      try {
        const r = await fetch('/display/state', { cache: 'no-store' });
        if (r.ok) {
          const st = await r.json();
          applyState(st);
        }
      } catch (_) {}
      setTimeout(poll, 1000);
    }

    document.getElementById('btnFullscreen').addEventListener('click', () => {
      if (!document.fullscreenElement) {
        document.documentElement.requestFullscreen?.().catch(() => {});
      } else {
        document.exitFullscreen?.().catch(() => {});
      }
    });

    let idleTimer = null;
    document.addEventListener('mousemove', () => {
      document.body.classList.remove('idle-cursor');
      clearTimeout(idleTimer);
      idleTimer = setTimeout(() => document.body.classList.add('idle-cursor'), 3000);
    });

    poll();
    requestAnimationFrame(tickSmooth);
  </script>
</body>
</html>
"##;
