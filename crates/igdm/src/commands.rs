//! Tauri command surface: thin wrappers over `Service` methods. Fire-and-forget
//! commands return immediately and deliver results through `igdm://event`;
//! query commands return data directly.

use base64::Engine;
use instagrapi::types::UserShort;
use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use crate::service::SharedService;

/// Reply target for `send_text` (target message id + its client context).
#[derive(Deserialize)]
pub struct ReplyRef {
    pub message_id: String,
    #[serde(default)]
    pub client_context: Option<String>,
}

/// Initial frontend state: saved sessions + configured reaction emojis +
/// theme + chat-themes toggle.
#[derive(serde::Serialize)]
pub struct BootstrapData {
    pub saved_sessions: Vec<String>,
    pub reaction_emojis: Vec<String>,
    pub theme: String,
    pub chat_themes: bool,
}

// ------------------------------------------------------------------ bootstrap

#[tauri::command]
pub async fn get_bootstrap(state: State<'_, SharedService>) -> Result<BootstrapData, String> {
    let svc = state.inner();
    let sessions = svc
        .session_files()
        .iter()
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    let (reaction_emojis, theme, chat_themes) = svc.settings_bundle();
    Ok(BootstrapData {
        saved_sessions: sessions,
        reaction_emojis,
        theme,
        chat_themes,
    })
}

// ---------------------------------------------------------------------- login

#[tauri::command]
pub fn login_password(state: State<'_, SharedService>, username: String, password: String) {
    state.login_password(username, password);
}

#[tauri::command]
pub fn login_sessionid(state: State<'_, SharedService>, sessionid: String) {
    state.login_sessionid(sessionid);
}

#[tauri::command]
pub fn login_saved(state: State<'_, SharedService>, name: String) {
    let svc = state.inner();
    let path = svc
        .session_files()
        .into_iter()
        .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(name.as_str()));
    match path {
        Some(path) => svc.login_saved(path),
        None => svc.emit_login_error(format!("Saved session '{name}' not found")),
    }
}

#[tauri::command]
pub fn provide_code(state: State<'_, SharedService>, code: String) {
    state.provide_code(&code);
}

#[tauri::command]
pub fn cancel_code(state: State<'_, SharedService>) {
    state.cancel_code();
}

#[tauri::command]
pub fn logout(state: State<'_, SharedService>) {
    state.logout();
}

// ---------------------------------------------------------------------- data

#[tauri::command]
pub fn refresh_inbox(state: State<'_, SharedService>) {
    state.refresh_inbox();
}

#[tauri::command]
pub fn load_messages(state: State<'_, SharedService>, thread_id: String, amount: i64) {
    state.load_messages(thread_id, amount);
}

#[tauri::command]
pub fn load_older(state: State<'_, SharedService>, thread_id: String, cursor: String) {
    state.load_older(thread_id, cursor);
}

#[tauri::command]
pub fn thread_details(state: State<'_, SharedService>, thread_id: String) {
    let svc = state.inner().clone();
    let thread_id2 = thread_id.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok((thread, meta, _raw)) = svc.thread_details(&thread_id).await {
            svc.emit_thread_details(thread_id2, thread, meta);
        }
    });
}

#[tauri::command]
pub async fn thread_raw(
    state: State<'_, SharedService>,
    thread_id: String,
) -> Result<Option<Value>, String> {
    Ok(state.thread_raw(&thread_id).await)
}

#[tauri::command]
pub async fn reel_info(
    state: State<'_, SharedService>,
    media_id: String,
) -> Result<Option<Value>, String> {
    Ok(state.reel_info(&media_id).await)
}

#[tauri::command]
pub async fn media_comments(
    state: State<'_, SharedService>,
    media_id: String,
    max_id: Option<String>,
) -> Result<Option<Value>, String> {
    Ok(state.media_comments(&media_id, max_id).await)
}

#[tauri::command]
pub async fn story_info(
    state: State<'_, SharedService>,
    story_id: String,
    owner_id: String,
) -> Result<Option<Value>, String> {
    Ok(state.story_info(&story_id, &owner_id).await)
}

#[tauri::command]
pub fn approve_request(state: State<'_, SharedService>, thread_id: String) {
    state.approve_request(thread_id);
}

#[tauri::command]
pub fn search_users(state: State<'_, SharedService>, query: String) {
    state.search_users(query);
}

#[tauri::command]
pub fn thread_for_user(state: State<'_, SharedService>, user: UserShort) {
    state.thread_for_user(user);
}

#[tauri::command]
pub fn get_reaction_emojis(state: State<'_, SharedService>) -> Vec<String> {
    state.load_reaction_emojis()
}

#[tauri::command]
pub fn save_reaction_emojis(state: State<'_, SharedService>, emojis: Vec<String>) {
    state.save_reaction_emojis(&emojis);
}

#[tauri::command]
pub fn get_theme(state: State<'_, SharedService>) -> String {
    state.load_theme()
}

#[tauri::command]
pub fn set_theme(state: State<'_, SharedService>, theme: String) {
    state.save_theme(&theme);
}

#[tauri::command]
pub fn get_chat_themes(state: State<'_, SharedService>) -> bool {
    state.load_chat_themes()
}

#[tauri::command]
pub fn set_chat_themes(state: State<'_, SharedService>, enabled: bool) {
    state.save_chat_themes(enabled);
}

// -------------------------------------------------------------------- actions

#[tauri::command]
pub fn send_text(
    state: State<'_, SharedService>,
    thread_id: String,
    text: String,
    user_ids: Vec<String>,
    reply_to: Option<ReplyRef>,
) {
    let reply = reply_to.map(|r| (r.message_id, r.client_context));
    state.send_text(thread_id, text, user_ids, reply);
}

#[tauri::command]
pub fn send_photo(state: State<'_, SharedService>, thread_id: String, path: String) {
    state.send_photo(thread_id, std::path::PathBuf::from(path));
}

/// Send an image pasted into the composer (clipboard bytes, no filesystem path).
#[tauri::command]
pub fn send_photo_bytes(
    state: State<'_, SharedService>,
    thread_id: String,
    data: Vec<u8>,
    ext: String,
) {
    state.send_photo_bytes(thread_id, data, ext);
}

#[tauri::command]
pub fn send_video(state: State<'_, SharedService>, thread_id: String, path: String) {
    state.send_video(thread_id, std::path::PathBuf::from(path));
}

#[tauri::command]
pub fn send_voice(state: State<'_, SharedService>, thread_id: String, data: Vec<u8>, ext: String) {
    state.send_voice(thread_id, data, ext);
}

#[tauri::command]
pub fn send_reaction(
    state: State<'_, SharedService>,
    thread_id: String,
    message_id: String,
    emoji: String,
    delete: bool,
) {
    state.send_reaction(thread_id, message_id, emoji, delete);
}

#[tauri::command]
pub fn mark_seen(
    state: State<'_, SharedService>,
    thread_id: String,
    item_id: String,
    raw: Option<Value>,
) {
    state.mark_seen(thread_id, item_id, raw);
}

#[tauri::command]
pub fn send_typing(state: State<'_, SharedService>, thread_id: String, active: bool) {
    state.send_typing(thread_id, active);
}

#[tauri::command]
pub fn download_media(state: State<'_, SharedService>, url: String) {
    state.download_media(url);
}

/// Fallback for <img> loads the IG CDN rejects: fetch through the signed client as a data URL.
#[tauri::command]
pub async fn fetch_image(
    state: State<'_, SharedService>,
    url: String,
) -> Result<Option<String>, String> {
    let svc = state.inner();
    match svc.client.public_get(&url).await {
        Ok((_status, headers, bytes)) => {
            let mime = headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(';').next().unwrap_or("").to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "image/jpeg".to_string());
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            Ok(Some(format!("data:{mime};base64,{b64}")))
        }
        Err(e) => Err(crate::service::err_text(&e)),
    }
}

/// Clipboard write that survives large payloads (thread raw responses can be
/// hundreds of KB). On Linux, arboard's Wayland backend forks a server process
/// that fails to serve such offers on KDE — the write reports success but
/// pasting yields nothing. GTK's own clipboard (served by the app, incremental
/// transfers) handles any size; elsewhere the plugin is fine.
#[tauri::command]
pub fn copy_large_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || {
            let ok = gtk::gdk::Display::default()
                .and_then(|display| gtk::Clipboard::default(&display))
                .map(|clipboard| {
                    clipboard.set_text(&text);
                })
                .is_some();
            let _ = tx.send(ok);
        })
        .map_err(|e| e.to_string())?;
        if rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap_or(false)
        {
            Ok(())
        } else {
            Err("clipboard unavailable".into())
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard().write_text(text).map_err(|e| e.to_string())
    }
}
