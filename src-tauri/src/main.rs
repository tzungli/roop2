#![windows_subsystem = "windows"]

mod commands;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tracing_subscriber::EnvFilter;

use commands::audio::*;
use commands::dlna::*;
use commands::dsp::*;
use commands::exclusive::*;
use commands::http::*;
use commands::library::*;
use commands::tidal::*;
use commands::window::*;

use roo_dlna::DlnaManager;
use roo_library::LibraryDatabase;
use roo_output::ExclusiveController;
use roo_streamer::StreamServer;

#[cfg(target_os = "windows")]
mod win_mem {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn SetProcessWorkingSetSize(
            h_process: *mut std::ffi::c_void,
            dw_minimum_working_set_size: usize,
            dw_maximum_working_set_size: usize,
        ) -> i32;
    }

    pub fn trim_working_set() {
        unsafe {
            let handle = GetCurrentProcess();
            SetProcessWorkingSetSize(handle, usize::MAX, usize::MAX);
        }
    }
}

#[tokio::main]
async fn main() {
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,WinHttpAutoProxySupport,NetworkPrediction --disable-background-networking --disable-component-update --disk-cache-size=33554432 --media-cache-size=33554432 --js-flags=\"--max-old-space-size=1024\" --renderer-process-limit=1"
    );

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    // 1. Start background progressive HTTP audio streamer (Port 39840)
    tokio::spawn(async {
        let server = StreamServer::new(39840);
        if let Err(e) = server.run().await {
            tracing::error!("StreamServer error: {}", e);
        }
    });

    // 2. Resolve application data directory
    let app_data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rooPlayer");
    let _ = std::fs::create_dir_all(&app_data_dir);

    let covers_dir = app_data_dir.join("covers");
    let _ = std::fs::create_dir_all(&covers_dir);

    let db_path = app_data_dir.join("library.db");
    let db = match LibraryDatabase::open(&db_path) {
        Ok(d) => Arc::new(d),
        Err(e) => {
            tracing::warn!("Failed to open library.db at {:?}: {}, fallback to in-memory", db_path, e);
            Arc::new(LibraryDatabase::in_memory().unwrap())
        }
    };

    let library_state = LibraryState {
        db,
        covers_dir,
    };

    let exclusive_controller = ExclusiveController::new();
    let dlna_manager = DlnaManager::new();
    let window_state_path = app_data_dir.join("window_state.json");
    let tidal_state = TidalState::new(app_data_dir);

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct WindowSizeState {
        width: f64,
        height: f64,
        #[serde(default)]
        maximized: bool,
    }

    let initial_win_state: Option<WindowSizeState> = std::fs::read_to_string(&window_state_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    let saved_win_state = Arc::new(std::sync::Mutex::new(
        initial_win_state.clone().unwrap_or(WindowSizeState {
            width: 1280.0,
            height: 820.0,
            maximized: false,
        }),
    ));
    let window_restored = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let setup_restored_flag = window_restored.clone();
    let setup_initial_state = initial_win_state.clone();
    let event_win_state = saved_win_state.clone();
    let event_state_path = window_state_path.clone();
    let event_restored_flag = window_restored.clone();

    tauri::Builder::default()
        .setup(move |app| {
            if let Some(win) = app.get_webview_window("main") {
                if let Some(icon) = app.default_window_icon() {
                    let _ = win.set_icon(icon.clone());
                }
                let _ = win.set_min_size(Some(tauri::Size::Logical(tauri::LogicalSize {
                    width: 840.0,
                    height: 680.0,
                })));

                // Restore last saved window size before revealing window
                let _ = win.hide();
                if let Some(st) = setup_initial_state {
                    let mut target_w = st.width.max(840.0);
                    let mut target_h = st.height.max(680.0);
                    if let Ok(Some(monitor)) = win.current_monitor() {
                        let scale = monitor.scale_factor().max(0.5);
                        let max_w = (monitor.size().width as f64 / scale).max(840.0);
                        let max_h = (monitor.size().height as f64 / scale).max(680.0);
                        target_w = target_w.min(max_w);
                        target_h = target_h.min(max_h);
                    }
                    let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize {
                        width: target_w,
                        height: target_h,
                    }));
                    let _ = win.center();
                    if st.maximized {
                        commands::window::mark_pending_maximize();
                    }
                }
                setup_restored_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            let handle = app.handle().clone();
            let dlna_warm_handle = app.handle().clone();
            tokio::spawn(async move {
                // Fast warm-probe of previously used DLNA devices on startup (< 100ms)
                let mgr = dlna_warm_handle.state::<DlnaManager>();
                let _ = commands::dlna::warm_probe_known_dlna(&dlna_warm_handle, &mgr).await;
            });
            tokio::spawn(async move {
                // Emergency fallback: reveal main window after 2.5s only if frontend failed to reveal it
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
                if let Some(win) = handle.get_webview_window("main") {
                    if let Ok(visible) = win.is_visible() {
                        if !visible {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                }
            });

            #[cfg(target_os = "windows")]
            tokio::spawn(async {
                // Periodic memory compaction & physical working set trimming (every 45s)
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(45));
                loop {
                    interval.tick().await;
                    win_mem::trim_working_set();
                }
            });
            Ok(())
        })
        .on_window_event(move |window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                    if let Ok(guard) = event_win_state.lock() {
                        if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                            let _ = std::fs::write(&event_state_path, json);
                        }
                    }
                    static SHUTDOWN_STOPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                    if !SHUTDOWN_STOPPED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        api.prevent_close();
                        roo_streamer::cancel_all_progressive_streams();
                        let win = window.clone();
                        tauri::async_runtime::spawn(async move {
                            let dlna = win.state::<DlnaManager>();
                            dlna.stop_active().await;
                            let _ = win.close();
                        });
                    }
                }
                tauri::WindowEvent::Resized(phys_size) if window.label() == "main" => {
                    let is_min = window.is_minimized().unwrap_or(false)
                        || phys_size.width == 0
                        || phys_size.height == 0;
                    let is_max = window.is_maximized().unwrap_or(false);

                    if !is_min {
                        if let Ok(scale) = window.scale_factor() {
                            let log_w = phys_size.width as f64 / scale;
                            let log_h = phys_size.height as f64 / scale;
                            if !is_max && (log_w < 840.0 || log_h < 680.0) {
                                let clamped_w = log_w.max(840.0);
                                let clamped_h = log_h.max(680.0);
                                let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                                    width: clamped_w,
                                    height: clamped_h,
                                }));
                            } else if event_restored_flag.load(std::sync::atomic::Ordering::SeqCst) {
                                if let Ok(mut guard) = event_win_state.lock() {
                                    let mut changed = false;
                                    if is_max {
                                        if !guard.maximized {
                                            guard.maximized = true;
                                            changed = true;
                                        }
                                    } else {
                                        let rw = log_w.round();
                                        let rh = log_h.round();
                                        if (guard.width - rw).abs() >= 1.0
                                            || (guard.height - rh).abs() >= 1.0
                                            || guard.maximized
                                        {
                                            guard.width = rw;
                                            guard.height = rh;
                                            guard.maximized = false;
                                            changed = true;
                                        }
                                    }
                                    if changed {
                                        if let Ok(json) = serde_json::to_string_pretty(&*guard) {
                                            let _ = std::fs::write(&event_state_path, json);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let _ = window.emit("window:maximizedChange", is_max);
                }
                _ => {}
            }
        })
        .manage(library_state)
        .manage(exclusive_controller)
        .manage(dlna_manager)
        .manage(tidal_state)
        .invoke_handler(tauri::generate_handler![
            // Native HTTP request command
            native_http_request,
            // Window commands
            window_show,
            window_minimize,
            window_maximize_toggle,
            window_is_maximized,
            window_start_dragging,
            window_close,
            // Audio commands
            audio_get_dsd_stream_url,
            audio_get_sox_stream_url,
            audio_get_duration,
            audio_get_spectrum,
            conv_get_ir_info,
            conv_pick_ir_file,
            // DSP commands
            dsp_list_filter_plugins,
            filters_list_slots,
            filters_save_slot,
            filters_delete_slot,
            filters_preview,
            capture_step,
            // WASAPI Exclusive commands
            exclusive_probe,
            exclusive_start,
            exclusive_pause,
            exclusive_resume,
            exclusive_seek,
            exclusive_stop,
            exclusive_state,
            // DLNA commands
            dlna_get_devices,
            dlna_scan,
            dlna_probe_ip,
            dlna_select_device,
            dlna_play_track,
            dlna_pause,
            dlna_resume,
            dlna_stop,
            dlna_seek,
            dlna_set_volume,
            dlna_fake_renderer_actions,
            // Library & Samba commands
            dialog_select_folder,
            dialog_select_network_folder,
            samba_test_share,
            samba_connect_and_scan,
            samba_discover_servers,
            library_scan_folder,
            library_load_saved,
            library_save_tracks,
            library_remove_folder,
            library_update_album_artist,
            library_export,
            library_import,
            playlist_read_file,
            playlist_write_file,
            shell_open_external,
            util_translate_to_zh_tw,
            // TIDAL commands
            tidal_get_stored_auth,
            tidal_save_auth,
            tidal_logout,
            tidal_probe_auth,
            tidal_focus_login_window,
            tidal_login_with_window,
            tidal_silent_refresh,
            tidal_api,
            tidal_get_stream_url,
            tidal_cache_track,
        ])
        .run(tauri::generate_context!())
        .expect("error while running rooPlayer application");
}
