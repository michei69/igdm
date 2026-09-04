//! Headless Instagram Direct service: owns the `instagrapi` Client, the
//! MQTToT realtime reader task and every network operation. Emits `AppEvent`s
//! to the React frontend over the Tauri event channel; never touches UI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use instagrapi::error::{ErrorKind, IgError};
use instagrapi::extract::{extract_direct_message, extract_direct_thread};
use instagrapi::realtime::RealtimeClient;
use instagrapi::types::{DirectMessage, DirectThread, UserShort};
use instagrapi::{Client, Result};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex};

use crate::state::{AppEvent, LiveMessage, MeInfo, ThreadMeta, DEFAULT_REACTION_EMOJIS};

pub type SharedService = Arc<Service>;

/// Event channel name emitted to the frontend.
pub const EVENT_CHANNEL: &str = "igdm://event";

/// The service is a process singleton; identity is pointer equality.
impl PartialEq for Service {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Human-readable error text (`err_text`).
pub fn err_text(e: &IgError) -> String {
    let msg = e.message.trim();
    if msg.is_empty() {
        e.kind_name().to_string()
    } else {
        msg.lines().next().unwrap_or("").to_string()
    }
}

/// Item types the frontend renders; anything else is logged raw so unknown
/// payloads surface in the console instead of silently vanishing.
const KNOWN_ITEM_TYPES: &[&str] = &[
    "text",
    "link",
    "media",
    "visual_media",
    "raven_media",
    "animated_media",
    "voice_media",
    "action_log",
    "direct_reaction",
    "xma_share",
    "xma_clip",
    "xma_media_share",
    "xma_story_share",
    "xma_reel_mention",
    "generic_xma",
    "placeholder",
];

fn is_known_item_type(item_type: &str) -> bool {
    KNOWN_ITEM_TYPES.contains(&item_type)
}

/// Dump the raw JSON of the message whose read receipt failed, so the payload
/// that triggered the error can be inspected in the console.
fn log_mark_seen_raw(label: &str, raw: Option<&Value>) {
    let Some(raw) = raw else {
        log::debug!("[igdm]   ({label}: no raw message payload available)");
        return;
    };
    let text = serde_json::to_string(raw).unwrap_or_else(|_| "<unserializable>".to_string());
    log::debug!("[igdm]   {label} raw message: {text}");
}

/// Log any message whose item type the app can't render, with its raw JSON.
fn log_unhandled_messages(source: &str, messages: &[DirectMessage]) {
    for msg in messages {
        let Some(item_type) = msg.item_type.as_deref() else { continue };
        if is_known_item_type(item_type) {
            continue;
        }
        let raw = msg
            .raw
            .as_ref()
            .map(|v| {
                serde_json::to_string(v).unwrap_or_else(|_| "<unserializable>".to_string())
            })
            .unwrap_or_else(|| "<no raw>".to_string());
        log::debug!("[igdm] unhandled {source} message type {item_type:?}: {raw}");
    }
}

/// Write media bytes (pasted image, recorded voice) to a uniquely named
/// temp file.
fn write_temp_media(what: &str, data: &[u8], ext: &str) -> std::result::Result<PathBuf, String> {
    if data.is_empty() {
        return Err(format!("{what} is empty"));
    }
    if data.len() > 20 * 1024 * 1024 {
        return Err(format!("{what} is too large (max 20 MB)"));
    }
    let ext: String = ext.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let ext = if ext.is_empty() { "bin" } else { &ext };
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "igdm-media-{}-{nonce}.{ext}",
        std::process::id()
    ));
    std::fs::write(&path, data).map_err(|e| format!("Could not write {what}: {e}"))?;
    Ok(path)
}

/// Transcode any input (webm/opus, fragmented mp4) to a clean AAC-in-MP4
/// m4a via ffmpeg — IG rejects non-m4a voice uploads at upload_finish.
/// m4a input passes through unchanged; mp4 without ffmpeg is attempted
/// as-is; anything else without ffmpeg is an error.
async fn transcode_to_m4a(input: &Path) -> std::result::Result<PathBuf, String> {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "m4a" {
        return Ok(input.to_path_buf());
    }
    let has_ffmpeg = tokio::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false);
    if !has_ffmpeg {
        if ext == "mp4" {
            log::warn!("ffmpeg not found; sending recorded mp4 voice as-is (server may reject it)");
            return Ok(input.to_path_buf());
        }
        return Err("Voice messages need ffmpeg installed to convert the recording to m4a".to_string());
    }
    let output = input.with_extension("m4a");
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(input)
        .args(["-c:a", "aac", "-b:a", "96k", "-f", "mp4"])
        .arg(&output)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| format!("Could not run ffmpeg: {e}"))?;
    if !status.success() || !output.exists() {
        let _ = std::fs::remove_file(&output);
        return Err("ffmpeg failed to convert the voice recording to m4a".to_string());
    }
    Ok(output)
}

#[derive(Clone)]
pub struct Service {
    pub client: Client,
    pub app: AppHandle,
    pub sessions_dir: PathBuf,
    pending_code: Arc<Mutex<Option<oneshot::Sender<String>>>>,
    realtime: Arc<Mutex<Option<Arc<RealtimeClient>>>>,
    running: Arc<AtomicBool>,
    me_id: Arc<RwLock<String>>,
    thread_raw: Arc<Mutex<HashMap<String, Arc<Value>>>>,
}

impl Service {
    pub fn new(sessions_dir: PathBuf, app: AppHandle) -> Arc<Self> {
        let pending = Arc::new(Mutex::new(None));
        let svc = Arc::new(Self {
            client: Client::new(),
            app,
            sessions_dir,
            pending_code: pending.clone(),
            realtime: Arc::new(Mutex::new(None)),
            running: Arc::new(AtomicBool::new(false)),
            me_id: Arc::new(RwLock::new(String::new())),
            thread_raw: Arc::new(Mutex::new(HashMap::new())),
        });

        // Challenge-code handler: ask the GUI for the code, await the answer.
        let app = svc.app.clone();
        let pending = pending.clone();
        svc.client
            .set_code_handler(Arc::new(move |_username, choice| {
                let app = app.clone();
                let pending = pending.clone();
                Box::pin(async move {
                    let _ = app.emit(
                        EVENT_CHANNEL,
                        AppEvent::CodePrompt(format!(
                            "Instagram sent a verification code ({choice}). Enter it to continue."
                        )),
                    );
                    let (tx, rx) = oneshot::channel();
                    *pending.lock().await = Some(tx);
                    tokio::time::timeout(Duration::from_secs(600), rx)
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                })
            }));
        // Password-change challenge is not supported by this client (the
        // Python app's handler returns "" too).
        svc.client
            .set_change_password_handler(Arc::new(|_username| Box::pin(async { None })));
        svc
    }

    pub fn provide_code(&self, code: &str) {
        let pending = self.pending_code.clone();
        let code = code.trim().to_string();
        tauri::async_runtime::spawn(async move {
            if let Some(tx) = pending.lock().await.take() {
                let _ = tx.send(code);
            }
        });
    }

    pub fn cancel_code(&self) {
        let pending = self.pending_code.clone();
        tauri::async_runtime::spawn(async move {
            *pending.lock().await = None;
        });
    }

    fn emit(&self, event: AppEvent) {
        let _ = self.app.emit(EVENT_CHANNEL, event);
    }

    pub fn emit_login_error(&self, text: String) {
        self.emit(AppEvent::LoginError(text));
    }

    pub fn emit_thread_details(&self, thread_id: String, thread: DirectThread, meta: ThreadMeta) {
        self.emit(AppEvent::ThreadDetails(thread_id, thread, meta));
    }

    fn spawn(&self, fut: impl std::future::Future<Output = ()> + Send + 'static) {
        tauri::async_runtime::spawn(fut);
    }

    // ----------------------------------------------------------------- login

    pub fn session_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.sessions_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        files.reverse();
        files
    }

    fn settings_path(&self) -> PathBuf {
        self.sessions_dir
            .parent()
            .unwrap_or(&self.sessions_dir)
            .join("settings.json")
    }

    /// Read the whole settings file as a JSON object (empty on any failure).
    fn read_settings(&self) -> Map<String, Value> {
        std::fs::read_to_string(self.settings_path())
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    /// Persist one settings key, preserving all other keys.
    fn write_settings_key(&self, key: &str, value: Value) {
        let mut map = self.read_settings();
        map.insert(key.to_string(), value);
        let _ = std::fs::write(
            self.settings_path(),
            serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default(),
        );
    }

    /// Reaction emojis persisted in `~/.igdm/settings.json`.
    pub fn load_reaction_emojis(&self) -> Vec<String> {
        reaction_emojis_from(&self.read_settings())
    }

    pub fn save_reaction_emojis(&self, emojis: &[String]) {
        self.write_settings_key("reaction_emojis", serde_json::json!(emojis));
    }

    /// Saved UI theme: "system", "light" or "dark" (default "system").
    pub fn load_theme(&self) -> String {
        theme_from(&self.read_settings())
    }

    /// Persist the UI theme and tell every window to apply it.
    pub fn save_theme(&self, theme: &str) {
        if !matches!(theme, "system" | "light" | "dark") {
            return;
        }
        self.write_settings_key("theme", serde_json::json!(theme));
        let _ = self.app.emit("igdm://theme", theme);
    }

    /// Whether per-thread IG chat themes are enabled (default: on).
    pub fn load_chat_themes(&self) -> bool {
        chat_themes_from(&self.read_settings())
    }

    /// All three persisted settings from a single file read (`get_bootstrap`).
    pub fn settings_bundle(&self) -> (Vec<String>, String, bool) {
        let map = self.read_settings();
        (
            reaction_emojis_from(&map),
            theme_from(&map),
            chat_themes_from(&map),
        )
    }

    /// Persist the chat-themes toggle and broadcast it to every window.
    pub fn save_chat_themes(&self, enabled: bool) {
        self.write_settings_key("chat_themes", serde_json::json!(enabled));
        let _ = self.app.emit("igdm://chat-themes", enabled);
    }

    pub fn session_path(&self, username: &str) -> PathBuf {
        let safe: String = username
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_')
            .collect();
        self.sessions_dir.join(format!("{safe}.json"))
    }

    pub fn login_password(&self, username: String, password: String) {
        let svc = self.clone();
        self.spawn(async move {
            let client = &svc.client;
            let path = svc.session_path(&username);
            if path.exists() {
                let _ = client.load_settings(&path).await;
            }
            match client.login(&username, &password, None).await {
                Ok(true) => svc.adopt(true).await,
                Err(e) if e.is(ErrorKind::TwoFactorRequired) => {
                    svc.emit(AppEvent::CodePrompt(
                        "Two-factor authentication is enabled. Enter the 6-digit code.".to_string(),
                    ));
                    let (tx, rx) = oneshot::channel();
                    *svc.pending_code.lock().await = Some(tx);
                    let code = tokio::time::timeout(Duration::from_secs(600), rx)
                        .await
                        .ok()
                        .and_then(|r| r.ok());
                    match code {
                        Some(code) => match client.login(&username, &password, Some(&code)).await {
                            Ok(true) => svc.adopt(true).await,
                            Ok(false) => svc.emit(AppEvent::LoginError("Login failed".to_string())),
                            Err(e) => svc.emit(AppEvent::LoginError(err_text(&e))),
                        },
                        None => svc.emit(AppEvent::LoginError("Login cancelled".to_string())),
                    }
                }
                Ok(false) => svc.emit(AppEvent::LoginError("Login failed".to_string())),
                Err(e) => svc.emit(AppEvent::LoginError(err_text(&e))),
            }
        });
    }

    pub fn login_sessionid(&self, sessionid: String) {
        let svc = self.clone();
        self.spawn(async move {
            match svc.client.login_by_sessionid(sessionid.trim()).await {
                Ok(true) => svc.adopt(true).await,
                Ok(false) => svc.emit(AppEvent::LoginError("Login failed".to_string())),
                Err(e) => svc.emit(AppEvent::LoginError(err_text(&e))),
            }
        });
    }

    pub fn login_saved(&self, path: PathBuf) {
        let svc = self.clone();
        self.spawn(async move {
            let client = &svc.client;
            if let Err(e) = client.load_settings(&path).await {
                svc.emit(AppEvent::LoginError(format!("Couldn't load session: {e}")));
                return;
            }
            let username = match client.account_info().await {
                Ok(account) => account.username.unwrap_or_default(),
                Err(e) => {
                    svc.emit(AppEvent::LoginError(format!("Session expired: {e}")));
                    return;
                }
            };
            {
                let mut state = client.state().await;
                state.username = username.clone();
            }
            let _ = client.dump_settings(&svc.session_path(&username)).await;
            svc.adopt(true).await;
        });
    }

    async fn adopt(&self, persist: bool) {
        let client = &self.client;
        let mut username = client.username().await;
        let user_id = client
            .user_id()
            .await
            .map(|id| id.to_string())
            .unwrap_or_default();
        let mut profile_pic_url = String::new();
        if let Ok(account) = client.account_info().await {
            if username.is_empty() {
                username = account.username.unwrap_or_default();
            }
            profile_pic_url = account.profile_pic_url.unwrap_or_default();
        }
        if persist && !username.is_empty() {
            let _ = client.dump_settings(&self.session_path(&username)).await;
        }
        *self.me_id.write().unwrap_or_else(|e| e.into_inner()) = user_id.clone();
        self.emit(AppEvent::LoggedIn(MeInfo {
            username,
            user_id,
            profile_pic_url,
        }));
        self.start_realtime();
    }

    pub fn logout(&self) {
        let svc = self.clone();
        svc.stop_realtime();
        self.spawn(async move {
            let client = &svc.client;
            let username = client.username().await;
            let _ = client.logout().await;
            if !username.is_empty() {
                let path = svc.session_path(&username);
                let _ = std::fs::remove_file(path);
            }
            svc.emit(AppEvent::LoggedOut);
        });
    }

    // -------------------------------------------------------------- realtime

    fn start_realtime(&self) {
        self.running.store(true, Ordering::SeqCst);
        let svc = self.clone();
        self.spawn(async move {
            svc.realtime_loop().await;
        });
    }

    fn stop_realtime(&self) {
        self.running.store(false, Ordering::SeqCst);
        let realtime = self.realtime.clone();
        self.spawn(async move {
            let rt = realtime.lock().await.take();
            if let Some(rt) = rt {
                let _ = rt.shutdown_transport().await;
            }
        });
    }

    async fn realtime_connect_once(&self) -> instagrapi::Result<Arc<RealtimeClient>> {
        let rt = Arc::new(RealtimeClient::new(self.client.clone()));
        rt.connect().await?;
        let svc = self.clone();
        rt.on("message", move |payload| svc.on_message(payload));
        let svc = self.clone();
        rt.on("direct", move |payload| svc.on_direct(payload));
        let svc = self.clone();
        rt.on("reaction", move |payload| svc.on_reaction(payload));
        let svc = self.clone();
        rt.on("send_response", move |payload| svc.on_send_response(payload));
        let svc = self.clone();
        rt.on("typing", move |payload| svc.on_typing(payload));
        let svc = self.clone();
        rt.on("seen", move |payload| svc.on_seen(payload));
        rt.direct_subscribe().await?;
        *self.realtime.lock().await = Some(rt.clone());
        Ok(rt)
    }

    async fn realtime_disconnect(&self) {
        let rt = {
            let mut realtime = self.realtime.lock().await;
            realtime.take()
        };
        if let Some(rt) = rt {
            let _ = rt.disconnect().await;
        }
    }

    async fn realtime_loop(&self) {
        let mut attempts = 0usize;
        while self.running.load(Ordering::SeqCst) && attempts < 6 {
            let result = self.realtime_connect_once().await;
            match result {
                Ok(rt) => {
                    attempts = 0;
                    self.emit(AppEvent::Status(true, "live".to_string()));
                    let mut last_rx = Instant::now();
                    while self.running.load(Ordering::SeqCst) {
                        match rt.read_once().await {
                            Ok(_) => last_rx = Instant::now(),
                            Err(e) if e.is(ErrorKind::ClientRequestTimeout) => {
                                // idle: keep the connection alive with a PINGREQ
                                if last_rx.elapsed() > Duration::from_secs(20) {
                                    let _ = rt.ping().await;
                                    last_rx = Instant::now();
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    self.realtime_disconnect().await;
                    if !self.running.load(Ordering::SeqCst) {
                        break;
                    }
                }
                Err(e) => {
                    attempts += 1;
                    let wait = (1u64 << attempts.min(5)).min(30);
                    self.emit(AppEvent::Status(
                        false,
                        format!("{} — retry in {wait}s", err_text(&e)),
                    ));
                    self.realtime_disconnect().await;
                    let deadline = Instant::now() + Duration::from_secs(wait);
                    while self.running.load(Ordering::SeqCst) && Instant::now() < deadline {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        }
        if self.running.load(Ordering::SeqCst) {
            self.emit(AppEvent::Status(
                false,
                "disconnected (retries exhausted)".to_string(),
            ));
        }
    }

    // -------------------------------------------------- realtime payloads

    /// Everything arriving on the realtime-sub `direct` channel: typing,
    /// seen, presence and anything not classified by the realtime layer.
    /// Logged verbatim — the exact shapes drive the reducer handling.
    fn on_direct(&self, payload: Value) {
        log::debug!(
            "[igdm] recv direct event: {}",
            instagrapi::utils::json_preview(&payload, 800)
        );
    }

    /// Reaction patches (`/items/{item_id}/reactions/likes/{user_id}`) carry
    /// no message body: the reacting user is the path's last segment and the
    /// target is the item id (or the canonical `mid.$...` in the patch
    /// value). Translate them into the same `LiveMessage` shape the reducer
    /// already applies for `direct_reaction` items, so likes/unlikes attach
    /// to (or detach from) the target message instead of being dropped.
    fn on_reaction(&self, event: Value) {
        let Some(obj) = event.as_object() else { return };
        let path = obj.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let op = obj
            .get("op")
            .and_then(|v| v.as_str())
            .unwrap_or("add")
            .to_string();
        let thread_id = obj
            .get("thread_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if thread_id.is_empty() || !path.contains("/reactions/") {
            return;
        }
        let segments: Vec<&str> = path.split('/').collect();
        let item_id = segments
            .windows(2)
            .find(|w| w[0] == "items")
            .and_then(|w| w.get(1).copied())
            .unwrap_or("")
            .to_string();
        let user_id = segments.last().copied().unwrap_or("").to_string();
        if item_id.is_empty() || user_id.is_empty() {
            log::debug!("[igdm] unhandled reaction path: {path}");
            return;
        }
        // The realtime layer already parses `value` before emitting. The patch
        // value's `message_id` is the reaction's own id (it differs between
        // reactions on the same message), not the target's. The path's item id
        // is the numeric key rows are matched on, so it always wins — add and
        // remove alike.
        let Some(mut vobj) = obj.get("value").and_then(|v| v.as_object()).cloned() else {
            return;
        };
        vobj.insert("user_id".to_string(), serde_json::json!(user_id));
        vobj.insert("message_id".to_string(), serde_json::json!(item_id));
        let dm = extract_direct_message(&Value::Object(vobj));
        let timestamp = if op == "remove" {
            now_secs()
        } else {
            dm.timestamp.timestamp() as f64
        };
        self.emit(AppEvent::LiveMessage(Self::live_message(
            thread_id, item_id, op, dm, None, timestamp,
        )));
    }

    fn on_message(&self, payload: Value) {
        let Some(message) = payload.get("message").cloned() else {
            return;
        };
        let Some(obj) = message.as_object() else {
            return;
        };
        let path = obj.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let op = obj
            .get("op")
            .and_then(|v| v.as_str())
            .unwrap_or("add")
            .to_string();
        let thread_id = obj.get("thread_id").and_then(|v| v.as_str()).unwrap_or("");
        if thread_id.is_empty() {
            return;
        }
        if !path.contains("/items/") {
            return; // thread metadata patch, not a message
        }
        if path.contains("/reactions/") {
            return; // reaction patches ride the realtime `reaction` channel
        }
        let item_id = path.rsplit('/').next().unwrap_or("").to_string();
        let mut value = Map::new();
        for (k, v) in obj {
            if k != "path" && k != "op" && k != "thread_id" {
                value.insert(k.clone(), v.clone());
            }
        }
        let value = Value::Object(value);
        let dm = extract_direct_message(&value);

        if op == "remove" {
            // Keep the payload: a remove can be an un-reaction (bare
            // `{emoji, message_id}` echo) rather than a deleted message row;
            // the reducer needs it to strip the reaction correctly.
            self.emit(AppEvent::LiveMessage(Self::live_message(
                thread_id.to_string(),
                item_id,
                op,
                dm,
                None,
                now_secs(),
            )));
            return;
        }

        let item_id = if dm.id.is_empty() { item_id } else { dm.id.clone() };
        // `LiveMessage.text` and `message.text` are independent owned fields
        // both consumed by the frontend — the text value is duplicated once.
        let text = dm.text.clone();
        let timestamp = dm.timestamp.timestamp() as f64;
        let live = Self::live_message(thread_id.to_string(), item_id, op, dm, text, timestamp);
        // Verbose diagnostics: log every message event so reactions and
        // other item types are traceable end to end.
        let preview = serde_json::json!({
            "item_id": live.item_id,
            "item_type": live.item_type,
            "text": live.text,
        });
        log::debug!(
            "[igdm] recv message thread={thread_id} item={} op={} type={:?} value={}",
            live.item_id,
            live.op,
            live.item_type,
            instagrapi::utils::json_preview(&preview, 600)
        );
        if !live.item_type.is_empty() && !is_known_item_type(&live.item_type) {
            log::warn!(
                "[igdm] unhandled realtime message type {:?}: {}",
                live.item_type,
                serde_json::to_string(&value).unwrap_or_else(|_| "<unserializable>".to_string())
            );
        }
        if !matches!(live.op.as_str(), "add" | "replace" | "remove") {
            log::warn!(
                "[igdm] unhandled realtime op {:?} on thread {}: {}",
                live.op,
                thread_id,
                serde_json::to_string(&value).unwrap_or_else(|_| "<unserializable>".to_string())
            );
        }
        self.emit(AppEvent::LiveMessage(live));
    }

    /// Acks for commands the app publishes over MQTT (mark_seen, activity).
    /// Failures surface here with the server's echoed payload — surface them
    /// so send bugs are diagnosable from the console.
    fn on_send_response(&self, payload: Value) {
        let status = payload.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if status != "ok" {
            log::error!(
                "[igdm] send response failed: {}",
                instagrapi::utils::json_preview(&payload, 400)
            );
        }
    }

    fn on_typing(&self, event: Value) {
        let Some(obj) = event.as_object() else { return };
        let thread_id = obj.get("thread_id").and_then(|v| v.as_str()).unwrap_or("");
        let value = obj.get("value");
        let sender = value
            .and_then(|v| v.get("sender_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let active = value
            .and_then(|v| v.get("activity_status"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            == "1";
        let me_id = self
            .me_id
            .read()
            .map(|guard| (*guard).clone())
            .unwrap_or_else(|e| (*e.into_inner()).clone());
        if !thread_id.is_empty() && !sender.is_empty() && sender != me_id {
            self.emit(AppEvent::Typing(
                thread_id.to_string(),
                sender.to_string(),
                active,
            ));
        }
    }

    fn on_seen(&self, event: Value) {
        let Some(obj) = event.as_object() else { return };
        let thread_id = obj.get("thread_id").and_then(|v| v.as_str()).unwrap_or("");
        let path = obj.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let value = obj.get("value");
        let mut user_id = value
            .and_then(|v| v.get("user_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // `has_seen` patches (`/participants/{uid}/has_seen`) carry the
        // reader in the path, not the value.
        if user_id.is_empty() {
            let segments: Vec<&str> = path.split('/').collect();
            if let Some(pos) = segments.iter().position(|s| *s == "participants") {
                user_id = segments.get(pos + 1).copied().unwrap_or("").to_string();
            }
        }
        let item_id = value
            .and_then(|v| v.get("item_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !thread_id.is_empty() && !user_id.is_empty() {
            self.emit(AppEvent::Seen(
                thread_id.to_string(),
                user_id,
                item_id.to_string(),
            ));
        }
    }

    // ------------------------------------------------------------ direct data

    /// Shared `LiveMessage` construction for the realtime `message` and
    /// `reaction` paths: takes the fields off the already-parsed
    /// `DirectMessage` instead of re-digesting raw JSON.
    fn live_message(
        thread_id: String,
        item_id: String,
        op: String,
        dm: DirectMessage,
        text: Option<String>,
        timestamp: f64,
    ) -> LiveMessage {
        LiveMessage {
            thread_id,
            item_id,
            op,
            user_id: dm.user_id.clone().unwrap_or_default(),
            text,
            timestamp,
            item_type: dm.item_type.clone().unwrap_or_default(),
            message: Some(dm),
        }
    }

    /// `_thread_meta_from_raw` — nicknames + group avatar from raw payloads.
    pub fn thread_meta_from_raw(raw: &Value) -> ThreadMeta {
        let mut nicknames = HashMap::new();
        if let Some(entries) = raw.get("nicknames").and_then(|v| v.as_array()) {
            for entry in entries {
                let nick = entry.get("nickname").and_then(|v| v.as_str()).unwrap_or("");
                let igid = entry.get("igid").and_then(|v| v.as_str());
                if !nick.is_empty() {
                    if let Some(igid) = igid {
                        nicknames.insert(igid.to_string(), nick.to_string());
                    } else if let Some(igid) = entry.get("igid").and_then(|v| v.as_i64()) {
                        nicknames.insert(igid.to_string(), nick.to_string());
                    }
                }
            }
        }
        if nicknames.is_empty() {
            if let Some(users) = raw.get("users").and_then(|v| v.as_array()) {
                for user in users {
                    let nick = user
                        .get("nickname")
                        .or_else(|| user.get("custom_nickname"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let pk = user.get("pk").and_then(|v| v.as_str());
                    if !nick.is_empty() {
                        if let Some(pk) = pk {
                            nicknames.insert(pk.to_string(), nick.to_string());
                        }
                    }
                }
            }
        }
        let avatar = {
            let sized: Vec<&Value> = raw
                .get("thread_image")
                .and_then(|v| v.get("image_versions2"))
                .and_then(|v| v.get("candidates"))
                .and_then(|v| v.as_array())
                .map(|candidates| {
                    candidates
                        .iter()
                        .filter(|c| c.get("url").is_some())
                        .collect()
                })
                .unwrap_or_default();
            if !sized.is_empty() {
                sized
                    .iter()
                    .min_by_key(|c| c.get("width").and_then(|w| w.as_i64()).unwrap_or(i64::MAX))
                    .and_then(|c| c.get("url").and_then(|u| u.as_str()))
                    .unwrap_or("")
                    .to_string()
            } else {
                raw.get("thread_avatar")
                    .or_else(|| raw.get("avatar_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            }
        };
        ThreadMeta { nicknames, avatar }
    }

    /// `_threads_page` — paginated raw inbox fetch keeping raw JSON.
    pub async fn threads_page(
        &self,
        amount: i64,
    ) -> Result<(Vec<DirectThread>, HashMap<String, ThreadMeta>)> {
        let mut threads = Vec::new();
        let mut meta = HashMap::new();
        let mut cursor: Option<String> = None;
        loop {
            let inbox = self
                .client
                .direct_inbox_raw(cursor.as_deref(), Some(10))
                .await?;
            if let Some(raw_threads) = inbox.get("threads").and_then(|t| t.as_array()) {
                for raw in raw_threads {
                    let key = raw
                        .get("thread_id")
                        .or_else(|| raw.get("thread_v2_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    meta.insert(key.clone(), Self::thread_meta_from_raw(raw));
                    self.cache_raw(&key, raw.clone(), raw.clone()).await;
                    threads.push(extract_direct_thread(raw));
                }
            }
            cursor = inbox
                .get("oldest_cursor")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            if cursor.is_none() || (amount > 0 && threads.len() as i64 >= amount) {
                break;
            }
        }
        if amount > 0 {
            threads.truncate(amount as usize);
        }
        Ok((threads, meta))
    }

    /// Insert the `{ thread, raw_response }` envelope into the raw-thread
    /// cache (the "copy raw data" menu reads it) and return the envelope.
    async fn cache_raw(&self, key: &str, thread: Value, raw_response: Value) -> Value {
        let value = serde_json::json!({ "thread": thread, "raw_response": raw_response });
        self.thread_raw
            .lock()
            .await
            .insert(key.to_string(), Arc::new(value.clone()));
        value
    }

    /// Fetch one thread from the API, cache its raw JSON, and return the
    /// `{ thread, raw_response }` envelope the "copy raw data" menu copies.
    async fn fetch_thread_raw(&self, thread_id: &str) -> Result<Value> {
        let mut params = Map::new();
        params.insert(
            "visual_message_return_type".to_string(),
            Value::String("unseen".into()),
        );
        params.insert(
            "thread_message_limit".to_string(),
            Value::String("20".into()),
        );
        let result = self
            .client
            .private_request(
                &format!("direct_v2/threads/{thread_id}/"),
                None,
                instagrapi::client::Req::signed().params(Some(&params)),
            )
            .await?;
        let raw = result.get("thread").cloned().unwrap_or(Value::Null);
        let value = self.cache_raw(thread_id, raw, result).await;
        Ok(value)
    }

    /// `thread_details` — one thread with nickname/avatar metadata.
    pub async fn thread_details(
        &self,
        thread_id: &str,
    ) -> Result<(DirectThread, ThreadMeta, Value)> {
        let value = self.fetch_thread_raw(thread_id).await?;
        let raw = value.get("thread").cloned().unwrap_or(Value::Null);
        let meta = Self::thread_meta_from_raw(&raw);
        let model = extract_direct_thread(&raw);
        Ok((model, meta, raw))
    }

    /// Full media info for a reel share (`media/{pk}/info/`): the xma_clip
    /// payload only carries a static preview; the video and caption come
    /// from here. `None` on any failure — the modal falls back to the
    /// preview image.
    pub async fn reel_info(&self, media_id: &str) -> Option<Value> {
        self.client.media_info(media_id).await.ok()
    }

    /// One page of comments for a media item (`media/{pk}/comments/`);
    /// `max_id` paginates.
    pub async fn media_comments(&self, media_id: &str, max_id: Option<String>) -> Option<Value> {
        self.client.media_comments(media_id, max_id.as_deref()).await.ok()
    }

    /// One story item (playable video included) by pk, resolved from the
    /// author's active reel — does NOT mark the story as seen.
    pub async fn story_info(&self, story_id: &str, owner_id: &str) -> Option<Value> {
        self.client.story_info(story_id, owner_id).await.ok()
    }

    /// One message item exactly as the server returns it (fresh fetch, not
    /// the local model). Used to complete media sends whose broadcast
    /// response comes back without the `media` object.
    async fn message_raw(&self, thread_id: &str, item_id: &str) -> Option<Value> {
        let value = self.fetch_thread_raw(thread_id).await.ok()?;
        let thread = value.get("thread")?;
        let items = thread.get("items")?.as_array()?;
        items.iter().find(|it| {
            it.get("item_id").and_then(|v| v.as_str()) == Some(item_id)
                || it.get("id").and_then(|v| v.as_str()) == Some(item_id)
        }).cloned()
    }

    /// Raw thread JSON for the "copy raw data" menu: the raw API response
    /// body (`direct_v2/threads/{id}/`), exactly what the server returned.
    /// Cached entries (inbox refresh, thread details, messages page) are
    /// returned as-is; anything else is fetched on demand so the menu works
    /// for every real thread id.
    pub async fn thread_raw(&self, thread_id: &str) -> Option<Value> {
        let cached = self.thread_raw.lock().await.get(thread_id).cloned();
        if let Some(value) = cached {
            // Unwrap the internal `{ thread, raw_response }` envelope so the
            // user gets the response body itself, not a wrapper.
            return Some(
                value
                    .get("raw_response")
                    .cloned()
                    .unwrap_or_else(|| value.as_ref().clone()),
            );
        }
        // Virtual `user:<pk>` keys are not thread ids — nothing to fetch.
        if !thread_id.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let value = self.fetch_thread_raw(thread_id).await.ok()?;
        Some(value.get("raw_response").cloned().unwrap_or(value))
    }

    /// `_messages_page` — one page of messages via the private thread endpoint.
    pub async fn messages_page(
        &self,
        thread_id: &str,
        amount: i64,
        cursor: Option<&str>,
    ) -> Result<(Vec<DirectMessage>, Option<String>, bool)> {
        let mut params = Map::new();
        params.insert(
            "visual_message_return_type".to_string(),
            Value::String("unseen".into()),
        );
        params.insert("direction".to_string(), Value::String("older".into()));
        params.insert("seq_id".to_string(), Value::String("40065".into()));
        params.insert(
            "limit".to_string(),
            Value::String(amount.min(20).to_string()),
        );
        if let Some(cursor) = cursor {
            params.insert("cursor".to_string(), Value::String(cursor.to_string()));
        }
        let result = self
            .client
            .private_request(
                &format!("direct_v2/threads/{thread_id}/"),
                None,
                instagrapi::client::Req::signed().params(Some(&params)),
            )
            .await?;
        let thread = result.get("thread").cloned().unwrap_or(Value::Null);
        // Cache the raw response body here too, so "copy raw data" works for
        // any chat that has been opened even if the inbox refresh or thread
        // details never populated the cache.
        self.cache_raw(thread_id, thread.clone(), result).await;
        let messages = thread
            .get("items")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(extract_direct_message)
            .collect();
        let oldest = thread
            .get("oldest_cursor")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());
        let has_older = thread
            .get("has_older")
            .and_then(|h| h.as_bool())
            .unwrap_or(false);
        Ok((messages, oldest, has_older))
    }

    /// Background inbox refresh (`ThreadsLoaded` event).
    pub fn refresh_inbox(&self) {
        let svc = self.clone();
        self.spawn(async move {
            if let Ok((threads, meta)) = svc.threads_page(40).await {
                svc.emit(AppEvent::ThreadsLoaded(threads, meta));
            }
        });
    }

    /// Load the first message page of a thread (`MessagesLoaded` event).
    pub fn load_messages(&self, thread_id: String, amount: i64) {
        let svc = self.clone();
        self.spawn(async move {
            match svc.messages_page(&thread_id, amount, None).await {
                Ok((messages, cursor, has_more)) => {
                    log_unhandled_messages("history", &messages);
                    svc.emit(AppEvent::MessagesLoaded(thread_id, messages, cursor, has_more));
                }
                Err(e) => {
                    svc.emit(AppEvent::SendFailed(
                        thread_id,
                        format!("Couldn't load messages: {}", err_text(&e)),
                    ));
                }
            }
        });
    }

    /// Load an earlier message page (`OlderLoaded` event).
    pub fn load_older(&self, thread_id: String, cursor: String) {
        let svc = self.clone();
        self.spawn(async move {
            match svc.messages_page(&thread_id, 20, Some(&cursor)).await {
                Ok((messages, cursor, has_more)) => {
                    log_unhandled_messages("history", &messages);
                    svc.emit(AppEvent::OlderLoaded(thread_id, messages, cursor, has_more));
                }
                Err(e) => {
                    svc.emit(AppEvent::SendFailed(
                        thread_id,
                        format!("Couldn't load older messages: {}", err_text(&e)),
                    ));
                }
            }
        });
    }

    // ---------------------------------------------------------------- actions

    /// `_patch_sent` — the broadcast response omits item_type/text/user_id.
    pub fn patch_sent(
        msg: &mut DirectMessage,
        text: Option<&str>,
        thread_id: Option<&str>,
        viewer_id: &str,
    ) {
        if msg.item_type.is_none() {
            msg.item_type = Some(if text.is_some() { "text" } else { "media" }.to_string());
        }
        if msg.text.is_none() {
            msg.text = text.map(|s| s.to_string());
        }
        if msg.user_id.is_none() || msg.user_id.as_deref() == Some("") {
            msg.user_id = Some(viewer_id.to_string());
        }
        if msg.thread_id.is_none() {
            msg.thread_id = thread_id.filter(|t| !t.is_empty()).map(str::to_string);
        }
    }

    fn emit_sent(
        &self,
        key: String,
        mut msg: DirectMessage,
        viewer: &str,
        text: Option<&str>,
        thread_id: Option<&str>,
    ) {
        let real_thread_id = msg
            .thread_id
            .clone()
            .unwrap_or_else(|| key.clone());
        Self::patch_sent(&mut msg, text, thread_id, viewer);
        self.emit(AppEvent::Sent {
            key,
            real_thread_id,
            msg,
        });
    }

    pub fn send_text(
        &self,
        thread_id: String,
        text: String,
        user_ids: Vec<String>,
        reply_to: Option<(String, Option<String>)>,
    ) {
        let svc = self.clone();
        self.spawn(async move {
            let viewer = svc
                .client
                .user_id()
                .await
                .map(|i| i.to_string())
                .unwrap_or_default();
            let reply_ref = reply_to
                .as_ref()
                .map(|(id, cc)| (id.as_str(), cc.as_deref()));
            let result = if user_ids.is_empty() {
                svc.client
                    .direct_send(&text, &[], &[thread_id.as_str()], reply_ref)
                    .await
            } else {
                let ids: Vec<i64> = user_ids.iter().filter_map(|u| u.parse().ok()).collect();
                svc.client.direct_send(&text, &ids, &[], reply_ref).await
            };
            match result {
                Ok(msg) => {
                    svc.emit_sent(thread_id.clone(), msg, &viewer, Some(&text), Some(&thread_id));
                }
                Err(e) => svc.emit(AppEvent::SendFailed(thread_id, err_text(&e))),
            }
        });
    }

    pub fn send_photo(&self, thread_id: String, path: PathBuf) {
        let svc = self.clone();
        self.spawn(async move {
            svc.send_photo_impl(thread_id, path).await;
        });
    }

    /// Send a pasted image: write the bytes to a temp file, upload, then remove it.
    pub fn send_photo_bytes(&self, thread_id: String, data: Vec<u8>, ext: String) {
        let svc = self.clone();
        self.spawn(async move {
            let path = match write_temp_media("Pasted image", &data, &ext) {
                Ok(p) => p,
                Err(e) => {
                    svc.emit(AppEvent::SendFailed(thread_id, e));
                    return;
                }
            };
            svc.send_photo_impl(thread_id, path.clone()).await;
            let _ = std::fs::remove_file(path);
        });
    }

    /// The broadcast response for media sends can omit the `media` object,
    /// leaving a row that cannot render. When the response lacks it, fetch
    /// the item as the server stores it (newest items page) and use that;
    /// fall back to the response on any failure or race.
    async fn complete_sent_media(&self, thread_id: &str, msg: DirectMessage) -> DirectMessage {
        if msg.media.is_some() || msg.id.is_empty() {
            return msg;
        }
        match self.message_raw(thread_id, &msg.id).await {
            Some(item) => extract_direct_message(&item),
            None => msg,
        }
    }

    async fn send_photo_impl(&self, thread_id: String, path: PathBuf) {
        let viewer = self
            .client
            .user_id()
            .await
            .map(|i| i.to_string())
            .unwrap_or_default();
        log::debug!("[igdm] send_photo: start (thread {thread_id}, file {})", path.display());
        match self
            .client
            .direct_send_photo(&path, &[thread_id.as_str()])
            .await
        {
            Ok(msg) => {
                log::debug!("[igdm] send_photo: ok");
                let msg = self.complete_sent_media(&thread_id, msg).await;
                self.emit_sent(thread_id.clone(), msg, &viewer, None, Some(&thread_id));
            }
            Err(e) => {
                log::error!("send_photo failed: {}", err_text(&e));
                self.emit(AppEvent::SendFailed(thread_id, err_text(&e)));
            }
        }
    }

    pub fn send_video(&self, thread_id: String, path: PathBuf) {
        let svc = self.clone();
        self.spawn(async move {
            let viewer = svc
                .client
                .user_id()
                .await
                .map(|i| i.to_string())
                .unwrap_or_default();
            match svc
                .client
                .direct_send_video(&path, &[thread_id.as_str()])
                .await
            {
                Ok(msg) => {
                    let msg = svc.complete_sent_media(&thread_id, msg).await;
                    svc.emit_sent(thread_id.clone(), msg, &viewer, None, Some(&thread_id));
                }
                Err(e) => svc.emit(AppEvent::SendFailed(thread_id, err_text(&e))),
            }
        });
    }

    /// Send a recorded voice clip: write bytes to a temp file, transcode to
    /// m4a (IG rejects non-m4a voice uploads), upload, then remove temp files.
    pub fn send_voice(&self, thread_id: String, data: Vec<u8>, ext: String) {
        let svc = self.clone();
        self.spawn(async move {
            let path = match write_temp_media("Voice recording", &data, &ext) {
                Ok(p) => p,
                Err(e) => {
                    svc.emit(AppEvent::SendFailed(thread_id, e));
                    return;
                }
            };
            let m4a = match transcode_to_m4a(&path).await {
                Ok(p) => p,
                Err(e) => {
                    svc.emit(AppEvent::SendFailed(thread_id, e));
                    let _ = std::fs::remove_file(&path);
                    return;
                }
            };
            let viewer = svc
                .client
                .user_id()
                .await
                .map(|i| i.to_string())
                .unwrap_or_default();
            let result = svc
                .client
                .direct_send_voice(&m4a, &[thread_id.as_str()])
                .await;
            let _ = std::fs::remove_file(&path);
            if m4a != path {
                let _ = std::fs::remove_file(&m4a);
            }
            match result {
                Ok(msg) => {
                    let msg = svc.complete_sent_media(&thread_id, msg).await;
                    svc.emit_sent(thread_id.clone(), msg, &viewer, None, Some(&thread_id));
                }
                Err(e) => {
                    log::error!("send_voice failed: {}", err_text(&e));
                    svc.emit(AppEvent::SendFailed(thread_id, err_text(&e)));
                }
            }
        });
    }

    pub fn send_reaction(
        &self,
        thread_id: String,
        message_id: String,
        emoji: String,
        delete: bool,
    ) {
        let svc = self.clone();
        self.spawn(async move {
            let status = if delete { "deleted" } else { "created" };
            match svc
                .client
                .direct_reaction(&thread_id, &message_id, &emoji, status)
                .await
            {
                Ok(_) => {}
                Err(e) => svc.emit(AppEvent::SendFailed(
                    thread_id,
                    format!(
                        "Couldn't {} reaction: {}",
                        if delete { "remove" } else { "react" },
                        err_text(&e)
                    ),
                )),
            }
        });
    }

    pub fn mark_seen(&self, thread_id: String, item_id: String, raw: Option<Value>) {
        let svc = self.clone();
        self.spawn(async move {
            // The HTTP read receipt is the authoritative server-side mark
            // (instagrapi's direct_send_seen is the same call on the latest
            // item). The MQTT `mark_seen` mirror was removed: IG 400s every
            // payload shape (no cc, mutation-token cc, UUID cc), and the
            // peer's read state arrives reliably as `has_seen` patches on
            // topic 146 regardless.
            if !item_id.is_empty() {
                if let Err(e) = svc.client.direct_message_seen(&thread_id, &item_id).await {
                    log::error!("[igdm] mark_seen failed (thread {thread_id}, item {item_id}): {e}");
                    log_mark_seen_raw("mark_seen", raw.as_ref());
                }
            }
        });
    }

    pub fn send_typing(&self, thread_id: String, active: bool) {
        let svc = self.clone();
        self.spawn(async move {
            let realtime = svc.realtime.lock().await.clone();
            match realtime {
                Some(rt) if rt.is_connected() => {
                    if let Err(e) = rt.direct_indicate_activity(&thread_id, active).await {
                        log::warn!("[igdm] typing failed (thread {thread_id}): {e}");
                    }
                }
                Some(_) => log::debug!(
                    "[igdm] typing dropped: realtime not connected (thread {thread_id})"
                ),
                None => log::debug!("[igdm] typing dropped: no realtime client (thread {thread_id})"),
            }
        });
    }

    pub fn search_users(&self, query: String) {
        let svc = self.clone();
        self.spawn(async move {
            let result = match svc.client.direct_search(&query).await {
                Ok(users) => Ok(users),
                Err(_) => svc.client.search_users_v1(&query, 30).await,
            };
            match result {
                Ok(users) => svc.emit(AppEvent::SearchResults(query, users)),
                Err(_) => svc.emit(AppEvent::SearchFailed(query)),
            }
        });
    }

    /// `thread_for_user` — existing 1:1 thread for a user, or None.
    pub fn thread_for_user(&self, user: UserShort) {
        let svc = self.clone();
        self.spawn(async move {
            let pk = user.pk.parse::<i64>().unwrap_or(0);
            let result = svc.client.direct_thread_by_participants(&[pk]).await;
            let thread_id = match result {
                Ok(value) => value
                    .get("thread")
                    .and_then(|t| t.get("thread_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                Err(_) => None,
            };
            svc.emit(AppEvent::ThreadByUser(user, thread_id));
        });
    }

    pub fn approve_request(&self, thread_id: String) {
        let svc = self.clone();
        self.spawn(async move {
            match svc.client.direct_request_approve(&thread_id).await {
                Ok(true) => svc.emit(AppEvent::Approved(thread_id)),
                Ok(false) => svc.emit(AppEvent::SendFailed(
                    thread_id,
                    "Couldn't accept: unknown error".into(),
                )),
                Err(e) => svc.emit(AppEvent::SendFailed(
                    thread_id,
                    format!("Couldn't accept: {}", err_text(&e)),
                )),
            }
        });
    }

    pub fn download_media(&self, url: String) {
        let svc = self.clone();
        self.spawn(async move {
            let cache = svc.sessions_dir.join("media");
            let _ = std::fs::create_dir_all(&cache);
            match svc.client.download_media(&url, &cache).await {
                Ok(path) => svc.emit(AppEvent::MediaDone(path.display().to_string())),
                Err(e) => svc.emit(AppEvent::MediaFailed(err_text(&e))),
            }
        });
    }
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Reaction emojis from a parsed settings map (defaults when absent/invalid).
fn reaction_emojis_from(map: &Map<String, Value>) -> Vec<String> {
    let list: Vec<String> = map
        .get("reaction_emojis")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if list.len() == 5 && list.iter().all(|e| !e.is_empty()) {
        list
    } else {
        DEFAULT_REACTION_EMOJIS
            .iter()
            .map(|e| e.to_string())
            .collect()
    }
}

/// UI theme from a parsed settings map ("system" default).
fn theme_from(map: &Map<String, Value>) -> String {
    match map.get("theme").and_then(|v| v.as_str()) {
        Some(t @ ("system" | "light" | "dark")) => t.to_string(),
        _ => "system".to_string(),
    }
}

/// Chat-themes toggle from a parsed settings map (default: on).
fn chat_themes_from(map: &Map<String, Value>) -> bool {
    map.get("chat_themes").and_then(|v| v.as_bool()).unwrap_or(true)
}
