# rooPlayer - Audiophile Music Media Manager (Rust Native Edition)

基於 Rust 語言與 Tauri 2 全新移植的發燒級高音質音樂管理及播放程式，完全保持與原版一致的發燒級界面與強大功能，並徹底釋放本機計算極限。

---

## 核心技術特色與架構

1. **純 Rust 發燒級 DSP 升頻核心 (`crates/roo-dsp`)**：
   - 內建 511-Taps `poly-sinc-gauss-long` 旗艦高斯多相 FIR 濾波器。
   - `pure-flat`（-160dB 止帶衰減，通帶 0~21.5kHz 絕對平坦）濾波器。
   - Kaiser-Sinc、Gaussian、Hermite 多項式插值。
   - 透過 FFT 希爾伯特轉換（Hilbert Transform）計算最小相位（Minimum Phase）。
   - 支援使用者自訂濾波器（窗函數、截止頻率、Taps 自由定義與即時響應頻譜圖預覽）。
   - 5 階與 7 階 CIFFR（Cascade of Integrators with Feedforward and Resonators）Delta-Sigma 1-bit 調變器（支援 DSD64、DSD128、DSD256、DSD512），搭配 TPDF / 高斯顫音（Dither）與 20k~50kHz 聲學超音波保護濾波器。
   - 參數化等化器（PEQ）：Direct Form II Transposed 雙二階濾波器（Peaking, Shelves, Highpass, Lowpass, Notch, Bandpass）。
   - 多段式 FFT 分割卷積引擎（Partitioned Overlap-Save FFT Convolution），支援空間校正脈衝響應（IR）。

2. **原生音訊解碼庫 (`crates/roo-decoder`)**：
   - 原生支援 WAV（PCM 16/24/32-bit 整數、32/64-bit IEEE 浮點數）。
   - 原生支援 AIFF / AIFC（包含 80-bit IEEE 擴充浮點採樣率轉換）。
   - 原生支援 DSF / DFF（SACD 原始 DSD 位元流解構，支援即時降採樣 Decimation 轉 24-bit / 88.2kHz WAV 串流）。
   - 內建 TIDAL CMAF ISO-BMFF 容器無損解封器（直接將 `ftyp` CMAF 封裝解為符合標準 libFLAC 的 Raw FLAC）。
   - 支援 FLAC、MP3、AAC、M4A 等通用格式解碼。

3. **原生 Windows WASAPI 獨佔輸出 (`crates/roo-output`)**：
   - 使用 Windows COM 原生 API（`IAudioClient`, `IAudioRenderClient`）直接在進程內以獨佔模式傳輸（AUDCLNT_SHAREMODE_EXCLUSIVE）。
   - 捨棄原版依賴 C# 外部行程編譯的作法，達成零 IPC 耗損、極致位元完美（Bit-Perfect）播放。
   - 支援 44.1kHz 至 768kHz+ 多檔位硬體自動採樣率切換與事件回呼緩衝。

4. **漸進式音訊串流伺服器 (`crates/roo-streamer`)**：
   - 內嵌高效非阻塞 HTTP 串流引擎，支援 HTTP 206 Partial Content 與 Range 分段讀取。
   - 即時在線升頻輸出 PCM WAV 或調變輸出 DSF 位元流。

5. **DLNA / uPNP 串流子系統 (`crates/roo-dlna`)**：
   - 支援 SSDP 區域網路廣播設備發現（URN: `MediaRenderer:1`）。
   - 完整的 SOAP / UPnP AVTransport 與 RenderingControl 控制管線（Play, Pause, Stop, Seek, Volume, GetPositionInfo）。
   - 支援串流路徑至串流機與外部發燒 DAC。

6. **本地與網路音樂庫索引 (`crates/roo-library`)**：
   - 高並發多執行緒資料夾與 Windows Samba / UNC 網路硬碟路徑掃描。
   - 高速提取音訊標籤與內嵌專輯封面並快取。
   - SQLite 本機資料庫持久化曲庫、專輯、演出者、播放清單（M3U/M3U8）。

7. **100% 還原的前端界限與外觀 (`frontend/`)**：
   - 完整保留原版高音質發燒界面、黑金與毛玻璃視覺風格、自訂濾波器響應 Canvas、頻譜分析儀、等化器等。
   - 透過 `frontend/tauri-shim.js` 提供完整的 `window.electronAPI` 兼容墊片，前端邏輯無縫運作。

---

## 啟動與編譯

### 1. 直接啟動
在根目錄雙擊執行：
```bat
啟動軟體.bat
```

### 2. 開發模式
```bash
cargo run -p rooplayer
```

### 3. 發行編譯
```bash
cargo build --release -p rooplayer
```
編譯產物位於 `target/release/rooplayer.exe`，為單一獨立極速本機執行檔（無需安裝 Node.js 或 Electron）。
