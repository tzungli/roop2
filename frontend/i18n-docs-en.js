/* rooPlayer 說明頁（About）的英文版本
 * index.html 的說明頁各區塊都標了 data-i18n-html="key"；語言選到 English 時，
 * i18n.js 會把這些區塊整段換成下面對應的英文（換回中文時原樣還原）。
 * 這裡的英文與中文內容一段對一段，結構（badge / h4 / ul / strong / code）保持一致。
 */
(function () {
  'use strict';

  const DOC_EN = {
    // ================= 說明頁：各選單操作手冊 =================
    'about.manual.title': `<span class="title-icon">📖</span>
              <h3>Menu Features &amp; User Manual</h3>`,

    'about.manual.intro': `
              rooPlayer combines high-performance Rust + Tauri v2 native architecture with local lossless library management, dedicated rooDSP processing, WASAPI Exclusive bit-perfect output, and DLNA network streaming. Key features are summarized below:
            `,

    'about.manual.langnote': `
              <strong>Language &amp; Adaptive Window</strong>: Toggle between Traditional Chinese and English, or Dark and Light themes from the top bar. Window dimensions are constrained to a minimum of 840 × 680 with responsive layout to prevent button collisions.
            `,

    'about.manual.m1': `<div class="manual-item-header">
                  <span class="item-badge">Overview</span>
                  <h4>Home Overview (Overview)</h4>
                </div>
                <div class="manual-item-body">
                  <p>Central music dashboard integrating local files and online streaming:</p>
                  <ul>
                    <li><strong>Library Statistics</strong>: Real-time count of albums, tracks, and Hi-Res formats.</li>
                    <li><strong>Recently Played</strong>: Displays recent listening history with one-click resume.</li>
                    <li><strong>Daily Picks &amp; Spotlight</strong>: Curated audiophile recommendations and artist spotlights refreshed daily.</li>
                  </ul>
                </div>`,

    'about.manual.m2': `<div class="manual-item-header">
                  <span class="item-badge">Browsing</span>
                  <h4>Albums, Artists &amp; Tracks (Library)</h4>
                </div>
                <div class="manual-item-body">
                  <p>Multi-dimensional library browsing with customizable layout:</p>
                  <ul>
                    <li><strong>Sorting</strong>: Sort by date added, title A-Z, artist, release year, or play count.</li>
                    <li><strong>Cover Sizing</strong>: Toggle between small, medium, and large grids with per-page persistence.</li>
                    <li><strong>Clean Metadata</strong>: Elegant presentation of artist, record label, year, and track details.</li>
                  </ul>
                </div>`,

    'about.manual.m3': `<div class="manual-item-header">
                  <span class="item-badge">Favorites</span>
                  <h4>Favorites (Albums / Tracks / Artists)</h4>
                </div>
                <div class="manual-item-body">
                  <p>Personal audiophile favorites library:</p>
                  <ul>
                    <li><strong>One-Click Favoriting</strong>: Click the heart icon (♡) on any album, track, or artist.</li>
                    <li><strong>Quick Playback</strong>: Play all favorites in order or shuffle with one click.</li>
                    <li><strong>Backup &amp; Migration</strong>: Export favorites or import them with lossless de-duplication.</li>
                  </ul>
                </div>`,

    'about.manual.m4': `<div class="manual-item-header">
                  <span class="item-badge" style="background: rgba(139, 92, 246, 0.16); color: #a78bfa; border: 1px solid rgba(139, 92, 246, 0.35);">Playlists</span>
                  <h4>Playlists (Create / Edit / Import)</h4>
                </div>
                <div class="manual-item-body">
                  <p>Organize and customize track playback order:</p>
                  <ul>
                    <li><strong>Save &amp; Create</strong>: Save the current playback queue into a named playlist or create a blank one.</li>
                    <li><strong>Track Reordering</strong>: Move tracks up/down, remove individual songs, or rename playlists.</li>
                    <li><strong>Import &amp; Export</strong>: Full support for <code>.m3u</code>, <code>.m3u8</code>, and <code>.json</code> formats.</li>
                  </ul>
                </div>`,

    'about.manual.m5': `<div class="manual-item-header">
                  <span class="item-badge" style="background: rgba(0, 229, 255, 0.15); color: #00e5ff; border: 1px solid rgba(0, 229, 255, 0.3);">Hi-Res Streaming</span>
                  <h4>TIDAL Streaming Explorer (TIDAL Hi-Res)</h4>
                </div>
                <div class="manual-item-body">
                  <p>Direct integration with master-quality TIDAL Hi-Res MAX streaming:</p>
                  <ul>
                    <li><strong>Audiophile Picks</strong>: Filter reference albums, vocals, classical, and jazz in one tap.</li>
                    <li><strong>New Releases &amp; Mixes</strong>: Stream latest high-dynamic releases and official playlists.</li>
                    <li><strong>Cloud Sync &amp; Library Integration</strong>: Sync TIDAL favorites or add online albums directly to local library.</li>
                  </ul>
                </div>`,

    'about.manual.m6': `<div class="manual-item-header">
                  <span class="item-badge" style="background: rgba(37, 213, 255, 0.15); color: #25d5ff; border: 1px solid rgba(37, 213, 255, 0.3);">Online Streaming</span>
                  <h4>KKBOX Streaming Center (KKBOX)</h4>
                </div>
                <div class="manual-item-body">
                  <p>Streamlined access to Mandarin and global pop catalogs:</p>
                  <ul>
                    <li><strong>Live Charts</strong>: Real-time charts across Mandarin, Western, Asian pop, rock, and jazz.</li>
                    <li><strong>Featured &amp; New Releases</strong>: Curated mood playlists and trending hits.</li>
                    <li><strong>Chart Playback &amp; Search</strong>: One-click chart playback, queue integration, and live search.</li>
                  </ul>
                </div>`,

    'about.manual.m7': `<div class="manual-item-header">
                  <span class="item-badge">Storage &amp; Maintenance</span>
                  <h4>Storage &amp; Database Maintenance</h4>
                </div>
                <div class="manual-item-body">
                  <p>Local folder indexing and database backup:</p>
                  <ul>
                    <li><strong>Fast Scanning</strong>: High-speed metadata and artwork extraction for FLAC, WAV, DSF, and DFF.</li>
                    <li><strong>Read-Only Safety</strong>: Scans audio files in read-only mode without modifying original files.</li>
                    <li><strong>Backup &amp; Restore</strong>: Export library data and favorites as JSON with smart de-duplication on import.</li>
                  </ul>
                </div>`,

    'about.manual.m8': `<div class="manual-item-header">
                  <span class="item-badge" style="background: rgba(168, 85, 247, 0.15); color: #c084fc; border: 1px solid rgba(168, 85, 247, 0.35);">Dedicated DSP Processor</span>
                  <h4>Dedicated rooDSP Processor (rooDSP)</h4>
                </div>
                <div class="manual-item-body">
                  <p>Full-page audiophile DSP tuning environment with preset management:</p>
                  <ul>
                    <li><strong>Headroom Management</strong>: Set 0 to -24dB pre-attenuation to prevent clipping distortion with built-in clip indicator.</li>
                    <li><strong>Sample Rate Conversion (SRC)</strong>:
                      <br>• <strong>PCM Upsampling</strong>: 2x/4x integer upsampling with high-grade FIR filter phases (Pure-Flat, Linear/Minimum phase, Gaussian, PS-EXT2 with up to -180dB stopband attenuation).
                      <br>• <strong>1-bit DSD Modulation</strong>: 2nd, 5th, and 7th-order resonant noise-shaping Delta-Sigma modulators (DSD64 to DSD512).
                    </li>
                    <li><strong>Crossfeed</strong>: Blends stereo channels to alleviate headphone "in-head" effect and recreate a natural frontal soundstage.</li>
                    <li><strong>Parametric EQ (PEQ)</strong>: Interactive drag-and-drop curve editor with multi-band biquads and 6 named user preset slots.</li>
                    <li><strong>Convolution (IR)</strong>: Load room correction or headphone impulse-response WAVs with 64-bit floating partitioned FFT.</li>
                    <li><strong>Ultrasonic Protection</strong>: 30k / 50k high-frequency safety cutoffs to protect amplifiers and tweeters from ultrasonic stress.</li>
                  </ul>
                </div>`,

    'about.manual.m9': `<div class="manual-item-header">
                  <span class="item-badge">Output &amp; Signal Path</span>
                  <h4>Signal Path &amp; Audio Zones</h4>
                </div>
                <div class="manual-item-body">
                  <p>Audiophile bit-perfect delivery and network streaming:</p>
                  <ul>
                    <li><strong>Signal Path</strong>: Click the purple status dot to inspect the live chain ("Source ➔ Decoder ➔ DSP ➔ Output") with Bit-Perfect verification.</li>
                    <li><strong>WASAPI Exclusive Mode</strong>: Bypasses the Windows mixer with a dedicated thread directly to the USB DAC for pure bit-perfect playback.</li>
                    <li><strong>DLNA / UPnP Streaming</strong>: Auto-discover or probe LAN network streamers (Eversolo, WiIM, Linn, etc.) with lossless PCM and DSF pass-through.</li>
                    <li><strong>Progressive Flow Control</strong>: Multi-core parallel generation streams audio on the fly with instant start and buffer clearing on track changes.</li>
                  </ul>
                </div>`,

    'about.manual.m10': `<div class="manual-item-header">
                  <span class="item-badge">Smart Search</span>
                  <h4>Global Real-Time Search</h4>
                </div>
                <div class="manual-item-body">
                  <p>Instant millisecond search across libraries:</p>
                  <ul>
                    <li><strong>Unified Search</strong>: Query local library and online catalogs (TIDAL / KKBOX) simultaneously.</li>
                    <li><strong>Filter Tabs</strong>: Quickly switch between All, Local, and Streaming results.</li>
                  </ul>
                </div>`,

    'about.manual.m11': `<div class="manual-item-header">
                  <span class="item-badge">Playback Experience</span>
                  <h4>Player Controls &amp; Adaptive UI</h4>
                </div>
                <div class="manual-item-body">
                  <p>Immersive audiophile player and responsive window layout:</p>
                  <ul>
                    <li><strong>Audiophile Bottom Bar</strong>: Full playback controls, repeat/shuffle modes, volume slider, and live Hi-Res format badge.</li>
                    <li><strong>Full-Screen Now Playing</strong>: Click cover art to open full-screen view featuring synced lyrics, large album artwork, album info, and tracklist.</li>
                    <li><strong>840 × 680 Window Protection</strong>: Minimum window boundary locked to 840 × 680 with responsive element layout to eliminate button collisions.</li>
                    <li><strong>Smart Resume</strong>: Automatically restores the last played track upon launch.</li>
                  </ul>
                </div>`,

    // ================= 使用授權範圍 =================
    'about.license.title': `<span class="title-icon">📜</span>
              <h3>License &amp; Scope of Use</h3>`,

    'about.license.b1': `<h4>1. Personal and Non-Commercial Use</h4>
                <p>
                  rooPlayer is developed in the spirit of the MIT license, for personal music lovers and audiophiles to play audio, manage master digital files, and run digital signal processing (DSP) on a personal computer or home network.
                </p>`,

    'about.license.b2': `<h4>2. Third-Party Components &amp; Open-Source Notices</h4>
                <p>This software combines industry-standard open-source technologies with proprietary native modules:</p>
                <ul>
                  <li><strong>Rust + Tauri v2</strong>: Modern native runtime delivering ultra-low memory footprint and 0% idle CPU usage.</li>
                  <li><strong>WASAPI Exclusive (cpal / windows-rs)</strong>: Low-level Windows audio exclusive mode for bit-perfect output.</li>
                  <li><strong>rooDSP Native Kernel (Rust)</strong>: 64-bit floating polyphase FIR upsampler, Crossfeed, PEQ biquad chain, partitioned FFT convolution (IR), and 1-bit Delta-Sigma DSD modulators.</li>
                  <li><strong>@wasm-audio-decoders/flac</strong>: High-performance pure-local WebAssembly FLAC decoding.</li>
                  <li><strong>music-metadata</strong>: Audio header parsing and master metadata extraction.</li>
                </ul>`,

    'about.license.b3': `<h4>3. Streaming Platform API Terms</h4>
                <p>
                  The online catalog features use the public interfaces of KKBOX Open API and TIDAL. All track rights, cover images, and preview clips remain the property of the platforms and record labels. This program does not decrypt or record DRM-protected audio without authorization.
                </p>`,

    // ================= 免責聲明 =================
    'about.disclaimer.title': `<span class="title-icon">⚖️</span>
              <h3>Disclaimer</h3>`,

    'about.disclaimer.b1': `<h4>1. Audio Hardware Output &amp; Volume Protection</h4>
                <p>
                  High sample rates (such as 384 kHz, DSD256, DSD512) produce wide frequency response and high-frequency energy. Before switching upsampling mode or changing DAC settings, <strong>please set amplifier volume to a safe level</strong> to prevent damage to equipment or hearing caused by transient spikes.
                </p>`,

    'about.disclaimer.b2': `<h4>2. Safety of Local Audio Files</h4>
                <p>
                  rooPlayer reads your original lossless audio files in <strong>read-only</strong> mode. Removing folders from the database only deletes internal index caches; no audio file on disk is modified or deleted.
                </p>`,

    'about.disclaimer.b3': `<h4>3. Software Provided AS-IS</h4>
                <p>
                  This software is provided “as is”, without any express or implied warranty, including but not limited to fitness for a particular purpose or error-free operation.
                </p>`,

    // ================= 核心技術與發燒音訊規格 =================
    'about.tech.title': `<span class="title-icon">⚙️</span>
              <h3>Core Technology &amp; Audiophile Specifications</h3>`,

    'about.tech.s1': `<span class="spec-label">Supported Audio Formats</span>
                <span class="spec-value">FLAC (24/192), WAV (32-bit Float), AIFF, DSF (DSD64/128/256/512), DFF, MP3, M4A, AAC, OGG</span>`,

    'about.tech.s2': `<span class="spec-label">PCM Upsampling Engine</span>
                <span class="spec-value">Proprietary 64-bit floating polyphase FIR kernel: 2x/4x integer upsampling, 96k/192k/384k DXD; 8 filter phases including Pure-Flat, Linear/Min-Phase, Gaussian, PS-EXT2 (stopband down to -180dB)</span>`,

    'about.tech.s3': `<span class="spec-label">DSD Modulator</span>
                <span class="spec-value">2nd/5th/7th-order resonant noise-shaping 1-bit Delta-Sigma modulators (DSD64 to DSD512); parallel multi-core modulation with seamless segment stitching</span>`,

    'about.tech.s4': `<span class="spec-label">Dedicated rooDSP</span>
                <span class="spec-value">Headroom management (-24dB to 0dB), Crossfeed, 6-band PEQ with 6 user preset slots, Convolution (IR), 30k/50k ultrasonic safety filters</span>`,

    'about.tech.s5': `<span class="spec-label">Audio Output Architecture</span>
                <span class="spec-value">WASAPI Exclusive Mode (Bit-Perfect direct DAC access), DLNA / UPnP MediaRenderer (progressive HTTP streaming with native DSF pass-through)</span>`,

    'about.tech.s6': `<span class="spec-label">Local Database</span>
                <span class="spec-value">High-speed JSON index cache, multi-level path/metadata de-duplication, full favorites and play-count synchronization</span>`,

    'about.tech.s7': `<span class="spec-label">Window &amp; Runtime Architecture</span>
                <span class="spec-value">Rust + Tauri v2 native window, 840×680 adaptive boundary protection, Dark/Light high-contrast themes, seamless bilingual switching</span>`,

    'about.filter_report': `<div class="about-card-title">
              <span class="title-icon">🔬</span>
              <h3>DSP Core Algorithm Benchmark Validation &amp; Audiophile Synergy Guide</h3>
            </div>
            <div class="manual-item-body" style="font-size: 13px; line-height: 1.7; color: var(--text-sub);">
              <p>
                In rooPlayer's bit-perfect digital audio processing chain, three defensive layers form an uncompromised high-fidelity barrier:
                <br>• <strong>First Line of Defense (SRC Filter)</strong>: Accurately eliminates ultrasonic spectral images during polyphase oversampling, eliminating aliasing fold-back.
                <br>• <strong>Second Line of Defense (Sigma-Delta Modulator)</strong>: Utilizes 5th-order (CRFB) or 7th-order (CLANS) multi-feedback noise shaping to push 1-bit quantization noise into ultra-high frequencies.
                <br>• <strong>Third Line of Defense (Ultrasonic Protection Filter)</strong>: Applies a 2nd-order Butterworth low-pass filter to attenuate ultrasonic energy before analog amplification, protecting amplifiers and tweeters from RF intermodulation distortion (IMD) and overheating.
              </p>

              <!-- Section 1: 7 Sample Rate Conversion (SRC) Filters: Principles & Benchmarks -->
              <h4 style="color: var(--text-main); margin-top: 22px; margin-bottom: 8px;">1. 7 Core Sample Rate Conversion (SRC) Filters: Principles &amp; Benchmarks</h4>
              <p style="font-size: 12.5px; margin-bottom: 8px;">
                Modeled after flagship high-end DSP architectures, combining four foundational orthogonal profiles (<strong>Frequency Roll-off</strong> &times; <strong>Phase Profile</strong>) with three reference long-tap linear-phase filters:
                <br>• <strong>Precise</strong>: Employs Kaiser window polyphase FIR (&beta; &approx; 14.47, A<sub>s</sub> &ge; 140 dB), flat passband (0~20kHz with 0.000 dB loss), and brickwall Nyquist attenuation.
                <br>• <strong>Smooth</strong>: Employs Gaussian-window sinc slow roll-off FIR, beginning a gentle slope at 18kHz to eliminate digital glare, reducing time-domain ringing duration by &gt;55%.
                <br>• <strong>Linear Phase</strong>: Symmetric impulse response with strictly constant group delay and <strong>absolute zero phase distortion</strong>.
                <br>• <strong>Minimum Phase</strong>: Employs complex cepstrum Hilbert spectral factorization to reflect zeros inside the unit circle, achieving <strong>strictly zero pre-ringing (0.00% Pre-ringing)</strong>, aligning perfectly with psychoacoustic pre-masking.
                <br>• <strong>Gauss Long</strong>: Inspired by HQPlayer <code>poly-sinc-gauss-long</code>; pure Gaussian window (&sigma; = 0.17L, edge attenuation &lt; -150 dB) with 449 input taps, dead-flat through 20 kHz, -166.6 dB at Nyquist, and ultra-smooth natural Gaussian ringing envelope.
                <br>• <strong>Ext2 (Extended Brickwall)</strong>: Inspired by HQPlayer <code>poly-sinc-ext2</code>; 1,281 input-tap Kaiser brickwall (A<sub>s</sub> = 175 dB), flat passband extended to 20.8 kHz (0.000 dB) and -183.0 dB suppression right at Nyquist.
                <br>• <strong>Sinc-M (Million-Tap FFT)</strong>: Inspired by HQPlayer <code>sinc-M</code> / Chord M-Scaler; <strong>Partitioned FFT Polyphase Convolution</strong> with stereo complex packing (L+iR, 2x throughput), 65,535 taps per phase (<strong>1,048,545 prototype taps</strong> at 16x), flat to 21.95 kHz, -205.3 dB first-image suppression at 22.15 kHz, 242.5 dB 1 kHz SNR, and 17.0 ms per stereo second at 4x (~59x real-time).
              </p>

              <!-- Table 1-1: Frequency Response Attenuation Benchmark -->
              <div style="overflow-x: auto; margin-bottom: 12px;">
                <div style="font-weight: 600; font-size: 12px; color: var(--text-main); margin-bottom: 4px;">Table 1-1: Frequency Response Attenuation (Benchmark: 44.1 kHz input &rarr; 4x PCM upsampling to 176.4 kHz, Nyquist = 22.05 kHz)</div>
                <table style="width: 100%; border-collapse: collapse; font-size: 12px; text-align: left;">
                  <thead>
                    <tr style="border-bottom: 1px solid var(--border-color); color: var(--text-main);">
                      <th style="padding: 8px 10px;">SRC Filter</th>
                      <th style="padding: 8px 10px;">10 kHz</th>
                      <th style="padding: 8px 10px;">18 kHz</th>
                      <th style="padding: 8px 10px;">20 kHz</th>
                      <th style="padding: 8px 10px;">22.05 kHz (Nyquist)</th>
                      <th style="padding: 8px 10px;">24 kHz (Stopband)</th>
                      <th style="padding: 8px 10px;">30 kHz (Ultrasonic)</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #38bdf8;">Precise, Linear Phase</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-142.55 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-151.01 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-161.50 dB</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #a78bfa;">Precise, Minimum Phase</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.002 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.008 dB</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-140.61 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-152.02 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-165.14 dB</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #34d399;">Smooth, Linear Phase</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.002 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.016 dB</td>
                      <td style="padding: 8px 10px; color: #fbbf24;">-7.902 dB</td>
                      <td style="padding: 8px 10px;">-77.62 dB</td>
                      <td style="padding: 8px 10px;">-73.48 dB</td>
                      <td style="padding: 8px 10px;">-83.73 dB</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #fbbf24;">Smooth, Minimum Phase</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.002 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.016 dB</td>
                      <td style="padding: 8px 10px; color: #fbbf24;">-7.902 dB</td>
                      <td style="padding: 8px 10px;">-76.94 dB</td>
                      <td style="padding: 8px 10px;">-73.86 dB</td>
                      <td style="padding: 8px 10px;">-85.96 dB</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #2dd4bf;">Gauss Long, Linear Phase</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-166.61 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-200.09 dB (@23k)</td>
                      <td style="padding: 8px 10px; color: #34d399;">-237.20 dB</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #f472b6;">Ext2 Brickwall, Linear Phase</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB (0dB @20.8k)</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-183.01 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">-192.71 dB (@23k)</td>
                      <td style="padding: 8px 10px; color: #34d399;">-213.26 dB</td>
                    </tr>
                    <tr>
                      <td style="padding: 8px 10px; font-weight: 600; color: #fb923c;">Sinc-M Million-Tap, Linear Phase</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">0.000 dB (0dB @21.95k)</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">&lt; -175 dB (-205.29 dB @22.15k)</td>
                      <td style="padding: 8px 10px; color: #34d399;">&lt; -205 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">&lt; -210 dB</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <!-- Table 1-2: Time-Domain Ringing Benchmark -->
              <div style="overflow-x: auto; margin-bottom: 16px;">
                <div style="font-weight: 600; font-size: 12px; color: var(--text-main); margin-bottom: 4px;">Table 1-2: Time-Domain Transient Response &amp; Ringing Energy Distribution</div>
                <table style="width: 100%; border-collapse: collapse; font-size: 12px; text-align: left;">
                  <thead>
                    <tr style="border-bottom: 1px solid var(--border-color); color: var(--text-main);">
                      <th style="padding: 8px 10px;">SRC Filter</th>
                      <th style="padding: 8px 10px;">Total Taps</th>
                      <th style="padding: 8px 10px;">Pre-Ringing Energy</th>
                      <th style="padding: 8px 10px;">Post-Ringing Energy</th>
                      <th style="padding: 8px 10px;">-60dB Ringing Duration</th>
                      <th style="padding: 8px 10px;">Phase Profile</th>
                      <th style="padding: 8px 10px;">Acoustic Signature</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #38bdf8;">Precise, Linear Phase</td>
                      <td style="padding: 8px 10px;">929</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px;">2.494 ms</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>Absolute Zero Phase Distortion</strong></td>
                      <td style="padding: 8px 10px;"><strong>Master Monitoring Purity</strong>: Expansive pinpoint soundstage, surgical instrument separation.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #a78bfa;">Precise, Minimum Phase</td>
                      <td style="padding: 8px 10px;">929</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">0.00% (Strictly Zero Pre-Ringing)</td>
                      <td style="padding: 8px 10px;">100.00%</td>
                      <td style="padding: 8px 10px;">3.005 ms</td>
                      <td style="padding: 8px 10px;">Causal Minimum Delay</td>
                      <td style="padding: 8px 10px;"><strong>Explosive Transient Punch</strong>: Energy concentrated at the leading edge, tight punchy bass and drum slam.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #34d399;">Smooth, Linear Phase</td>
                      <td style="padding: 8px 10px;">257</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">1.134 ms (-55% duration)</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>Absolute Zero Phase Distortion</strong></td>
                      <td style="padding: 8px 10px;"><strong>Lush &amp; Organic Staging</strong>: Ultra-short ringing, velvety smooth treble, lush natural harmonics.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #fbbf24;">Smooth, Minimum Phase</td>
                      <td style="padding: 8px 10px;">257</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">0.00% (Strictly Zero Pre-Ringing)</td>
                      <td style="padding: 8px 10px;">100.00%</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">1.009 ms (-60% duration)</td>
                      <td style="padding: 8px 10px;">Causal Minimum Delay</td>
                      <td style="padding: 8px 10px;"><strong>Audiophile Vinyl Warmth</strong>: Zero pre-ringing plus short decay, zero digital edge, fatigue-free musicality.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #2dd4bf;">Gauss Long, Linear Phase</td>
                      <td style="padding: 8px 10px;">1,793 (4x)</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px; color: #34d399;">Smooth Gaussian Envelope</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>Absolute Zero Phase Distortion</strong></td>
                      <td style="padding: 8px 10px;"><strong>All-Round Natural Staging</strong>: Zero truncation glare, spacious 3D soundstage with natural harmonics.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #f472b6;">Ext2 Brickwall, Linear Phase</td>
                      <td style="padding: 8px 10px;">5,121 (4x)</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px;">High-Order Symmetric Brickwall</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>Absolute Zero Phase Distortion</strong></td>
                      <td style="padding: 8px 10px;"><strong>Ultra-Transparent Detail</strong>: Extended 20.8kHz flat passband, -183dB Nyquist rejection, crisp micro-detail.</td>
                    </tr>
                    <tr>
                      <td style="padding: 8px 10px; font-weight: 600; color: #fb923c;">Sinc-M Million-Tap, Linear Phase</td>
                      <td style="padding: 8px 10px; font-weight: 600;">262,137 (4x)<br>1,048,545 (16x)</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px;">50.00%</td>
                      <td style="padding: 8px 10px; color: #34d399;">Partitioned FFT Zero-Shift<br>(17.0 ms / 1s stereo)</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>Absolute Zero Phase Distortion</strong><br>(242.48 dB SNR)</td>
                      <td style="padding: 8px 10px;"><strong>Million-Tap Spatial Precision</strong>: Flat to 21.95kHz, -205dB image cut at 22.15kHz, deep hall ambiance.</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <!-- Section 2: DSD 5th & 7th Order Modulators Benchmarks -->
              <h4 style="color: var(--text-main); margin-top: 22px; margin-bottom: 8px;">2. DSD 5th-Order (CRFB) &amp; 7th-Order (CLANS) Modulator Benchmarks</h4>
              <p style="font-size: 12.5px; margin-bottom: 8px;">
                rooPlayer features pure 64-bit floating-point Sigma-Delta modulators. Verified via 2.0s -6dBFS 1kHz sine stimulus with FFT spectral analysis and bit-level Hamming distance verification:
                <br>• <strong>5th-Order (CRFB)</strong>: Cascade of Resonators, Feedback Form architecture. Golden audiophile balance with gentle noise rise and rich analog atmosphere.
                <br>• <strong>7th-Order (CLANS)</strong>: Closed-Loop Asymptotic Noise Shaping. Features a noise shaping slope of <strong>52.6 dB/oct</strong> for pitch-black audible backgrounds.
              </p>

              <!-- Table 2-1: DSD 5th vs 7th Order Benchmark Table -->
              <div style="overflow-x: auto; margin-bottom: 16px;">
                <div style="font-weight: 600; font-size: 12px; color: var(--text-main); margin-bottom: 4px;">Table 2-1: DSD 5th vs 7th Order Objective Measurements (DSD64 / DSD128 / DSD256)</div>
                <table style="width: 100%; border-collapse: collapse; font-size: 12px; text-align: left;">
                  <thead>
                    <tr style="border-bottom: 1px solid var(--border-color); color: var(--text-main);">
                      <th style="padding: 8px 10px;">Sample Rate</th>
                      <th style="padding: 8px 10px;">Measured Metric</th>
                      <th style="padding: 8px 10px;">DSD 5th-Order (CRFB)</th>
                      <th style="padding: 8px 10px;">DSD 7th-Order (CLANS)</th>
                      <th style="padding: 8px 10px;">Validation &amp; Acoustic Meaning</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td rowspan="4" style="padding: 8px 10px; font-weight: 600; color: #fbbf24; vertical-align: top;">DSD64<br>(2.8224 MHz)</td>
                      <td style="padding: 8px 10px;">Hamming Bit Distance</td>
                      <td colspan="2" style="padding: 8px 10px; color: #34d399; font-weight: 600;">48.48% Bit Difference (96.69% Byte Difference)</td>
                      <td style="padding: 8px 10px;"><strong>100% Active Autonomy</strong>: Proves completely distinct, independent bitstreams.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px;">Audio Band SNR / THD+N</td>
                      <td style="padding: 8px 10px;">86.69 dB / -86.69 dB</td>
                      <td style="padding: 8px 10px;">86.70 dB / -86.69 dB</td>
                      <td style="padding: 8px 10px;">THD &lt; 0.00017%, pristine linearity without harmonic spurs.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px;">NTF Noise Shaping Slope</td>
                      <td style="padding: 8px 10px; color: #38bdf8;">36.0 dB/oct</td>
                      <td style="padding: 8px 10px; color: #a78bfa; font-weight: 600;">52.6 dB/oct</td>
                      <td style="padding: 8px 10px;">7th order delivers steep 52.6 dB/oct out-of-band noise rejection.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.15);">
                      <td style="padding: 8px 10px;">Integrator State (Stability)</td>
                      <td style="padding: 8px 10px;">0.4523 (Max)</td>
                      <td style="padding: 8px 10px;">0.5398 (Max)</td>
                      <td style="padding: 8px 10px;">Well below 6.0 threshold, <strong>guaranteed stable with zero overload</strong>.</td>
                    </tr>

                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td rowspan="3" style="padding: 8px 10px; font-weight: 600; color: #38bdf8; vertical-align: top;">DSD128<br>(5.6448 MHz)</td>
                      <td style="padding: 8px 10px;">Hamming Bit Distance</td>
                      <td colspan="2" style="padding: 8px 10px; color: #34d399; font-weight: 600;">47.80% Bit Difference (96.44% Byte Difference)</td>
                      <td style="padding: 8px 10px;">Independent dynamic bitstream, vibrant acoustic energy.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px;">Audio Band SNR / THD+N</td>
                      <td style="padding: 8px 10px; color: #34d399;">90.60 dB / -90.57 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;">90.60 dB / -90.57 dB</td>
                      <td style="padding: 8px 10px;">High dynamic headroom, micro-details fully preserved.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.15);">
                      <td style="padding: 8px 10px;">Ultrasonic Noise (20k - 50k)</td>
                      <td style="padding: 8px 10px; color: #34d399;">-90.92 dB</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-96.76 dB</td>
                      <td style="padding: 8px 10px;">7th order pushes near-band ultrasonic noise down to -96.76 dB.</td>
                    </tr>

                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td rowspan="2" style="padding: 8px 10px; font-weight: 600; color: #a78bfa; vertical-align: top;">DSD256<br>(11.2896 MHz)</td>
                      <td style="padding: 8px 10px;">Hamming Bit Distance</td>
                      <td colspan="2" style="padding: 8px 10px; color: #34d399; font-weight: 600;">47.97% Bit Difference (96.64% Byte Difference)</td>
                      <td style="padding: 8px 10px;">High-density 1-bit stream with massive micro-harmonic information.</td>
                    </tr>
                    <tr>
                      <td style="padding: 8px 10px;">Ultrasonic Noise (20k - 50k)</td>
                      <td style="padding: 8px 10px; color: #34d399;">-118.60 dB</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-120.21 dB</td>
                      <td style="padding: 8px 10px;">Ultrasonic noise suppressed past -120 dB, extreme pitch-black noise floor.</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <!-- Section 3: Ultrasonic Protection Filter Benchmarks -->
              <h4 style="color: var(--text-main); margin-top: 22px; margin-bottom: 8px;">3. Ultrasonic Noise Protection Filter Benchmarks</h4>
              <p style="font-size: 12.5px; margin-bottom: 8px;">
                DSD 1-bit modulation pushes shaped quantization noise into frequencies above 20kHz. Sensitive amplifiers, vacuum tube stages, op-amps, or ribbon tweeters subjected to high RF energy can suffer from intermodulation distortion (IMD) or thermal buildup.
                rooPlayer integrates a <strong>2nd-Order Butterworth Low-Pass Protection Filter</strong> with maximally flat passband (Q = 0.707, 0 dB ripple) to safely shield equipment:
              </p>

              <!-- Table 3-1: Ultrasonic Filter Specs -->
              <div style="overflow-x: auto; margin-bottom: 16px;">
                <div style="font-weight: 600; font-size: 12px; color: var(--text-main); margin-bottom: 4px;">Table 3-1: Ultrasonic Protection Filter Measured Attenuation &amp; Target Applications</div>
                <table style="width: 100%; border-collapse: collapse; font-size: 12px; text-align: left;">
                  <thead>
                    <tr style="border-bottom: 1px solid var(--border-color); color: var(--text-main);">
                      <th style="padding: 8px 10px;">Setting</th>
                      <th style="padding: 8px 10px;">-3dB Cutoff</th>
                      <th style="padding: 8px 10px;">Passband (0~20kHz)</th>
                      <th style="padding: 8px 10px;">50 kHz Atten.</th>
                      <th style="padding: 8px 10px;">100 kHz Atten.</th>
                      <th style="padding: 8px 10px;">Recommended Equipment &amp; Pairing</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: var(--text-muted);">None (Bypass)</td>
                      <td style="padding: 8px 10px;">None (Direct)</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.00 dB</td>
                      <td style="padding: 8px 10px;">0.00 dB</td>
                      <td style="padding: 8px 10px;">0.00 dB</td>
                      <td style="padding: 8px 10px;"><strong>Full-band Passthrough</strong>: For modern flagship DACs with robust internal analog filtering.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #fbbf24;">20 kHz</td>
                      <td style="padding: 8px 10px;">20,000 Hz</td>
                      <td style="padding: 8px 10px;">-3.01 dB (@20k)</td>
                      <td style="padding: 8px 10px; color: #34d399;">-16.0 dB</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-28.0 dB</td>
                      <td style="padding: 8px 10px;"><strong>Maximum Protection</strong>: Shields sensitive tube amps, vintage R2R DACs, or delicate ribbon tweeters.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #34d399;">30 kHz (Recommended)</td>
                      <td style="padding: 8px 10px;">30,000 Hz</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.85 dB (@20k)</td>
                      <td style="padding: 8px 10px; color: #34d399;">-9.5 dB</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-21.0 dB</td>
                      <td style="padding: 8px 10px;"><strong>Golden Audiophile Balance</strong>: Preserves air and micro-harmonics under 20k while dropping 100k noise by 21 dB.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #38bdf8;">40 kHz</td>
                      <td style="padding: 8px 10px;">40,000 Hz</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.30 dB (@20k)</td>
                      <td style="padding: 8px 10px;">-4.8 dB</td>
                      <td style="padding: 8px 10px;">-16.0 dB</td>
                      <td style="padding: 8px 10px;"><strong>Extended Treble Shelf</strong>: Virtually transparent passband while reducing downstream RF IMD.</td>
                    </tr>
                    <tr>
                      <td style="padding: 8px 10px; font-weight: 600; color: #a78bfa;">50 kHz</td>
                      <td style="padding: 8px 10px;">50,000 Hz</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.12 dB (@20k)</td>
                      <td style="padding: 8px 10px;">-3.0 dB</td>
                      <td style="padding: 8px 10px;">-12.0 dB</td>
                      <td style="padding: 8px 10px;"><strong>Ultra-High Extension</strong>: Unrestricted treble air, shaving off only extreme far-field noise.</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <!-- Section 4: Quantization Dither & Noise Shaping Benchmarks -->
              <h4 style="color: var(--text-main); margin-top: 22px; margin-bottom: 8px;">4. Quantization Dither &amp; Noise Shaping: Principles &amp; Benchmarks</h4>
              <p style="font-size: 12.5px; margin-bottom: 8px;">
                When reducing 64-bit floating-point audio to 24-bit/16-bit PCM or feeding a 1-bit Delta-Sigma DSD modulator, wordlength reduction is required.
                Without dither, quantization error correlates strongly with the input signal, creating severe harmonic distortion and audible harshness at low signal levels.
                By Lipshitz &amp; Vanderkooy's Theorem, applying the correct Probability Density Function (PDF) completely decorrelates error from signal:
                <br>• <strong>TPDF (Triangular PDF)</strong>: Two independent uniform distributions subtracted (2 LSB pk-pk). Error correlation $r \to 0.000$, harmonic distortion drops by 17.8 dB, representing the reference studio standard.
                <br>• <strong>HP-TPDF (High-Pass TPDF)</strong>: $H(z) = 1 - z^{-1}$ high-pass differencing, depressing in-band noise in the ear's sensitive 1kHz~4kHz range by <strong>4.27 dB</strong>.
                <br>• <strong>Noise-Shaped (F-Weighted)</strong>: 3rd-order error feedback loop with deep psychoacoustic notch at 3.5kHz~4kHz, depressing perceived sensitive-band noise by <strong>14.87 dB</strong> for an ultra-black background.
                <br>• <strong>Gauss (Gaussian Dither)</strong>: Unbounded Box-Muller distribution that eliminates Delta-Sigma integrator limit cycles and audible DC idle tones.
              </p>

              <!-- Table 4-1: Low-Level Linearity & Distortion Table -->
              <div style="overflow-x: auto; margin-bottom: 12px;">
                <div style="font-weight: 600; font-size: 12px; color: var(--text-main); margin-bottom: 4px;">Table 4-1: Low-Level Linearity, THD &amp; Error Correlation (16-bit PCM @ 1kHz Sine)</div>
                <table style="width: 100%; border-collapse: collapse; font-size: 12px; text-align: left;">
                  <thead>
                    <tr style="border-bottom: 1px solid var(--border-color); color: var(--text-main);">
                      <th style="padding: 8px 10px;">Dither Mode</th>
                      <th style="padding: 8px 10px;">-80dBFS THD</th>
                      <th style="padding: 8px 10px;">-80dBFS Corr. r</th>
                      <th style="padding: 8px 10px;">-90dBFS THD</th>
                      <th style="padding: 8px 10px;">-90dBFS Corr. r</th>
                      <th style="padding: 8px 10px;">1k~4kHz Noise Floor</th>
                      <th style="padding: 8px 10px;">Characteristics</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: var(--text-muted);">None (Truncation)</td>
                      <td style="padding: 8px 10px; color: #ef4444;">6.28% (-24.0dB)</td>
                      <td style="padding: 8px 10px; color: #ef4444;">0.1682</td>
                      <td style="padding: 8px 10px; color: #ef4444; font-weight: 600;">26.79% (-11.4dB)</td>
                      <td style="padding: 8px 10px; color: #ef4444;">0.2267</td>
                      <td style="padding: 8px 10px;">-105.95 dBFS</td>
                      <td style="padding: 8px 10px; color: #ef4444;">Severe odd-harmonic distortion</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #38bdf8;">TPDF (Triangular)</td>
                      <td style="padding: 8px 10px; color: #34d399;">1.06% (-39.5dB)</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">0.0009</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">3.44% (-29.3dB)</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">0.0028</td>
                      <td style="padding: 8px 10px;">-99.15 dBFS</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>17.8dB THD drop</strong>, zero distortion</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #a78bfa;">HP-TPDF (High-Pass)</td>
                      <td style="padding: 8px 10px; color: #34d399;">1.50% (-36.5dB)</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.0230</td>
                      <td style="padding: 8px 10px; color: #34d399;">4.60% (-26.7dB)</td>
                      <td style="padding: 8px 10px;">0.0334</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-103.42 dBFS</td>
                      <td style="padding: 8px 10px;">Sensitive band <strong>4.27dB quieter</strong></td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #34d399;">Noise-Shaped (F-Weight)</td>
                      <td style="padding: 8px 10px; color: #34d399;">1.37% (-37.3dB)</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">0.0001</td>
                      <td style="padding: 8px 10px; color: #34d399;">3.45% (-29.2dB)</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">0.0001</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">-114.02 dBFS</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>14.87dB reduction</strong>, inky black background</td>
                    </tr>
                    <tr>
                      <td style="padding: 8px 10px; font-weight: 600; color: #fbbf24;">Gauss (Gaussian)</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.95% (-40.4dB)</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.0064</td>
                      <td style="padding: 8px 10px; color: #34d399; font-weight: 600;">3.26% (-29.7dB)</td>
                      <td style="padding: 8px 10px;">0.0047</td>
                      <td style="padding: 8px 10px;">-97.85 dBFS</td>
                      <td style="padding: 8px 10px; color: #fbbf24;"><strong>DSD limit cycles eliminated</strong>, warm analog tone</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <!-- Section 5: Audiophile Synergy Matching Guide -->
              <h4 style="color: var(--text-main); margin-top: 22px; margin-bottom: 8px;">5. Full-Chain Audiophile Acoustic Synergy Matching Guide</h4>
              <ul>
                <li><strong>Ultimate Master Monitoring (Flagship Recommended)</strong>: <code>Precise, Linear Phase</code> &rarr; <code>7th-Order (CLANS)</code> &rarr; <code>TPDF / Gauss</code> &rarr; <code>DSD128 / DSD256</code> &rarr; <code>30k / 40k</code>. Absolute zero phase distortion, &gt;140dB stopband rejection, pitch-black background, surgical imaging. Perfect for orchestral works and master recordings.</li>
                <li><strong>Inky Black Background &amp; Micro-Detail Retrieval</strong>: <code>Precise, Linear Phase</code> &rarr; <code>4x / 8x PCM</code> &rarr; <code>Noise-Shaped Dither</code> &rarr; <code>30k / 40k</code>. 3rd-order psychoacoustic shaping pushes sensitive-band noise to -114 dBFS, exposing delicate room acoustic decay and micro-details.</li>
                <li><strong>Rhythmic Drive &amp; Explosive Percussion Attack</strong>: <code>Precise, Minimum Phase</code> &rarr; <code>7th-Order (CLANS)</code> &rarr; <code>HP-TPDF Dither</code> &rarr; <code>DSD128</code> &rarr; <code>30k</code>. Zero pre-ringing concentrates energy right at the transient onset. Tight, punchy bass, explosive dynamic slam, and cohesive rhythm.</li>
                <li><strong>Lush Audiophile Vocals &amp; Acoustic Strings</strong>: <code>Smooth, Linear Phase</code> &rarr; <code>5th-Order (CRFB)</code> &rarr; <code>Gauss Dither</code> &rarr; <code>DSD128 / DSD256</code> &rarr; <code>30k</code>. Ringing duration shortened by &gt;75%, velvety smooth treble, lush natural overtones without high-frequency digital hardness.</li>
                <li><strong>Classic Analog &amp; Vinyl Warmth</strong>: <code>Smooth, Minimum Phase</code> &rarr; <code>5th-Order (CRFB)</code> &rarr; <code>Gauss Dither</code> &rarr; <code>DSD64 / DSD128</code> &rarr; <code>20k / 30k</code>. Zero pre-ringing plus minimal post-decay eliminates digital harshness, recreating the organic, fatigue-free warmth of vintage vinyl and classic SACDs.</li>
                <li><strong>Pure Direct PCM Oversampling</strong>: <code>Precise, Linear Phase</code> &rarr; <code>4x Integer Rate (176.4k / 192k)</code> &rarr; <code>TPDF Dither</code> &rarr; <code>None / 30k</code>. Direct polyphase FIR upsampling passed directly to DAC chips, delivering open clarity and spacious imaging.</li>
              </ul>
            </div>`,
  };

  window.I18N_DOC_EN = DOC_EN;
})();
