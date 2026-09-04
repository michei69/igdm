//! Small helpers ported from `instagrapi.utils`: JSON navigation, the
//! Instagram JSON dumps format, form/query encoding, token/uuid generation.

use std::time::{SystemTime, UNIX_EPOCH};

use rand::{Rng, RngCore};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Navigate one string key.
pub fn get<'a>(data: &'a Value, key: &str) -> Option<&'a Value> {
    data.as_object().and_then(|o| o.get(key))
}

fn value_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn value_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn value_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_i64().map(|i| i != 0),
        Value::String(s) => match s.as_str() {
            "1" | "true" | "True" => Some(true),
            "0" | "false" | "False" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

pub fn get_str(data: &Value, key: &str) -> Option<String> {
    get(data, key).and_then(value_str)
}

pub fn get_i64(data: &Value, key: &str) -> Option<i64> {
    get(data, key).and_then(value_i64)
}

pub fn get_str_m(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(value_str)
}

pub fn get_i64_m(map: &Map<String, Value>, key: &str) -> Option<i64> {
    map.get(key).and_then(value_i64)
}

pub fn get_bool_m(map: &Map<String, Value>, key: &str) -> Option<bool> {
    map.get(key).and_then(value_bool)
}

pub fn get_bool(data: &Value, key: &str) -> Option<bool> {
    get(data, key).and_then(value_bool)
}

/// `InstagrapiJSONEncoder` equivalent: datetimes serialize as unix seconds.
pub fn dumps(data: &Value) -> String {
    serde_json::to_string(data).expect("serialize to string")
}

/// Compact JSON truncated to `max` chars (char-safe), for console previews.
pub fn json_preview(data: &Value, max: usize) -> String {
    let text = dumps(data);
    if text.chars().count() <= max {
        return text;
    }
    let head: String = text.chars().take(max).collect();
    format!("{head}…(+{} chars)", text.chars().count() - max)
}

/// `urllib.parse.quote_plus` equivalent (safe chars: alnum `_.-~`, space -> `+`).
pub fn quote_plus(input: &str) -> String {
    let encoded = percent_encoding::utf8_percent_encode(input, QUOTE_PLUS_SET);
    encoded.to_string().replace("%20", "+")
}

const QUOTE_PLUS_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'_')
    .remove(b'.')
    .remove(b'-')
    .remove(b'~');

/// `requests` form encoding: bool -> "True"/"False", numbers -> decimal.
pub fn form_field(key: &str, value: &Value) -> String {
    let text = match value {
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    format!("{}={}", quote_plus(key), quote_plus(&text))
}

/// Encode a dict as `application/x-www-form-urlencoded` (requests-style).
pub fn form_encode(data: &Map<String, Value>) -> String {
    let mut parts = Vec::with_capacity(data.len());
    for (key, value) in data {
        parts.push(form_field(key, value));
    }
    parts.join("&")
}

pub fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn generate_uuid_prefix(prefix: &str, suffix: &str) -> String {
    format!("{prefix}{}{suffix}", uuid::Uuid::new_v4())
}

/// `generate_android_device_id`: "android-" + sha256(time) hex[:16].
pub fn generate_android_device_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let digest = Sha256::digest(now.to_string().as_bytes());
    format!("android-{}", &hex(&digest)[..16])
}

pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// `gen_token`: random alphanumeric string.
pub fn gen_token(size: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..size)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}

/// `generate_jazoest`: "2" + sum of char codes.
pub fn generate_jazoest(symbols: &str) -> String {
    let amount: u32 = symbols.chars().map(|c| c as u32).sum();
    format!("2{amount}")
}

/// `generate_mutation_token`: random 19-digit number in the IG range.
pub fn generate_mutation_token() -> String {
    let mut rng = rand::thread_rng();
    let lo: u64 = 6_800_011_111_111_111_111;
    let hi: u64 = 6_800_099_999_999_999_999;
    rng.gen_range(lo..=hi).to_string()
}

/// Random 16 hex chars (`secrets.token_hex(16)`).
pub fn random_hex(size: usize) -> String {
    let mut bytes = vec![0u8; size];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex(&bytes)
}

/// Current unix time in seconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Current unix time in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
