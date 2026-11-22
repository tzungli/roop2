// tauri-shim.js — rooPlayer Tauri 2 Compatibility Bridge
// Implements 100% of window.electronAPI so the original frontend works unmodified!
(function () {
  'use strict';

  function getTauriCore() {
    if (window.__TAURI__ && window.__TAURI__.core) {
      return window.__TAURI__.core;
    }
    if (window.__TAURI_INTERNALS__) {
      return window.__TAURI_INTERNALS__;
    }
    return null;
  }

  function getTauriEvent() {
    if (window.__TAURI__ && window.__TAURI__.event) {
      return window.__TAURI__.event;
    }
    return null;
  }

  async function invoke(cmd, args = {}) {
    let core = getTauriCore();
    if (!core) {
      for (let i = 0; i < 30; i++) {
        await new Promise((r) => setTimeout(r, 20));
        core = getTauriCore();
        if (core) break;
      }
    }
    if (core && core.invoke) {
      try {
        return await core.invoke(cmd, args);
      } catch (err) {
        console.error(`[TauriShim] invoke "${cmd}" failed:`, err);
        throw err;
      }
    }
    console.warn(`[TauriShim] invoke "${cmd}" called before Tauri initialized:`, args);
    return null;
  }

  function listen(eventName, callback) {
    const eventModule = getTauriEvent();
    if (eventModule && eventModule.listen) {
      eventModule.listen(eventName, (event) => {
        try {
          callback(event.payload);
        } catch (e) {
          console.error(`[TauriShim] error handling event "${eventName}":`, e);
        }
      });
    } else {
      let attempts = 0;
      const poll = setInterval(() => {
        attempts++;
        const em = getTauriEvent();
        if (em && em.listen) {
          clearInterval(poll);
          em.listen(eventName, (event) => {
            try {
              callback(event.payload);
            } catch (e) {
              console.error(`[TauriShim] error handling event "${eventName}":`, e);
            }
          });
        } else if (attempts > 60) {
          clearInterval(poll);
        }
      }, 50);
    }
    window.addEventListener(eventName, (e) => callback(e.detail));
  }

  // Native HTTP Fetch Bridge — Bypasses browser CORS and restricted headers for external APIs (KKBOX, iTunes, MusicBrainz, Wikipedia, etc.)
  const originalFetch = window.fetch;
  let isInsideNativeFetch = false;

  function isLocalOrInternalUrl(u) {
    if (!u || typeof u !== 'string') return true;
    const lower = u.trim().toLowerCase();
    if (lower.startsWith('/') || lower.startsWith('./') || lower.startsWith('../') ||
        lower.startsWith('blob:') || lower.startsWith('data:') || lower.startsWith('tauri:') ||
        lower.startsWith('asset:') || lower.startsWith('media:') || lower.startsWith('file:')) {
      return true;
    }
    // Tauri IPC / internal endpoints (e.g. http://ipc.localhost, http://tauri.localhost, http://127.0.0.1:39840)
    try {
      const parsed = new URL(u, window.location.href);
      const host = parsed.hostname.toLowerCase();
      if (host === 'localhost' || host === '127.0.0.1' || host.endsWith('.localhost') || host === 'ipc.localhost') {
        return true;
      }
    } catch (_) {
      if (lower.includes('localhost') || lower.includes('127.0.0.1')) return true;
    }
    return false;
  }

  window.fetch = async function (resource, init = {}) {
    let url = '';
    if (typeof resource === 'string') {
      url = resource;
    } else if (resource && typeof resource.url === 'string') {
      url = resource.url;
    }

    // Only intercept true external web APIs (api.kkbox.com, tidal.com, musicbrainz.org, etc.)
    // Never intercept local IPC, 127.0.0.1 stream server, or re-entrant calls
    if (!isInsideNativeFetch && !isLocalOrInternalUrl(url) && (url.startsWith('http://') || url.startsWith('https://'))) {
      isInsideNativeFetch = true;
      try {
        const method = (init.method || (typeof resource === 'object' && resource.method) || 'GET').toUpperCase();
        let headers = {};
        const rawHeaders = init.headers || (typeof resource === 'object' && resource.headers);
        if (rawHeaders) {
          if (typeof rawHeaders.forEach === 'function') {
            rawHeaders.forEach((val, key) => { headers[key] = val; });
          } else if (Array.isArray(rawHeaders)) {
            rawHeaders.forEach(([key, val]) => { headers[key] = val; });
          } else if (typeof rawHeaders === 'object') {
            headers = { ...rawHeaders };
          }
        }

        let body = null;
        const rawBody = init.body !== undefined ? init.body : (typeof resource === 'object' ? resource.body : null);
        if (rawBody) {
          if (typeof rawBody === 'string') {
            body = rawBody;
          } else if (rawBody instanceof URLSearchParams) {
            body = rawBody.toString();
            if (!headers['Content-Type'] && !headers['content-type']) {
              headers['Content-Type'] = 'application/x-www-form-urlencoded';
            }
          } else if (typeof rawBody === 'object') {
            body = JSON.stringify(rawBody);
            if (!headers['Content-Type'] && !headers['content-type']) {
              headers['Content-Type'] = 'application/json';
            }
          }
        }

        const res = await invoke('native_http_request', {
          request: {
            url,
            method,
            headers,
            body
          }
        });

        if (res && typeof res.status === 'number') {
          return new Response(res.body, {
            status: res.status,
            statusText: res.status >= 200 && res.status < 300 ? 'OK' : 'HTTP Error',
            headers: new Headers(res.headers)
          });
        }
      } catch (err) {
        console.warn('[TauriShim] Native HTTP request proxy error, fallback to browser fetch:', err);
      } finally {
        isInsideNativeFetch = false;
      }
    }

    return originalFetch.apply(this, arguments);
  };

  function normalizeShimTrack(t) {
    if (!t || typeof t !== 'object') return t;
    const p = t.filePath || t.path || '';
    t.filePath = p;
    t.path = p;
    const c = t.cover || t.coverPath || '';
    t.cover = c;
    t.coverPath = c;
    const f = t.formatName || t.format || 'PCM';
    t.formatName = f;
    t.format = f;
    return t;
  }

  window.electronAPI = {
    // Window controls
    showWindow: async () => {
      return await invoke('window_show');
    },
    startDragging: async () => {
      try {
        if (window.__TAURI__?.window?.getCurrentWindow) {
          return await window.__TAURI__.window.getCurrentWindow().startDragging();
        }
      } catch (_) {}
      return await invoke('window_start_dragging');
    },
    minimize: async () => {
      try {
        if (window.__TAURI__?.window?.getCurrentWindow) {
          return await window.__TAURI__.window.getCurrentWindow().minimize();
        }
      } catch (e) {
        console.warn('[TauriShim] getCurrentWindow.minimize fallback', e);
      }
      return await invoke('window_minimize');
    },
    isMaximized: async () => {
      try {
        if (window.__TAURI__?.window?.getCurrentWindow) {
          return await window.__TAURI__.window.getCurrentWindow().isMaximized();
        }
      } catch (_) {}
      return await invoke('window_is_maximized');
    },
    maximizeToggle: async () => {
      try {
        if (window.__TAURI__?.window?.getCurrentWindow) {
          const win = window.__TAURI__.window.getCurrentWindow();
          const isMax = await win.isMaximized();
          await win.toggleMaximize();
          window.dispatchEvent(new CustomEvent('window:maximizedChange', { detail: !isMax }));
          return !isMax;
        }
      } catch (e) {
        console.warn('[TauriShim] getCurrentWindow.maximizeToggle fallback', e);
      }
      const res = await invoke('window_maximize_toggle');
      window.dispatchEvent(new CustomEvent('window:maximizedChange', { detail: !!res }));
      return res;
    },
    close: async () => {
      try {
        if (window.__TAURI__?.window?.getCurrentWindow) {
          return await window.__TAURI__.window.getCurrentWindow().close();
        }
      } catch (e) {
        console.warn('[TauriShim] getCurrentWindow.close fallback', e);
      }
      return await invoke('window_close');
    },
    onMaximizedChange: (callback) => {
      listen('window:maximizedChange', callback);
    },

    // HTTP Request helper
    nativeHttpRequest: (request) => invoke('native_http_request', { request }),

    // Library & File Dialogs
    selectFolder: () => invoke('dialog_select_folder'),
    selectNetworkFolder: () => invoke('dialog_select_network_folder'),
    connectSambaShare: (params) => invoke('samba_connect_and_scan', { params }),
    testSambaShare: (uncPath) => invoke('samba_test_share', { uncPath }),
    discoverSambaServers: () => invoke('samba_discover_servers'),
    scanFolder: async (folderPath) => {
      const res = await invoke('library_scan_folder', { folderPath });
      if (res && Array.isArray(res.tracks)) {
        res.tracks.forEach(normalizeShimTrack);
      }
      if (res && Array.isArray(res.scannedTracks)) {
        res.scannedTracks.forEach(normalizeShimTrack);
      }
      return res;
    },
    onScanProgress: (callback) => listen('library:scanProgress', callback),
    loadSavedLibrary: async () => {
      const res = await invoke('library_load_saved');
      if (res && Array.isArray(res.tracks)) {
        res.tracks.forEach(normalizeShimTrack);
      }
      return res;
    },
    saveLibraryTracks: (tracks) => {
      if (Array.isArray(tracks)) {
        tracks.forEach(normalizeShimTrack);
      }
      return invoke('library_save_tracks', { tracks });
    },
    exportLibrary: (defaultFilename, extraData) => invoke('library_export', { defaultFilename, extraData }),
    importLibrary: async () => {
      const res = await invoke('library_import');
      if (res && res.data && Array.isArray(res.data.tracks)) {
        res.data.tracks.forEach(normalizeShimTrack);
      }
      return res;
    },
    readPlaylistFile: () => invoke('playlist_read_file'),
    writePlaylistFile: (suggestedName, text, ext) => invoke('playlist_write_file', { suggestedName, text, ext }),
    removeFolder: async (folderPath) => {
      const res = await invoke('library_remove_folder', { folderPath });
      if (res && Array.isArray(res.tracks)) {
        res.tracks.forEach(normalizeShimTrack);
      }
      return res;
    },
    updateAlbumArtist: (albumTitle, artistName) => invoke('library_update_album_artist', { albumTitle, artistName }),
    openExternal: (url) => invoke('shell_open_external', { url }),
    getDsdStreamUrl: (filePath, startSec = 0, peqJson = '', convJson = '') =>
      invoke('audio_get_dsd_stream_url', { filePath, startSec, peqJson, convJson }),
    getSoxStreamUrl: (filePath, startSec = 0, sampleRate = 192000, dsdMode = '', dsdOrder = 5, filterPhase = 'linear', peqJson = '', convJson = '', ultrasonicFilter = '30k') =>
      invoke('audio_get_sox_stream_url', { filePath, startSec, sampleRate, dsdMode, dsdOrder, filterPhase, peqJson, convJson, ultrasonicFilter }),
    pickIrFile: () => invoke('conv_pick_ir_file'),
    getIrInfo: (irPath) => invoke('conv_get_ir_info', { irPath }),
    listFilterPlugins: () => invoke('dsp_list_filter_plugins'),
    listFilterSlots: () => invoke('filters_list_slots'),
    saveFilterSlot: (spec) => invoke('filters_save_slot', { spec }),
    deleteFilterSlot: (slotId) => invoke('filters_delete_slot', { slotId }),
    previewFilterSpec: (spec, srcRate, targetRate) => invoke('filters_preview', { spec, srcRate, targetRate }),
    captureStep: (n, info) => invoke('capture_step', { n, info }),
    getAudioDuration: (filePath) => invoke('audio_get_duration', { filePath }),
    translateToZhTw: (text) => invoke('util_translate_to_zh_tw', { text }),

    // TIDAL Streaming Subsystem
    tidalLoginWithWindow: () => invoke('tidal_login_with_window'),
    tidalProbeAuth: () => invoke('tidal_probe_auth'),
    tidalGetStoredAuth: () => invoke('tidal_get_stored_auth'),
    tidalSaveAuth: (data) => invoke('tidal_save_auth', { data }),
    tidalLogout: () => invoke('tidal_logout'),
    tidalApi: (options) => invoke('tidal_api', { options }),
    tidalGetStreamUrl: (trackId, playbackInfo) => invoke('tidal_get_stream_url', { trackId, playbackInfo }),
    tidalCacheTrack: (trackId, streamUrlOrInfo, optionalInfo) => invoke('tidal_cache_track', { trackId, streamUrlOrInfo, optionalInfo }),

    // DLNA / UPnP Subsystem
    getDlnaDevices: () => invoke('dlna_get_devices'),
    scanDlnaDevices: () => invoke('dlna_scan'),
    probeDlnaIp: (ip) => invoke('dlna_probe_ip', { ip }),
    selectDlnaDevice: (deviceId) => invoke('dlna_select_device', { deviceId }),
    playDlnaTrack: (deviceId, track, startSec = 0, dspOptions = null) =>
      invoke('dlna_play_track', { deviceId, track, startSec, dspOptions }),
    pauseDlna: (deviceId) => invoke('dlna_pause', { deviceId }),
    resumeDlna: (deviceId) => invoke('dlna_resume', { deviceId }),
    stopDlna: (deviceId) => invoke('dlna_stop', { deviceId }),
    seekDlna: (deviceId, seconds) => invoke('dlna_seek', { deviceId, seconds }),
    getAudioSpectrum: (token, timeSec) => invoke('audio_get_spectrum', { token, timeSec }),
    setDlnaVolume: (deviceId, volume) => invoke('dlna_set_volume', { deviceId, volume }),
    onDlnaDevicesChanged: (callback) => listen('dlna:devicesChanged', callback),
    onDlnaProgress: (callback) => listen('dlna:progress', callback),
    onDlnaTrackEnded: (callback) => listen('dlna:trackEnded', callback),
    onDlnaPrepareStatus: (callback) => listen('dlna:prepareStatus', callback),
    getFakeDlnaRendererActions: () => invoke('dlna_fake_renderer_actions'),

    // WASAPI 獨佔輸出
    exclusiveProbe: (force = false) => invoke('exclusive_probe', { force }),
    exclusiveStart: (options) => invoke('exclusive_start', { options }),
    exclusivePause: () => invoke('exclusive_pause'),
    exclusiveResume: () => invoke('exclusive_resume'),
    exclusiveSeek: (streamSeconds) => invoke('exclusive_seek', { streamSeconds }),
    exclusiveStop: () => invoke('exclusive_stop'),
    exclusiveState: () => invoke('exclusive_state'),
    onExclusiveStatus: (callback) => listen('exclusive:status', callback),

    // App Metadata
    getAppVersion: () => '0.1.0',

    // Media file URL helper
    getMediaUrl: (absolutePath) => {
      if (!absolutePath) return '';
      return `http://127.0.0.1:39840/sox-stream?file_path=${encodeURIComponent(absolutePath)}`;
    },
    getCoverUrl: (absolutePath, size = 360) => {
      if (!absolutePath) return '';
      return `http://127.0.0.1:39840/cover?path=${encodeURIComponent(absolutePath)}&size=${size}`;
    }
  };

  console.log('[TauriShim] window.electronAPI successfully initialized.');
})();
