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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("error"),
    )
    .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = sessions_dir();
            std::fs::create_dir_all(&dir).ok();
            let service = Service::new(dir, app.handle().clone());
            app.manage(service);
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
