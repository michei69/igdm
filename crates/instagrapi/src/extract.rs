//! Extractors ported from `instagrapi.extractors` (direct-messaging subset).

use chrono::{DateTime, Local, TimeZone};
use serde_json::{json, Map, Value};

use crate::types::{
    Account, DirectMedia, DirectMessage, DirectThread, LastSeenInfo, MessageReaction,
    MessageReactions, User, UserShort,
};
use crate::utils::{get, get_bool, get_bool_m, get_i64, get_i64_m, get_str, get_str_m};

const XMA_KEYS: &[&str] = &[
    "xma_clip",
    "xma_media_share",
    "xma_story_share",
    "xma_profile",
    "generic_xma",
];

/// `datetime.fromtimestamp(int(ts) // 1_000_000)` as local time.
fn ts_from_micros(value: &Value) -> DateTime<Local> {
    let micros = match value {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    };
    let secs = micros / 1_000_000;
    let nanos = ((micros % 1_000_000).max(0) * 1000) as u32;
    Local
        .timestamp_opt(secs, nanos)
        .single()
        .unwrap_or_default()
}

/// `InstagramIdCodec.encode` — media id -> shortcode (base-64 with IG alphabet).
fn media_codec_encode(mut num: i64) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    if num == 0 {
        return (ALPHABET[0] as char).into();
    }
    let mut out = Vec::new();
    while num > 0 {
        out.push(ALPHABET[(num % 64) as usize]);
        num /= 64;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn raw_xma_from(data: &Map<String, Value>) -> Option<Value> {
    let mut raw = Map::new();
    for key in XMA_KEYS {
        if let Some(v) = data.get(*key) {
            if !v.is_null() {
                raw.insert(key.to_string(), v.clone());
            }
        }
    }
    if raw.is_empty() {
        None
    } else {
        Some(Value::Object(raw))
    }
}

/// Best candidate by width*height: (url, width, height).
fn best_candidate(candidates: &Value) -> Option<(String, i64, i64)> {
    let arr = candidates.as_array()?;
    arr.iter()
        .filter_map(|c| {
            let url = get(c, "url")?.as_str()?;
            let w = get_i64(c, "width").unwrap_or(0);
            let h = get_i64(c, "height").unwrap_or(0);
            Some((w * h, url, w, h))
        })
        .max_by_key(|(area, _, _, _)| *area)
        .map(|(_, url, w, h)| (url.to_string(), w, h))
}

/// Best candidate url by width*height (clones only the winner).
fn best_candidate_url(candidates: &Value) -> Option<String> {
    best_candidate(candidates).map(|(url, _, _)| url)
}

/// `extract_direct_media`
pub(crate) fn extract_direct_media(data: &Value) -> DirectMedia {
    let mut m = DirectMedia {
        id: get_str(data, "id").unwrap_or_default(),
        media_type: get_i64(data, "media_type").unwrap_or(0),
        user: None,
        thumbnail_url: None,
        video_url: None,
        width: None,
        height: None,
        audio_url: None,
        audio_duration_ms: None,
        waveform: None,
    };
    if let Some(videos) = get(data, "video_versions") {
        m.video_url = best_candidate_url(videos);
    }
    if let Some(imgs) = get(data, "image_versions2") {
        if let Some((url, w, h)) = best_candidate(get(imgs, "candidates").unwrap_or(&Value::Null)) {
            m.thumbnail_url = Some(url);
            m.width = Some(w);
            m.height = Some(h);
        }
    }
    // Fallback: direct media items often carry explicit dimensions.
    if m.width.is_none() {
        m.width = get_i64(data, "original_width").or_else(|| get_i64(data, "media_width"));
        m.height = get_i64(data, "original_height").or_else(|| get_i64(data, "media_height"));
    }
    if let Some(user) = get(data, "user") {
        m.user = Some(extract_user_short(user));
    }
    if let Some(audio) = get(data, "audio") {
        m.audio_url = get_str(audio, "audio_src");
        m.audio_duration_ms = get_i64(audio, "duration");
        m.waveform = audio
            .get("waveform_data")
            .and_then(|w| w.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect());
    }
    m
}

/// `extract_media_v1` (minimal: video/thumbnail urls + user), stored raw.
pub(crate) fn extract_media_share(data: &Value) -> Value {
    let mut media = data.clone();
    if let Some(obj) = media.as_object_mut() {
        if let Some(videos) = obj.get("video_versions") {
            if let Some(url) = best_candidate_url(videos) {
                obj.insert("video_url".to_string(), json!(url));
            }
        }
        if let Some(imgs) = obj.get("image_versions2") {
            if let Some(url) = best_candidate_url(get(imgs, "candidates").unwrap_or(&Value::Null)) {
                obj.insert("thumbnail_url".to_string(), json!(url));
            }
        }
        if let Some(user) = obj.get("user") {
            obj.insert("user".to_string(), json!(extract_user_short(user)));
        }
        if !obj.contains_key("code") {
            let id = obj.get("id").and_then(|v| match v {
                Value::String(s) => s.parse::<i64>().ok(),
                Value::Number(n) => n.as_i64(),
                _ => None,
            });
            if let Some(id) = id {
                obj.insert("code".to_string(), json!(media_codec_encode(id)));
            }
        }
    }
    media
}

/// `extract_media_v1_xma` (minimal) -> Value with video_url/title, or Null.
fn extract_media_xma(data: &Value) -> Value {
    if data.is_null() {
        return Value::Null;
    }
    // Unavailable shares (deleted content / privacy-hidden) carry neither a
    // target_url nor a preview but still deserve extraction: the UI renders
    // the `caption_body_text`/`title_text` notice instead of an unsupported
    // row. Drop the payload only when nothing renderable is present.
    if get_str(data, "target_url").is_none()
        && get_str(data, "title_text").is_none()
        && get_str(data, "caption_body_text").is_none()
        && get_str(data, "preview_url").is_none()
    {
        return Value::Null;
    }
    let mut media = data.clone();
    if let Some(obj) = media.as_object_mut() {
        if let Some(url) = obj.get("target_url").cloned() {
            obj.insert("video_url".to_string(), url);
        }
        if let Some(title) = obj.get("title_text").cloned() {
            obj.insert("title".to_string(), title);
        }
        for key in [
            "preview_url",
            "preview_url_mime_type",
            "header_icon_url",
            "header_icon_width",
            "header_icon_height",
            "header_title_text",
            "preview_media_fbid",
        ] {
            if !obj.contains_key(key) {
                obj.insert(
                    key.to_string(),
                    match key {
                        "header_icon_width" | "header_icon_height" => json!(0),
                        _ => json!(""),
                    },
                );
            }
        }
    }
    media
}

/// `extract_direct_message`
pub fn extract_direct_message(data: &Value) -> DirectMessage {
    let mut msg = DirectMessage {
        id: get_str(data, "item_id").unwrap_or_default(),
        ..DirectMessage::default()
    };
    if let Some(obj) = data.as_object() {
        let raw_xma = raw_xma_from(obj);

        if let Some(replied) = obj.get("replied_to_message") {
            msg.reply = Some(Box::new(extract_direct_message(replied)));
        }
        if let Some(ms) = obj.get("media_share") {
            if !ms.is_null() {
                msg.media_share = Some(extract_media_share(ms));
            }
        }
        if let Some(media) = obj.get("media") {
            if !media.is_null() {
                msg.media = Some(extract_direct_media(media));
            }
        }
        if let Some(voice) = obj.get("voice_media") {
            if let Some(media) = get(voice, "media") {
                if !media.is_null() {
                    msg.media = Some(extract_direct_media(media));
                }
            }
        }
        // `visual_media` (e.g. `raven_media` items) wraps the media under a
        // nested `media` object; normalize it into `msg.media` like
        // `voice_media`. A top-level `media` (photo items) takes precedence.
        if msg.media.is_none() {
            if let Some(vm) = obj.get("visual_media") {
                if let Some(media) = get(vm, "media") {
                    if !media.is_null() {
                        msg.media = Some(extract_direct_media(media));
                    }
                }
            }
        }
        let clip = obj.get("clip").cloned().unwrap_or(Value::Null);
        if !clip.is_null() {
            let inner = if let Some(inner) = get(&clip, "clip") {
                inner.clone()
            } else {
                clip
            };
            msg.clip = Some(extract_media_share(&inner));
        }
        // xma_clip / xma_media_share -> xma_share
        let mut xma_share = None;
        if let Some(xma_clip) = obj.get("xma_clip") {
            if let Some(first) = xma_clip.as_array().and_then(|a| a.first()) {
                let extracted = extract_media_xma(first);
                if !extracted.is_null() {
                    xma_share = Some(extracted);
                }
            }
        }
        if xma_share.is_none() {
            if let Some(xma_ms) = obj.get("xma_media_share") {
                if let Some(first) = xma_ms.as_array().and_then(|a| a.first()) {
                    let extracted = extract_media_xma(first);
                    if !extracted.is_null() {
                        xma_share = Some(extracted);
                    }
                }
            }
        }
        msg.xma_share = xma_share;
        let generic = obj.get("generic_xma").cloned().unwrap_or(Value::Null);
        if !generic.is_null() {
            let items: Vec<Value> = generic
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(extract_media_xma)
                        .filter(|v| !v.is_null())
                        .collect()
                })
                .unwrap_or_default();
            msg.generic_xma = Some(Value::Array(items));
        }
        if let Some(ts) = obj.get("timestamp") {
            msg.timestamp = ts_from_micros(ts);
        }
        msg.user_id = obj
            .get("user_id")
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .filter(|s| !s.is_empty());
        msg.thread_id = obj.get("thread_id").and_then(|v| match v {
            Value::Number(n) => Some(n.to_string()),
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        });
        msg.item_type = get_str_m(obj, "item_type");
        msg.is_sent_by_viewer = get_bool_m(obj, "is_sent_by_viewer");
        msg.is_shh_mode = get_bool_m(obj, "is_shh_mode");
        msg.client_context = get_str_m(obj, "client_context").filter(|s| !s.is_empty());
        msg.text = get_str_m(obj, "text");
        // `link` items carry the URL inside `link.text`, not at the top
        // level; surface it as the message text so links render as normal
        // text messages (bubble, preview, replies).
        if msg.text.is_none() || msg.text.as_deref() == Some("") {
            if let Some(link) = obj.get("link") {
                msg.text = get_str(link, "text");
            }
        }
        msg.raw = if data.is_null() {
            None
        } else {
            Some(data.clone())
        };
        msg.link = obj.get("link").cloned().filter(|v| !v.is_null());
        msg.animated_media = obj.get("animated_media").cloned().filter(|v| !v.is_null());
        msg.visual_media = obj.get("visual_media").cloned().filter(|v| !v.is_null());
        msg.reel_share = obj.get("reel_share").cloned().filter(|v| !v.is_null());
        msg.story_share = obj.get("story_share").cloned().filter(|v| !v.is_null());
        msg.felix_share = obj.get("felix_share").cloned().filter(|v| !v.is_null());
        msg.placeholder = obj.get("placeholder").cloned().filter(|v| !v.is_null());
        msg.xma_story_share = obj
            .get("xma_story_share")
            .cloned()
            .filter(|v| !v.is_null());
        msg.xma_reel_mention = obj
            .get("xma_reel_mention")
            .cloned()
            .filter(|v| !v.is_null());
        msg.action_log = obj.get("action_log").cloned().filter(|v| !v.is_null());
        msg.raw_xma = raw_xma;

        if let Some(reactions) = obj.get("reactions") {
            if let Some(robj) = reactions.as_object() {
                let mut out = MessageReactions {
                    likes: robj
                        .get("likes")
                        .and_then(|l| l.as_array().cloned())
                        .unwrap_or_default(),
                    likes_count: get_i64(reactions, "likes_count").unwrap_or(0),
                    emojis: Vec::new(),
                };
                if let Some(emojis) = robj.get("emojis").and_then(|e| e.as_array()) {
                    for emoji in emojis {
                        out.emojis.push(MessageReaction {
                            timestamp: get(emoji, "timestamp")
                                .map(ts_from_micros)
                                .unwrap_or_default(),
                            client_context: get_str(emoji, "client_context"),
                            sender_id: get_i64(emoji, "sender_id").unwrap_or(0),
                            emoji: get_str(emoji, "emoji").unwrap_or_default(),
                            super_react_type: get_str(emoji, "super_react_type")
                                .unwrap_or_else(|| "none".to_string()),
                        });
                    }
                }
                msg.reactions = Some(out);
            }
        }
    }
    msg
}

/// `extract_user_short`
pub(crate) fn extract_user_short(data: &Value) -> UserShort {
    let pk = get_str(data, "id")
        .or_else(|| get_str(data, "pk"))
        .unwrap_or_default();
    UserShort {
        pk,
        username: get_str(data, "username"),
        full_name: get_str(data, "full_name"),
        profile_pic_url: get_str(data, "profile_pic_url"),
        profile_pic_url_hd: get_str(data, "profile_pic_url_hd"),
        is_private: get_bool(data, "is_private"),
        is_verified: get_bool(data, "is_verified"),
        latest_reel_media: get_i64(data, "latest_reel_media"),
        has_anonymous_profile_picture: get_bool(data, "has_anonymous_profile_picture"),
        profile_pic_id: get_str(data, "profile_pic_id"),
        fbid_v2: get_str(data, "fbid_v2"),
        interop_messaging_user_fbid: get_str(data, "interop_messaging_user_fbid"),
        strong_id__: get_str(data, "strong_id__"),
        account_badges: data
            .get("account_badges")
            .and_then(|b| b.as_array().cloned())
            .unwrap_or_default(),
    }
}

/// `extract_direct_thread`
pub fn extract_direct_thread(data: &Value) -> DirectThread {
    let id = get_str(data, "thread_id").unwrap_or_default();
    let mut thread = DirectThread {
        pk: get_str(data, "thread_v2_id").unwrap_or_default(),
        id,
        ..DirectThread::default()
    };
    if let Some(obj) = data.as_object() {
        if let Some(items) = obj.get("items").and_then(|i| i.as_array()) {
            for item in items {
                let mut msg = extract_direct_message(item);
                if msg.thread_id.is_none() && !thread.id.is_empty() {
                    msg.thread_id = Some(thread.id.clone());
                }
                thread.messages.push(msg);
            }
        }
        thread.users = obj
            .get("users")
            .and_then(|u| u.as_array())
            .map(|arr| arr.iter().map(extract_user_short).collect())
            .unwrap_or_default();
        if let Some(inviter) = obj.get("inviter") {
            if !inviter.is_null() {
                thread.inviter = Some(extract_user_short(inviter));
            }
        }
        thread.left_users = obj
            .get("left_users")
            .and_then(|u| u.as_array())
            .map(|arr| arr.iter().map(extract_user_short).collect())
            .unwrap_or_default();
        thread.admin_user_ids = obj
            .get("admin_user_ids")
            .and_then(|u| u.as_array().cloned())
            .unwrap_or_default();
        thread.last_activity_at = obj
            .get("last_activity_at")
            .map(ts_from_micros)
            .unwrap_or_default();
        thread.muted = get_bool_m(obj, "muted").unwrap_or(false);
        thread.is_pin = get_bool_m(obj, "is_pin");
        thread.named = get_bool_m(obj, "named").unwrap_or(false);
        thread.canonical = get_bool_m(obj, "canonical").unwrap_or(false);
        thread.pending = get_bool_m(obj, "pending").unwrap_or(false);
        thread.archived = get_bool_m(obj, "archived").unwrap_or(false);
        thread.thread_type = get_str_m(obj, "thread_type").unwrap_or_default();
        thread.thread_title = get_str_m(obj, "thread_title").unwrap_or_default();
        thread.folder = get_i64_m(obj, "folder").unwrap_or(0);
        thread.vc_muted = get_bool_m(obj, "vc_muted").unwrap_or(false);
        thread.is_group = get_bool_m(obj, "is_group").unwrap_or(false);
        thread.mentions_muted = get_bool_m(obj, "mentions_muted").unwrap_or(false);
        thread.approval_required_for_new_members =
            get_bool_m(obj, "approval_required_for_new_members").unwrap_or(false);
        thread.input_mode = get_i64_m(obj, "input_mode").unwrap_or(0);
        thread.business_thread_folder = get_i64_m(obj, "business_thread_folder");
        thread.read_state = get_i64_m(obj, "read_state");
        thread.is_close_friend_thread = get_bool_m(obj, "is_close_friend_thread").unwrap_or(false);
        thread.assigned_admin_id = get_i64_m(obj, "assigned_admin_id");
        thread.shh_mode_enabled = get_bool_m(obj, "shh_mode_enabled");
        if let Some(seen) = obj.get("last_seen_at").and_then(|s| s.as_object()) {
            for (uid, info) in seen {
                let mut entry = LastSeenInfo {
                    item_id: get_str(info, "item_id"),
                    shh_seen_state: get(info, "shh_seen_state")
                        .cloned()
                        .filter(|v| !v.is_null()),
                    disappearing_messages_seen_state: get(info, "disappearing_messages_seen_state")
                        .cloned()
                        .filter(|v| !v.is_null()),
                    ..LastSeenInfo::default()
                };
                entry.timestamp = get(info, "timestamp").map(ts_from_micros);
                entry.created_at = get(info, "created_at").map(ts_from_micros);
                thread.last_seen_at.insert(uid.clone(), entry);
            }
        }
    }
    thread.theme_data = data
        .get("theme_data")
        .filter(|v| !v.is_null())
        .cloned();
    thread
}

/// `extract_account`
pub(crate) fn extract_account(data: &Value) -> Account {
    let mut acc = Account {
        pk: get_str(data, "pk").unwrap_or_default(),
        username: get_str(data, "username"),
        full_name: get_str(data, "full_name"),
        profile_pic_url: get_str(data, "profile_pic_url"),
        profile_pic_url_hd: get_str(data, "profile_pic_url_hd"),
        is_private: get_bool(data, "is_private"),
        is_verified: get_bool(data, "is_verified"),
        follower_count: get_i64(data, "follower_count"),
        following_count: get_i64(data, "following_count"),
        media_count: get_i64(data, "media_count"),
        email: get_str(data, "email"),
        phone_number: get_str(data, "phone_number"),
        external_url: get_str(data, "external_url").filter(|u| !u.is_empty()),
    };
    if acc.pk.is_empty() {
        acc.pk = get_str(data, "id").unwrap_or_default();
    }
    acc
}

/// `extract_user_v1`
pub(crate) fn extract_user_v1(data: &Value) -> User {
    User {
        pk: get_str(data, "pk")
            .or_else(|| get_str(data, "id"))
            .unwrap_or_default(),
        username: get_str(data, "username"),
        full_name: get_str(data, "full_name"),
        profile_pic_url: get_str(data, "profile_pic_url"),
        is_private: get_bool(data, "is_private"),
        is_verified: get_bool(data, "is_verified"),
        media_count: get_i64(data, "media_count"),
        follower_count: get_i64(data, "follower_count"),
        following_count: get_i64(data, "following_count"),
    }
}
