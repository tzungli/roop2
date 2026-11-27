# rooPlayer — Audiophile Music Media Manager

[![Release](https://img.shields.io/github/v/release/tzungli/roop2?color=6c63ff&label=Release)](https://github.com/tzungli/roop2/releases/latest)
[![Platform](https://img.shields.io/badge/Platform-Windows%20x64-0078D6)](#下載與安裝)
[![Built with](https://img.shields.io/badge/Built%20with-Rust%20%2B%20Tauri%202-dea584)](#專案架構)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)

**rooPlayer** 是一款以 **Rust + Tauri 2** 原生打造的發燒級高解析音樂管理與播放程式。結合 64-bit 浮點多相升頻與 DSD512 調變 DSP 核心、Windows WASAPI 獨佔位元完美輸出、多區域 DLNA / AirPlay 無損串流、Roon 風格 21 大類音樂庫與全螢幕區網網頁顯示器（Web Display），在極低記憶體佔用下提供純粹的 Hi-Res 聆聽體驗。

---

## 核心功能亮點

### 1. 純 Rust 64-bit 發燒級 DSP 升頻與 DSD512 調變 (`crates/roo-dsp`)
- **旗艦升頻濾波器組**：
  - `poly-sinc-ext2`、`poly-sinc-ext3 (apod)`、`poly-sinc-gauss-long`（511-Taps 高斯多相 FIR）、`sinc-M`、`sinc-L`（131,070x 分段 FFT 多相濾波器）、`pure-flat`（-160 dB 止帶衰減）、Kaiser-Sinc、Gaussian 與 Hermite 插值。
  - 支援 **線性相位 / 最小相位**（FFT 希爾伯特轉換 Hilbert Transform），並提供自訂濾波器參數與即時頻率響應預覽。
- **1-bit DSD64 ~ DSD512 調變引擎**：
  - 內建旗艦雙頻段 5 階與 7 階 Trellis 調變器 **`ASDM5EC-super 512+fs`** 與 **`ASDM7EC-super 512+fs`**（AVX2 FMA 向量化加速、Stage-1B 1.4112 MHz 抗鏡像多相 FIR、0 dBFS 軟限幅穩定防護）。
  - 另提供 5 階 / 7 階 CIFFR 調變器、TPDF / Gaussian Dither，以及可調式 20 kHz – 50 kHz 超音波保護濾波器。
- **參數化等化器（PEQ）與空間校正卷積（Convolution）**：
  - Direct Form II Transposed 雙二階濾波器（Peaking / Shelf / Highpass / Lowpass / Notch / Bandpass）。
  - 多段式 FFT 分割卷積引擎（Partitioned Overlap-Save），支援載入自訂空間校正脈衝響應（IR）WAV 檔。

### 2. 多區域獨立輸出與無損網路串流 (`crates/roo-output` / `roo-dlna` / `roo-airplay` / `roo-streamer`)
- **原生 Windows WASAPI 獨佔模式**：直接呼叫 Windows COM API（`IAudioClient` / `IAudioRenderClient`），零外部行程 IPC 耗損，支援 44.1 kHz 至 768 kHz+ 自動取樣率切換與 Bit-Perfect 輸出。
- **DLNA / UPnP 與 AirPlay 串流**：
  - 支援 SSDP / mDNS 區網探索與快取暖啟動（< 100 ms 迅速喚醒已知設備）、無縫預載（Gapless Queueing）、AirPlay 無損 ALAC 串流。
  - 各輸出區域支援獨立播放／暫停／上下曲控制與音量調節，並內建 **22 款 Hi-Fi 廠牌裝置 SVG 圖示**自動辨識（如 NAD、Nagra、MBL、ESOTERIC、TEAC、Yamaha、Dynaudio 等）。
- **漸進式 HTTP 串流伺服器**：內嵌非阻塞 HTTP 引擎（通訊埠 `39840`），支援 HTTP 206 Range 分段串流與即時 PCM / DSD 升頻輸出。

### 3. 外部顯示器：Roon 風格網頁顯示器（Web Display）
- 於 **設定 → 外部顯示器** 一鍵啟用，自動提供區網專屬網址（`http://<區網IP>:39840/display/`），支援複製連結與直接在瀏覽器開啟。
- 區網內任意電腦、平板或智慧電視瀏覽器皆可全螢幕顯示專輯封面、動態模糊背景、曲目／藝人／專輯／年份、播放區域與 Hi-Res 音質標籤、60 FPS 平滑進度條與即時同步滾動歌詞。

### 4. Roon 風格音樂庫、21 大類曲風與線上串流整合 (`crates/roo-library` / `crates/roo-decoder`)
- **原生無損解碼**：WAV（PCM 16/24/32-bit、IEEE Float 32/64-bit）、AIFF/AIFC、DSF/DFF（SACD DSD 原始位元流）、FLAC、MP3、AAC、M4A，並內建 TIDAL CMAF ISO-BMFF 無損解封器。
- **本機與 NAS / SMB 曲庫**：多執行緒高速掃描本機資料夾與 Windows Samba / UNC 共享路徑，SQLite 本機資料庫持久化索引與 M3U/M3U8 播放清單。
- **TIDAL 與 KKBOX 線上整合**：支援瀏覽、收藏與串流播放；總覽頁提供每日精選（Daily Pick）與每 3 天自動更新的個人化推薦歌單。
- **Roon 21 大類音樂類型專頁**：馬賽克曲風卡片與完整「音樂類型詳情頁」（曲風簡介、8 欄精簡專輯與藝術家列表、12 張新發行專輯），無對應本機專輯時自動由 TIDAL / KKBOX / iTunes 取得封面，並於背景自動透過 iTunes / MusicBrainz 補齊曲風標籤。
- **多來源專輯評論、藝術家肖像與同步歌詞**：整合 TIDAL / TiVo、Apple Music、MusicBrainz / Wikidata、維基百科、網易雲、豆瓣與百度百科，並提供繁簡自動轉換與 LRCLIB / 網易雲即時同步滾動歌詞。

### 5. Roon 風格設定中心、系統列與 Microsoft Store 合規
- **設定頁面**：包含「一般」、「音訊裝置」（2 欄式裝置卡片一鍵啟用／停用）與「外部顯示器」分頁。
- **桌面體驗**：支援關閉視窗縮小至 Windows 系統狀態列（Close-to-Tray）、單一執行個體保護（Single Instance）、深色／淺色雙主題與繁體中文／英文介面切換。
- **自動更新與安全驗證**：內建 GitHub Releases 版本檢查與 `SHA256SUMS.txt` 完整性校驗；透過 Microsoft Store（`\WindowsApps\`）安裝時自動停用程式內自更新並交由 Store 管理，完整符合 Microsoft Store 規範。

---

## 下載與安裝

請前往 [GitHub Releases](https://github.com/tzungli/roop2/releases/latest) 下載最新版本（目前為 **`v0.2.5`**）：

- **`rooPlayer_0.2.5_x64-setup.exe`** — Windows NSIS 安裝程式（推薦，支援自動更新與 `/S` 靜默安裝）
- **`rooPlayer_0.2.5_x64_en-US.msi`** — Windows MSI 安裝程式（支援 `/qn` 靜默安裝）
- **`rooplayer-0.2.5-windows-x64-portable.exe`** — 免安裝可攜版單一執行檔
- **`SHA256SUMS.txt`** — 各檔案 SHA-256 校驗碼

---

## 專案架構

```text
ROO-RUST-ANT2/
├── crates/
│   ├── roo-dsp/       # 64-bit 多相/FFT 升頻濾波器、ASDM5EC/7EC-super 512+fs DSD 調變、PEQ、FFT 卷積
│   ├── roo-decoder/   # WAV / AIFF / FLAC / DSF / DFF / MP3 / AAC / TIDAL CMAF 原生解碼器
│   ├── roo-output/    # Windows WASAPI 獨佔模式音訊輸出引擎
│   ├── roo-streamer/  # 漸進式 HTTP 音訊串流伺服器與全螢幕網頁顯示器 (/display/)
│   ├── roo-dlna/      # DLNA / UPnP 裝置探索與 AVTransport / RenderingControl 控制
│   ├── roo-airplay/   # AirPlay 探索與無損 ALAC 串流
│   └── roo-library/   # 本機 / SMB 多執行緒掃描、標籤解析與 SQLite 索引
├── src-tauri/         # Tauri 2 主程式、系統列、單一執行個體、自動更新與 IPC 指令
└── frontend/          # 前端介面（HTML / CSS / Vanilla JS + tauri-shim.js）
```

---

## 從原始碼編譯與打包

### 開發模式啟動
```bash
cargo run -p rooplayer
```
或在專案根目錄雙擊執行 `啟動軟體.bat`。

### 編譯獨立執行檔
```bash
cargo build --release -p rooplayer
```
產物位於 `target/release/rooplayer.exe`。

### 建置 Windows 安裝程式（NSIS / MSI）
```bash
npx --yes @tauri-apps/cli@2 build
```
產物位於 `target/release/bundle/nsis/` 與 `target/release/bundle/msi/`。

---

## 隱私權與授權

- **隱私權政策**：本軟體零遙測、零廣告追蹤，所有曲庫與設定僅儲存於本機，詳見 [PRIVACY.md](PRIVACY.md)。
- **授權條款**：詳見 [LICENSE](LICENSE)。
