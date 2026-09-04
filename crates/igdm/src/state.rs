//! Wire-format state model: background events delivered from the Rust service
//! to the React frontend over the Tauri event channel (`igdm://event`).

use std::collections::HashMap;

use instagrapi::types::{DirectMessage, DirectThread, UserShort};
use serde::Serialize;

/// Per-thread metadata parsed from raw payloads (`_thread_meta_from_raw`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ThreadMeta {
    pub nicknames: HashMap<String, String>,
    pub avatar: String,
}

/// Default reaction emojis shown in the message context menu.
pub const DEFAULT_REACTION_EMOJIS: [&str; 5] = ["❤\u{fe0f}", "😆", "😮", "😢", "😡"];

/// Current user info.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MeInfo {
    pub username: String,
    pub user_id: String,
    pub profile_pic_url: String,
}

/// Normalized realtime item (`LiveMessage`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LiveMessage {
    pub thread_id: String,
    pub item_id: String,
    pub op: String,
    pub user_id: String,
    pub text: Option<String>,
    pub timestamp: f64,
    pub item_type: String,
    pub message: Option<DirectMessage>,
}

/// Events emitted from background tasks to the frontend. Serialized with a
/// canonical shape: `{"type": "...", "data": ...}` where `data` is an array
/// for tuple/newtype variants and an object for the struct variant (`Sent`).
/// A hand-written serializer is required because serde's internally-tagged
/// enum mode serializes newtype variants transparently (no array wrapper),
/// which the TS reducer does not expect.
#[derive(Clone, Debug)]
pub enum AppEvent {
    Status(bool, String),
    LoginError(String),
    LoggedIn(MeInfo),
    LoggedOut,
    CodePrompt(String),
    LiveMessage(LiveMessage),
    Typing(String, String, bool),
    Seen(String, String, String),
    ThreadsLoaded(Vec<DirectThread>, HashMap<String, ThreadMeta>),
    ThreadDetails(String, DirectThread, ThreadMeta),
    MessagesLoaded(String, Vec<DirectMessage>, Option<String>, bool),
    OlderLoaded(String, Vec<DirectMessage>, Option<String>, bool),
    /// key (thread or `user:<pk>`), the real thread id as a lossless string,
    /// and the sent message.
    Sent {
        key: String,
        real_thread_id: String,
        msg: DirectMessage,
    },
    SendFailed(String, String),
    SearchResults(String, Vec<UserShort>),
    SearchFailed(String),
    ThreadByUser(UserShort, Option<String>),
    Approved(String),
    MediaDone(String),
    MediaFailed(String),
}

impl AppEvent {
    fn event_name(&self) -> &'static str {
        match self {
            AppEvent::Status(..) => "Status",
            AppEvent::LoginError(..) => "LoginError",
            AppEvent::LoggedIn(..) => "LoggedIn",
            AppEvent::LoggedOut => "LoggedOut",
            AppEvent::CodePrompt(..) => "CodePrompt",
            AppEvent::LiveMessage(..) => "LiveMessage",
            AppEvent::Typing(..) => "Typing",
            AppEvent::Seen(..) => "Seen",
            AppEvent::ThreadsLoaded(..) => "ThreadsLoaded",
            AppEvent::ThreadDetails(..) => "ThreadDetails",
            AppEvent::MessagesLoaded(..) => "MessagesLoaded",
            AppEvent::OlderLoaded(..) => "OlderLoaded",
            AppEvent::Sent { .. } => "Sent",
            AppEvent::SendFailed(..) => "SendFailed",
            AppEvent::SearchResults(..) => "SearchResults",
            AppEvent::SearchFailed(..) => "SearchFailed",
            AppEvent::ThreadByUser(..) => "ThreadByUser",
            AppEvent::Approved(..) => "Approved",
            AppEvent::MediaDone(..) => "MediaDone",
            AppEvent::MediaFailed(..) => "MediaFailed",
        }
    }
}

#[derive(Serialize)]
struct SentData<'a> {
    key: &'a str,
    real_thread_id: &'a str,
    msg: &'a DirectMessage,
}

impl serde::Serialize for AppEvent {
    /// `{"type": "<name>", "data": <payload>}` with payloads written straight
    /// to the serializer (no intermediate `Value`). The two `HashMap`-carrying
    /// variants keep `json!` so their keys stay sorted, byte-identical to the
    /// old impl.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut s = serializer.serialize_map(Some(2))?;
        s.serialize_entry("type", self.event_name())?;
        match self {
            AppEvent::Status(a, b) => s.serialize_entry("data", &(a, b))?,
            AppEvent::LoginError(t) => s.serialize_entry("data", &(t,))?,
            AppEvent::LoggedIn(me) => s.serialize_entry("data", &(me,))?,
            AppEvent::LoggedOut => {}
            AppEvent::CodePrompt(t) => s.serialize_entry("data", &(t,))?,
            AppEvent::LiveMessage(live) => s.serialize_entry("data", &(live,))?,
            AppEvent::Typing(a, b, c) => s.serialize_entry("data", &(a, b, c))?,
            AppEvent::Seen(a, b, c) => s.serialize_entry("data", &(a, b, c))?,
            AppEvent::ThreadsLoaded(threads, meta) => {
                s.serialize_entry("data", &serde_json::json!([threads, meta]))?
            }
            AppEvent::ThreadDetails(a, b, c) => {
                s.serialize_entry("data", &serde_json::json!([a, b, c]))?
            }
            AppEvent::MessagesLoaded(a, b, c, d) => s.serialize_entry("data", &(a, b, c, d))?,
            AppEvent::OlderLoaded(a, b, c, d) => s.serialize_entry("data", &(a, b, c, d))?,
            AppEvent::Sent {
                key,
                real_thread_id,
                msg,
            } => s.serialize_entry(
                "data",
                &SentData {
                    key: key.as_str(),
                    real_thread_id: real_thread_id.as_str(),
                    msg,
                },
            )?,
            AppEvent::SendFailed(a, b) => s.serialize_entry("data", &(a, b))?,
            AppEvent::SearchResults(a, b) => s.serialize_entry("data", &(a, b))?,
            AppEvent::SearchFailed(q) => s.serialize_entry("data", &(q,))?,
            AppEvent::ThreadByUser(user, tid) => s.serialize_entry("data", &(user, tid))?,
            AppEvent::Approved(k) => s.serialize_entry("data", &(k,))?,
            AppEvent::MediaDone(p) => s.serialize_entry("data", &(p,))?,
            AppEvent::MediaFailed(t) => s.serialize_entry("data", &(t,))?,
        }
        s.end()
    }
}
