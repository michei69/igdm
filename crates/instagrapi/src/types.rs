//! Public model types mirroring `instagrapi.types` (only the surface the
//! direct-messaging client needs), with serde so "copy raw data" works.

use std::collections::HashMap;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn is_false(v: &bool) -> bool {
    !*v
}

/// Short user profile (`UserShort`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserShort {
    pub pk: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_url_hd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_private: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_reel_media: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_anonymous_profile_picture: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fbid_v2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interop_messaging_user_fbid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strong_id__: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub account_badges: Vec<Value>,
}

/// The authenticated account (`Account`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub pk: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_url_hd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_private: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follower_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub following_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_url: Option<String>,
}

/// Full user profile (`User`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub pk: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_private: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follower_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub following_count: Option<i64>,
}

/// Media inside a direct message (`DirectMedia`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DirectMedia {
    pub id: String,
    pub media_type: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserShort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<String>,
    /// Media pixel dimensions (best image candidate), for aspect-ratio rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_url: Option<String>,
    /// Voice message length in milliseconds (`voice_media.media.audio.duration`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_duration_ms: Option<i64>,
    /// Voice message waveform amplitudes, 0..1 (`voice_media.media.audio.waveform_data`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waveform: Option<Vec<f64>>,
}

/// One emoji reaction (`MessageReaction`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageReaction {
    pub timestamp: DateTime<Local>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_context: Option<String>,
    pub sender_id: i64,
    pub emoji: String,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub super_react_type: String,
}

impl Default for MessageReaction {
    fn default() -> Self {
        Self {
            timestamp: DateTime::<Local>::default(),
            client_context: None,
            sender_id: 0,
            emoji: String::new(),
            super_react_type: "none".to_string(),
        }
    }
}

/// Reactions structure on a direct message (`MessageReactions`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageReactions {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub likes: Vec<Value>,
    #[serde(default)]
    pub likes_count: i64,
    #[serde(default)]
    pub emojis: Vec<MessageReaction>,
}

/// Deserialize a string-or-number JSON value into a `String` (thread ids
/// arrive as strings that overflow `u64`, so they must not pass through a
/// numeric type).
fn de_str_or_num<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Visitor;
    impl serde::de::Visitor<'_> for Visitor {
        type Value = Option<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or number")
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }
        fn visit_u128<E: serde::de::Error>(self, v: u128) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }
    }
    deserializer.deserialize_option(Visitor)
}

/// A direct message item (`DirectMessage`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DirectMessage {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "de_str_or_num")]
    pub thread_id: Option<String>,
    pub timestamp: DateTime<Local>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_sent_by_viewer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_shh_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactions: Option<MessageReactions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// `replied_to_message`, minimal fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<Box<DirectMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animated_media: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<DirectMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_media: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_share: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reel_share: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_share: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub felix_share: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xma_share: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generic_xma: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_xma: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xma_story_share: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xma_reel_mention: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_log: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_context: Option<String>,
    /// The verbatim server payload for this message item. Used by "copy raw
    /// data" and for rendering message types the model does not cover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}

/// Last-seen info per user inside a thread (`LastSeenInfo`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LastSeenInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shh_seen_state: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disappearing_messages_seen_state: Option<Value>,
}

/// A direct thread (`DirectThread`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DirectThread {
    pub pk: String,
    pub id: String,
    #[serde(default)]
    pub messages: Vec<DirectMessage>,
    #[serde(default)]
    pub users: Vec<UserShort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inviter: Option<UserShort>,
    #[serde(default)]
    pub left_users: Vec<UserShort>,
    #[serde(default)]
    pub admin_user_ids: Vec<Value>,
    pub last_activity_at: DateTime<Local>,
    pub muted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_pin: Option<bool>,
    pub named: bool,
    pub canonical: bool,
    pub pending: bool,
    pub archived: bool,
    pub thread_type: String,
    #[serde(default)]
    pub thread_title: String,
    pub folder: i64,
    pub vc_muted: bool,
    pub is_group: bool,
    pub mentions_muted: bool,
    pub approval_required_for_new_members: bool,
    pub input_mode: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub business_thread_folder: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_state: Option<i64>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub is_close_friend_thread: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_admin_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shh_mode_enabled: Option<bool>,
    #[serde(default)]
    pub last_seen_at: HashMap<String, LastSeenInfo>,
    /// Raw IG chat theme (`theme_data`): bubble/background/composer colors
    /// and background art, passed through verbatim for the frontend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_data: Option<Value>,
}
