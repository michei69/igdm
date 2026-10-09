//! IG Direct — Instagram DM desktop client (Tauri 2 + React).

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

pub mod commands;
pub mod service;
pub mod state;

use service::Service;

/// Tray icon id — the event handlers are keyed on it.
const TRAY_ID: &str = "ig-direct";

/// Set once the tray icon exists. Closing a window only hides it while there is
/// a tray to bring it back from; without this, a machine where the tray failed
/// to register (no D-Bus StatusNotifier host) would leave the app running with
/// no visible window and no way to reach it.
static TRAY_READY: AtomicBool = AtomicBool::new(false);

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

/// Bring the main window back: show it, un-minimise and focus it.
fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// Left click on the tray toggles the main window: one that is showing *and*
/// focused goes back to the tray, anything else (hidden, minimised, or merely
/// in the background) comes forward. Checking the focus as well keeps the
/// first click after switching to another app a "bring it back" instead of a
/// hide.
fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let in_front = window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false);
    if in_front {
        let _ = window.hide();
    } else {
        show_main_window(app);
    }
}

/// Always-on tray icon: left click toggles the window, right click opens the
/// Open/Exit menu. The icon lives for the whole session (Tauri keeps a
/// reference to it), which is what makes close-to-tray useful.
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open IG Direct", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &exit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("IG Direct")
        .menu(&menu)
        // Linux ignores this: the StatusNotifierItem host decides. KDE calls
        // Activate on left click (see the handler below) and ContextMenu on
        // right click, which is exactly the split the menu is for.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "exit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        });

    // The bundle icon is the window icon too; without one the tray would come
    // up as an empty spot on the panel.
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    TRAY_READY.store(true, Ordering::Relaxed);
    Ok(())
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
            // A tray that cannot be created must not keep the app from
            // starting: without it the window simply closes as it always did.
            if let Err(e) = setup_tray(app.handle()) {
                log::error!("could not create the tray icon: {e}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            let WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            // Close to tray: hide instead of destroying the window, so the
            // webview keeps running. It owns the MQTT connection and the
            // notification bridge, which is what still lets notifications
            // through while the window is away; the tray brings it back.
            // (Without a tray the window closes and the app quits as before.)
            if !TRAY_READY.load(Ordering::Relaxed) {
                return;
            }
            api.prevent_close();
            let _ = window.hide();
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
