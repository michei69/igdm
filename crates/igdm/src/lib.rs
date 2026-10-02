//! IG Direct — Instagram DM desktop client (Tauri 2 + React).

#![forbid(unsafe_code)]

use std::path::PathBuf;

use tauri::Manager;

pub mod commands;
pub mod service;
pub mod state;

use service::Service;

fn sessions_dir() -> PathBuf {
    std::env::var_os("IGDM_SESSIONS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".igdm")
                .join("sessions")
        })
}

/// Let the webview record audio.
///
/// Neither wry nor Tauri handles `WebKitWebView::permission-request`, and an
/// unhandled `WebKitUserMediaPermissionRequest` is denied by default
/// (webkitgtk.org: "When a WebKitUserMediaPermissionRequest is not handled by
/// the user, it is denied by default"). `navigator.mediaDevices.getUserMedia`
/// therefore always fails on Linux and the voice recorder never uploads
/// anything. Grants are restricted to the app's own origin, which is the only
/// document the webview ever loads.
///
/// There is no per-request type check because `WebKitPermissionRequest` is a
/// GObject interface and glib 0.18 only exposes downcasts for object types.
#[cfg(target_os = "linux")]
fn enable_media_capture(window: &tauri::WebviewWindow) {
    use webkit2gtk::{PermissionRequestExt, SettingsExt, WebViewExt};

    let result = window.with_webview(|webview| {
        let view = webview.inner();
        // Off by default on older WebKitGTK; enabling it is a no-op where the
        // distro already turns it on.
        if let Some(settings) = view.settings() {
            settings.set_enable_media_stream(true);
        }
        view.connect_permission_request(|view, request| {
            let local = view.uri().is_some_and(|uri| {
                uri.starts_with("tauri://localhost")
                    || uri.starts_with("http://tauri.localhost")
                    || uri.starts_with("https://tauri.localhost")
            });
            if local {
                request.allow();
                true
            } else {
                false
            }
        });
    });
    if let Err(e) = result {
        log::error!("could not configure webview media capture: {e}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = sessions_dir();
            // Session files carry the sessionid cookie, so the directory is
            // owner-only as well (0755 lets any local user enumerate accounts).
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                std::fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(&dir)
                    .ok();
            }
            #[cfg(not(unix))]
            std::fs::create_dir_all(&dir).ok();
            let service = Service::new(dir, app.handle().clone());
            app.manage(service);
            #[cfg(target_os = "linux")]
            if let Some(window) = app.get_webview_window("main") {
                enable_media_capture(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::login_password,
            commands::login_sessionid,
            commands::login_saved,
            commands::provide_code,
            commands::cancel_code,
            commands::logout,
            commands::refresh_inbox,
            commands::load_messages,
            commands::load_older,
            commands::thread_details,
            commands::thread_raw,
            commands::reel_info,
            commands::media_comments,
            commands::story_info,
            commands::approve_request,
            commands::search_users,
            commands::thread_for_user,
            commands::get_reaction_emojis,
            commands::save_reaction_emojis,
            commands::get_theme,
            commands::set_theme,
            commands::get_chat_themes,
            commands::set_chat_themes,
            commands::send_text,
            commands::pick_media,
            commands::send_photo,
            commands::send_photo_bytes,
            commands::send_video,
            commands::send_voice,
            commands::send_reaction,
            commands::mark_seen,
            commands::send_typing,
            commands::download_media,
            commands::fetch_image,
            commands::copy_large_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
