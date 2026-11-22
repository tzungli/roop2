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
                  <h4>Player Controls, Spectrum &amp; Adaptive UI</h4>
                </div>
                <div class="manual-item-body">
                  <p>Immersive audiophile player and responsive window layout:</p>
                  <ul>
                    <li><strong>Audiophile Bottom Bar</strong>: Full playback controls, repeat/shuffle modes, volume slider, and live Hi-Res format badge.</li>
                    <li><strong>Full-Screen Artwork &amp; Live Spectrum</strong>: Click cover art to open full-screen view featuring large album art, real-time spectrum, and lyrics.</li>
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
              <h3>Roon Sample Rate Conversion (SRC) Filter Benchmark Report & DSD Modulator Guide</h3>
            </div>
            <div class="manual-item-body" style="font-size: 13px; line-height: 1.7; color: var(--text-sub);">
              <p>
                In digital audio sample rate conversion (SRC) and 1-bit DSD modulation architecture, <strong>the SRC filter is the first line of defense</strong> (responsible for rejecting ultrasonic spectral images generated during oversampling), while <strong>the Delta-Sigma modulator is the second line of defense</strong> (shaping quantization noise into ultra-high frequencies). When the pre-filter thoroughly attenuates ultrasonic images, the single-bit comparator operates without intermodulation distortion (IMD), resulting in a pure, pitch-black background and silky high-frequency air.
              </p>

              <h4 style="color: var(--text-main); margin-top: 18px; margin-bottom: 8px;">📊 Benchmarked Specifications and Acoustic Analysis of the 4 SRC Filters</h4>
              <div style="overflow-x: auto; margin-bottom: 16px;">
                <table style="width: 100%; border-collapse: collapse; font-size: 12px; text-align: left;">
                  <thead>
                    <tr style="border-bottom: 1px solid var(--border-color); color: var(--text-main);">
                      <th style="padding: 8px 10px;">SRC Filter</th>
                      <th style="padding: 8px 10px;">Architecture</th>
                      <th style="padding: 8px 10px;">20kHz Flatness</th>
                      <th style="padding: 8px 10px;">Stopband (22.05k)</th>
                      <th style="padding: 8px 10px;">Time-Domain Ringing</th>
                      <th style="padding: 8px 10px;">Acoustic Signature</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #38bdf8;">Precise, Linear Phase</td>
                      <td style="padding: 8px 10px;">Steep Kaiser Polyphase FIR</td>
                      <td style="padding: 8px 10px; color: #34d399;">0.000 dB (Flat)</td>
                      <td style="padding: 8px 10px; color: #34d399;">-142.55 dB (Extreme)</td>
                      <td style="padding: 8px 10px;">Symmetric (50% pre / 50% post)</td>
                      <td style="padding: 8px 10px;"><strong>Roon Default Flagship</strong>: absolute zero phase distortion, strictly constant group delay, pinpoint imaging and micro-detail resolution.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #a78bfa;">Precise, Minimum Phase</td>
                      <td style="padding: 8px 10px;">Cepstral Hilbert Spectral Factorization</td>
                      <td style="padding: 8px 10px; color: #34d399;">-0.008 dB (Flat)</td>
                      <td style="padding: 8px 10px; color: #34d399;">-140.61 dB (Extreme)</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>Zero Pre-Ringing (0.00%)</strong></td>
                      <td style="padding: 8px 10px;"><strong>Punchy Dynamic Attack</strong>: strictly zero pre-ringing, front-loaded impulse energy, tight explosive drum kicks and cohesive acoustic body.</td>
                    </tr>
                    <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                      <td style="padding: 8px 10px; font-weight: 600; color: #34d399;">Smooth, Linear Phase</td>
                      <td style="padding: 8px 10px;">Gaussian Slow Roll-off FIR</td>
                      <td style="padding: 8px 10px; color: #fbbf24;">-7.90 dB (Gentle slope)</td>
                      <td style="padding: 8px 10px;">-77.62 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>Minimal Ringing (>75% shorter)</strong></td>
                      <td style="padding: 8px 10px;"><strong>Lush & Open Soundstage</strong>: dramatic reduction in ringing artifacts, silky smooth treble, relaxed natural harmonics.</td>
                    </tr>
                    <tr>
                      <td style="padding: 8px 10px; font-weight: 600; color: #fbbf24;">Smooth, Minimum Phase</td>
                      <td style="padding: 8px 10px;">Slow Roll-off + Causal Minimum Phase</td>
                      <td style="padding: 8px 10px; color: #fbbf24;">-7.90 dB (Gentle slope)</td>
                      <td style="padding: 8px 10px;">-76.94 dB</td>
                      <td style="padding: 8px 10px; color: #34d399;"><strong>Zero Pre + Ultra-Short Post</strong></td>
                      <td style="padding: 8px 10px;"><strong>Analog Vinyl Warmth</strong>: aligns with psychoacoustic temporal masking, organically warm and non-fatiguing, vinyl-like musicality.</td>
                    </tr>
                  </tbody>
                </table>
              </div>

              <h4 style="color: var(--text-main); margin-top: 18px; margin-bottom: 8px;">🎯 Audiophile Matching Matrix</h4>
              <ul>
                <li><strong>Ultimate Master Fidelity (Roon Default)</strong>: <code>Precise, Linear Phase</code> ➔ <code>7th Order (CLANS) / 5th Order</code> ➔ <code>All Sample Rates</code>. Absolute zero phase shift, &gt;140dB stopband rejection, true-to-the-master soundstage.</li>
                <li><strong>Rhythmic Drive & Dynamic Slam</strong>: <code>Precise, Minimum Phase</code> ➔ <code>7th Order (CLANS) / 5th Order</code> ➔ <code>DSD128 / DSD256</code>. Zero pre-ringing brings unmatched speed, explosive drum impacts, and focused acoustic presence.</li>
                <li><strong>Warm Audiophile Vocals & Strings</strong>: <code>Smooth, Linear Phase</code> ➔ <code>5th Order (CRFB)</code> ➔ <code>DSD128 / DSD256</code>. 75% shorter ringing duration, silky treble, expansive and airy presentation.</li>
                <li><strong>Classic Vinyl Analog Musicality</strong>: <code>Smooth, Minimum Phase</code> ➔ <code>5th Order (CRFB)</code> ➔ <code>DSD64 / DSD128</code>. Zero pre-ringing and minimal post-decay eliminate digital harshness for organic, non-fatiguing analog warmth.</li>
              </ul>
            </div>`,
  };

  window.I18N_DOC_EN = DOC_EN;
})();
