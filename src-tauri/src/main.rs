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

#[tokio::main]
async fn main() {
    std::env::set_var(
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,WinHttpAutoProxySupport,NetworkPrediction --disable-background-networking --disable-component-update --js-flags=\"--max-old-space-size=512\" --renderer-process-limit=1"
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
    let tidal_state = TidalState::new(app_data_dir);

    tauri::Builder::default()
        .setup(|app| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.hide();
            }
            let handle = app.handle().clone();
            tokio::spawn(async move {
                // Emergency fallback: reveal main window after 12s only if frontend failed to reveal it
                tokio::time::sleep(std::time::Duration::from_millis(12000)).await;
                if let Some(win) = handle.get_webview_window("main") {
                    if let Ok(visible) = win.is_visible() {
                        if !visible {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Resized(_) = event {
                if let Ok(is_max) = window.is_maximized() {
                    let _ = window.emit("window:maximizedChange", is_max);
                }
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
            tidal_login_with_window,
            tidal_api,
            tidal_get_stream_url,
            tidal_cache_track,
        ])
        .run(tauri::generate_context!())
        .expect("error while running rooPlayer application");
}
