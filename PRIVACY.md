# Privacy Policy / 隱私權政策 — rooPlayer

Effective Date / 生效日期：2026-11-27  
Publisher / 發行者：tzungli  
Repository / 專案網址：https://github.com/tzungli/roop2

---

## English

**rooPlayer** respects your privacy. We do not collect, sell, rent, or transmit personal data, usage analytics, or telemetry to any first-party or third-party servers.

### 1. Local Data Storage
All application data is stored strictly on your local device:
- Music library index, metadata, playlists, favorites, and playback history (local SQLite database / `localStorage`).
- DSP settings, custom FIR filter coefficients, and per-zone audio profiles.
- Discovered LAN DLNA/UPnP and AirPlay renderer endpoints cached locally for faster device reconnects.

### 2. Optional Network Access
rooPlayer only initiates network connections when you use specific features:
- **Local Area Network (LAN) Discovery, Streaming & Web Display**: SSDP/mDNS/HTTP/RTSP/SMB requests within your local network to discover and stream audio to your DLNA/UPnP renderers, AirPlay devices, NAS/SMB folders, or serve the optional fullscreen Web Display (`/display/`) to browsers on your LAN.
- **Online Lyrics & Metadata Lookup**: Queries public music databases (such as LRCLIB, NetEase Cloud Music, Apple Music/iTunes Search, MusicBrainz/Wikidata, Wikipedia, Douban, and Baidu Baike) using only the track/album/artist title you are viewing or playing.
- **Third-Party Streaming (Optional)**: If you choose to connect your TIDAL or KKBOX account, authentication tokens are stored locally on your device and used strictly to fetch streams from those services according to their respective privacy policies.

### 3. Permissions
- **Local Files / Music Folders**: Used exclusively to read audio files, embedded metadata, album artwork, and convolution impulse response (IR) WAV files selected by you.
- **Network Access**: Used only for LAN audio streaming and the optional online lookups listed above.

### 4. Contact
For privacy questions or support, please open an issue at:  
https://github.com/tzungli/roop2/issues

---

## 繁體中文

**rooPlayer** 重視並保護您的隱私。本軟體**不會**蒐集、出售、出租或向任何開發者或第三方伺服器傳送您的個人資料、使用遙測數據或廣告追蹤資訊。

### 1. 本機資料儲存
所有應用程式資料皆僅儲存於您自己的裝置本機中：
- 本機音樂庫索引、曲目中繼資料、播放清單、我的最愛與播放紀錄（本機 SQLite 資料庫與 `localStorage`）。
- DSP 升降頻設定、自訂 FIR 濾波器參數與各輸出區域音訊設定檔。
- 曾搜尋到的區網 DLNA / UPnP 與 AirPlay 裝置位址快取（僅供加速連線使用）。

### 2. 選用性網路連線行為
僅在您主動使用下列功能時，本軟體才會發起對應網路連線：
- **區域網路裝置探索、串流與網頁顯示器**：在您的區網內透過 SSDP / mDNS / HTTP / RTSP / SMB 探索並串流無損音訊至您的 DLNA / UPnP 音響、AirPlay 裝置或 SMB 網路資料夾，或於區網提供選用之全螢幕網頁顯示器（`/display/`）。
- **線上歌詞與專輯資料查詢**：僅以您正在播放或檢視的曲名、專輯與藝術家名稱向公開音樂資料庫（如 LRCLIB、網易雲、Apple Music / iTunes、MusicBrainz / Wikidata、維基百科、豆瓣與百度百科）查詢歌詞、曲風或簡介。
- **第三方串流服務（選用）**：若您主動登入 TIDAL 或 KKBOX 帳號，相關授權憑證僅儲存於您的本機裝置上，並僅用於向該服務請求串流與收藏資料。

### 3. 系統權限用途
- **本機檔案存取**：僅用於讀取您手動選取的音樂資料夾、內嵌封面／標籤與卷積脈衝響應（IR）WAV 檔案。
- **網路存取**：僅用於上述區網音訊串流及選用之線上資料查詢。

### 4. 聯絡方式
若有任何隱私疑問或技術支援需求，請透過 GitHub Issues 聯繫：  
https://github.com/tzungli/roop2/issues
