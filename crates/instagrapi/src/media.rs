//! Media handling ported from instagrapi: `prepare_image` (JPEG normalize),
//! MP4 metadata parsing (`utils/video.py`), FB rupload for direct photos and
//! videos, and plain media downloads.

use std::io::Cursor;
use std::path::Path;

use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader};
use serde_json::{json, Map, Value};

use crate::client::{Body, Client, Req};
use crate::error::{ErrorKind, IgError, Result};
use crate::extract::extract_direct_message;
use crate::types::DirectMessage;
use crate::utils::{dumps, generate_mutation_token, now_ms, random_hex};

/// First 300 chars of a response body for error messages (char-safe: slicing
/// by byte index can panic on multi-byte UTF-8).
fn truncate_300(text: &str) -> &str {
    let end = text.char_indices().nth(300).map_or(text.len(), |(i, _)| i);
    &text[..end]
}

// ------------------------------------------------------------------ images

/// `calc_crop` — center crop to keep the aspect ratio within [min, max].
fn calc_crop(
    min_aspect: f64,
    max_aspect: f64,
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    let curr = width as f64 / height as f64;
    if curr >= min_aspect && curr <= max_aspect {
        return None;
    }
    if curr > max_aspect {
        let new_height = height;
        let new_width = (max_aspect * new_height as f64) as u32;
        let left = (width - new_width) / 2;
        let top = 0;
        Some((left, top, left + new_width, top + new_height))
    } else {
        let new_width = width;
        let new_height = (new_width as f64 / min_aspect) as u32;
        let left = 0;
        let top = (height - new_height) / 2;
        Some((left, top, left + new_width, top + new_height))
    }
}

/// `calc_resize` — fit within max_size, upscale from min_size.
fn calc_resize(max: (u32, u32), min: (u32, u32), width: u32, height: u32) -> Option<(u32, u32)> {
    let (max_width, max_height) = max;
    let (min_width, min_height) = min;
    if (max_width > 0 && min_width > max_width) || (max_height > 0 && min_height > max_height) {
        return None;
    }
    if max_width > 0 && max_height > 0 && (width > max_width || height > max_height) {
        let factor = (max_width as f64 / width as f64).min(max_height as f64 / height as f64);
        let new_width = (factor * width as f64) as u32;
        let new_height = (factor * height as f64) as u32;
        return Some((new_width, new_height));
    }
    if min_width > 0 && min_height > 0 && (width < min_width || height < min_height) {
        let factor = (min_width as f64 / width as f64).max(min_height as f64 / height as f64);
        let new_width = (factor * width as f64) as u32;
        let new_height = (factor * height as f64) as u32;
        return Some((new_width, new_height));
    }
    None
}

/// Alpha-composite `fg` over `bg` (both 0-255), matching the official client.
fn blend(bg: u8, fg: u8, alpha: u8) -> u8 {
    ((bg as u16 * (255 - alpha as u16) + fg as u16 * alpha as u16) / 255) as u8
}

/// `prepare_image` — decode, crop to 4:5..90:47, fit 1080x1350, JPEG encode.
pub fn prepare_image(path: &Path) -> Result<Vec<u8>> {
    let img = ImageReader::open(path)
        .map_err(|e| IgError::client_error(format!("prepare_image open: {e}")))?
        .decode()
        .map_err(|e| IgError::client_error(format!("prepare_image decode: {e}")))?;
    let (width, height) = (img.width(), img.height());
    let mut img = img;
    if let Some((left, top, right, bottom)) = calc_crop(4.0 / 5.0, 90.0 / 47.0, width, height) {
        img = img.crop_imm(left, top, right - left, bottom - top);
    }
    if let Some((new_width, new_height)) =
        calc_resize((1080, 1350), (320, 167), img.width(), img.height())
    {
        img = img.resize(new_width, new_height, FilterType::Triangle);
    }
    // RGB flatten (white background for alpha).
    let rgb = match &img {
        DynamicImage::ImageRgba8(_) => {
            let rgba = img.to_rgba8();
            let mut canvas = image::RgbImage::from_pixel(
                rgba.width(),
                rgba.height(),
                image::Rgb([255, 255, 255]),
            );
            for (dst, src) in canvas.pixels_mut().zip(rgba.pixels()) {
                let a = src[3];
                if a == 255 {
                    dst.0 = [src[0], src[1], src[2]];
                } else if a > 0 {
                    dst.0 = [
                        blend(dst.0[0], src[0], a),
                        blend(dst.0[1], src[1], a),
                        blend(dst.0[2], src[2], a),
                    ];
                }
            }
            DynamicImage::ImageRgb8(canvas)
        }
        _ => img.to_rgb8().into(),
    };
    let mut out = Cursor::new(Vec::new());
    rgb.write_to(&mut out, ImageFormat::Jpeg)
        .map_err(|e| IgError::client_error(format!("prepare_image encode: {e}")))?;
    Ok(out.into_inner())
}

// ------------------------------------------------------------------ videos

/// MP4 metadata (ported `utils/video.py`): moov -> mvhd/trak boxes.
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub duration: f64,
}

/// Walk top-level ISO-BMFF boxes, yielding `(box_type, payload)` slices
/// without copying any payload.
fn iter_boxes(data: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let size =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let box_type = &data[pos + 4..pos + 8];
        let mut header_size = 8usize;
        let mut size = size;
        if size == 1 {
            if pos + 16 > data.len() {
                break;
            }
            size = u64::from_be_bytes([
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
                data[pos + 12],
                data[pos + 13],
                data[pos + 14],
                data[pos + 15],
            ]) as usize;
            header_size = 16;
        } else if size == 0 {
            size = data.len() - pos;
        }
        if size < header_size || pos + size > data.len() {
            break;
        }
        let box_end = pos + size;
        out.push((box_type, &data[pos + header_size..box_end]));
        pos = box_end;
    }
    out
}

fn read_moov(data: &[u8]) -> Option<&[u8]> {
    iter_boxes(data)
        .into_iter()
        .find(|(t, _)| *t == b"moov")
        .map(|(_, payload)| payload)
}

fn parse_mvhd(data: &[u8]) -> Option<f64> {
    if data.is_empty() {
        return None;
    }
    let version = data[0];
    let (timescale, duration) = if version == 0 {
        if data.len() < 20 {
            return None;
        }
        (
            u32::from_be_bytes(data[12..16].try_into().ok()?),
            u64::from_be_bytes([0, 0, 0, 0, data[16], data[17], data[18], data[19]]),
        )
    } else if version == 1 {
        if data.len() < 32 {
            return None;
        }
        (
            u32::from_be_bytes(data[20..24].try_into().ok()?),
            u64::from_be_bytes(data[24..32].try_into().ok()?),
        )
    } else {
        return None;
    };
    if timescale == 0 {
        return None;
    }
    Some(duration as f64 / timescale as f64)
}

fn parse_tkhd(data: &[u8]) -> Option<(u32, u32)> {
    if data.is_empty() {
        return None;
    }
    let version = data[0];
    let offset = match version {
        0 => 76,
        1 => 88,
        _ => return None,
    };
    if data.len() < offset + 8 {
        return None;
    }
    let width_fixed = u32::from_be_bytes(data[offset..offset + 4].try_into().ok()?);
    let height_fixed = u32::from_be_bytes(data[offset + 4..offset + 8].try_into().ok()?);
    let width = (width_fixed as f64 / 65536.0).round() as u32;
    let height = (height_fixed as f64 / 65536.0).round() as u32;
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

fn parse_hdlr(data: &[u8]) -> Option<&[u8]> {
    data.get(8..12)
}

/// A parsed track inside `moov`: (hdlr handler, tkhd dimensions, mdhd duration).
type TrackInfo<'a> = (Option<&'a [u8]>, Option<(u32, u32)>, Option<f64>);

/// `read_video_metadata` — pure-Rust MP4 box walk.
pub fn read_video_metadata(data: &[u8]) -> Result<VideoMetadata> {
    let moov = read_moov(data)
        .ok_or_else(|| IgError::client_error("MP4 metadata box 'moov' was not found"))?;
    let mut movie_duration: Option<f64> = None;
    let mut tracks: Vec<TrackInfo<'_>> = Vec::new();
    for (box_type, payload) in iter_boxes(moov) {
        if box_type == b"mvhd" {
            movie_duration = parse_mvhd(payload);
        } else if box_type == b"trak" {
            let mut handler: Option<&[u8]> = None;
            let mut size: Option<(u32, u32)> = None;
            let mut duration: Option<f64> = None;
            for (tbox, tpayload) in iter_boxes(payload) {
                if tbox == b"tkhd" {
                    size = parse_tkhd(tpayload);
                } else if tbox == b"mdia" {
                    for (mbox, mpayload) in iter_boxes(tpayload) {
                        if mbox == b"mdhd" {
                            duration = parse_mvhd(mpayload);
                        } else if mbox == b"hdlr" {
                            handler = parse_hdlr(mpayload);
                        }
                    }
                }
            }
            tracks.push((handler, size, duration));
        }
    }
    let video_track = tracks
        .iter()
        .find(|(h, s, _)| *h == Some(b"vide".as_slice()) && s.is_some())
        .or_else(|| tracks.iter().find(|(_, s, _)| s.is_some()));
    let (_, size, track_duration) = video_track
        .ok_or_else(|| IgError::client_error("MP4 video track dimensions were not found"))?;
    let (width, height) =
        size.ok_or_else(|| IgError::client_error("MP4 video track dimensions were not found"))?;
    let duration = movie_duration
        .or(*track_duration)
        .ok_or_else(|| IgError::client_error("MP4 video duration was not found"))?;
    Ok(VideoMetadata {
        width,
        height,
        duration,
    })
}

// ----------------------------------------------------------------- rupload

/// `_messenger_rupload_headers` — FB rupload allow-list (no X-IG-* leak).
async fn messenger_rupload_headers(client: &Client) -> Result<reqwest::header::HeaderMap> {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    // NOTE: `state()` returns a MutexGuard; tokio mutexes are not
    // reentrant, so the guard must be dropped before any call that locks
    // state again (`authorization()`). Holding it across that call
    // deadlocks the task and, because the guard is never released, every
    // subsequent `private_request` (all sends) blocks forever.
    let state = client.state().await;
    let user_id = state.user_id().unwrap_or(0).to_string();
    let rur = state.ig_u_rur.clone();
    let mid = state.mid.clone();
    let user_agent = state.user_agent.clone();
    drop(state);
    let auth = client.authorization().await;
    if !auth.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&auth) {
            headers.insert("authorization", v);
        }
    }
    if let Ok(v) = HeaderValue::from_str(&user_id) {
        headers.insert("ig-intended-user-id", v.clone());
        headers.insert("ig-u-ds-user-id", v);
    }
    headers.insert("accept-encoding", HeaderValue::from_static("gzip"));
    headers.insert("accept-language", HeaderValue::from_static("en-US"));
    headers.insert("priority", HeaderValue::from_static("u=6, i"));
    if let Ok(v) = HeaderValue::from_str(&user_agent) {
        headers.insert("user-agent", v);
    }
    headers.insert("x-fb-client-ip", HeaderValue::from_static("True"));
    headers.insert(
        "x-fb-friendly-name",
        HeaderValue::from_static("undefined:media-upload"),
    );
    headers.insert(
        "x-fb-http-engine",
        HeaderValue::from_static("Tigon/MNS/TCP"),
    );
    headers.insert(
        "x-fb-request-analytics-tags",
        HeaderValue::from_static(
            "{\"network_tags\":{\"product\":\"567067343352427\",\"surface\":\"undefined\",\
             \"request_category\":\"media_upload\",\"purpose\":\"none\",\"retry_attempt\":\"0\"}}",
        ),
    );
    headers.insert("x-fb-rmd", HeaderValue::from_static("state=URL_ELIGIBLE"));
    headers.insert("x-fb-server-cluster", HeaderValue::from_static("True"));
    headers.insert("x-tigon-is-retry", HeaderValue::from_static("False"));
    headers.insert("x-ig-salt-ids", HeaderValue::from_static("51052545"));
    if !rur.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&rur) {
            headers.insert("ig-u-rur", v);
        }
    }
    if !mid.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&mid) {
            headers.insert("x-mid", v);
        }
    }
    Ok(headers)
}

/// Parse the rupload response's `media_id` (int or numeric string).
fn parse_media_id(messenger: &str, text: &str) -> Result<i64> {
    let json: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    json.get("media_id")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .ok_or_else(|| {
            IgError::client_error(format!(
                "{messenger} response missing media_id: {}",
                truncate_300(text)
            ))
        })
}

/// One resumable FB rupload session — the flow shared by photo/video/voice:
/// optional offset GET, then POST of the remaining bytes, then `media_id`
/// parse. Per-type differences (edge path, entity type, headers, timeouts)
/// live here.
struct RuploadSpec<'a> {
    /// rupload edge path, e.g. `messenger_image` (also the error prefix).
    messenger: &'static str,
    /// `x-entity-type` value for the POST.
    entity_type: &'static str,
    /// Probe the resumable offset with a GET before uploading.
    offset_get: bool,
    /// Seconds allowed for the POST.
    post_timeout: u64,
    /// Media-specific headers, inserted before the offset GET.
    headers: &'static [(&'static str, &'static str)],
    /// `x_fb_video_waterfall_id` (video only).
    waterfall_id: Option<&'a str>,
}

/// Shared photo/video/voice upload: build headers, optionally GET the
/// resumable offset, POST the remaining bytes, parse the `media_id`.
async fn rupload(
    client: &Client,
    bytes: &[u8],
    entity_name: &str,
    spec: RuploadSpec<'_>,
) -> Result<i64> {
    use reqwest::header::HeaderValue;
    let url = format!("https://rupload.facebook.com/{}/{entity_name}", spec.messenger);
    let mut headers = messenger_rupload_headers(client).await?;
    for &(name, value) in spec.headers {
        headers.insert(name, HeaderValue::from_static(value));
    }
    if let Some(waterfall_id) = spec.waterfall_id {
        headers.insert(
            "x_fb_video_waterfall_id",
            HeaderValue::from_str(waterfall_id).unwrap(),
        );
    }

    // 1. resumable offset (photo uploads skip the probe and start at 0)
    let offset: usize = if spec.offset_get {
        let offset_resp = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            client.upload_http().get(&url).headers(headers.clone()).send(),
        )
        .await
        .map_err(|_| {
            IgError::new(
                ErrorKind::ClientRequestTimeout,
                format!("{} offset timeout", spec.messenger),
            )
        })?
        .map_err(IgError::from)?;
        let status = offset_resp.status();
        let text = offset_resp.text().await.unwrap_or_default();
        if status != 200 {
            return Err(IgError::client_error(format!(
                "{} offset GET failed: {status} {}",
                spec.messenger,
                truncate_300(&text)
            )));
        }
        serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("offset").and_then(|o| o.as_i64()))
            .unwrap_or(0) as usize
    } else {
        0
    };

    // 2. POST the remaining bytes
    headers.insert(
        "content-type",
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        "offset",
        HeaderValue::from_str(&offset.to_string()).unwrap(),
    );
    headers.insert(
        "x-entity-length",
        HeaderValue::from_str(&bytes.len().to_string()).unwrap(),
    );
    headers.insert("x-entity-name", HeaderValue::from_str(entity_name).unwrap());
    headers.insert("x-entity-type", HeaderValue::from_static(spec.entity_type));

    let body = if offset < bytes.len() {
        bytes[offset..].to_vec()
    } else {
        Vec::new()
    };
    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(spec.post_timeout),
        client.upload_http().post(&url).headers(headers).body(body).send(),
    )
    .await
    .map_err(|_| {
        IgError::new(
            ErrorKind::ClientRequestTimeout,
            format!("{} upload timeout", spec.messenger),
        )
    })?
    .map_err(IgError::from)?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status != 200 {
        return Err(IgError::client_error(format!(
            "{} upload POST failed: {status} {}",
            spec.messenger,
            truncate_300(&text)
        )));
    }
    parse_media_id(spec.messenger, &text)
}

/// `_photo_rupload` — POST jpeg bytes to messenger_image, return media_id.
async fn photo_rupload(client: &Client, photo_bytes: Vec<u8>, entity_name: &str) -> Result<i64> {
    let media_id = rupload(
        client,
        &photo_bytes,
        entity_name,
        RuploadSpec {
            messenger: "messenger_image",
            entity_type: "image/jpeg",
            offset_get: false,
            post_timeout: 120,
            headers: &[("image_type", "FILE_ATTACHMENT")],
            waterfall_id: None,
        },
    )
    .await?;
    eprintln!("[igdm-media] messenger_image upload ok (media_id={media_id})");
    Ok(media_id)
}

/// `_video_rupload` — resumable GET offset + POST mp4 bytes.
async fn video_rupload(
    client: &Client,
    video_bytes: &[u8],
    entity_name: &str,
    waterfall_id: &str,
) -> Result<i64> {
    rupload(
        client,
        video_bytes,
        entity_name,
        RuploadSpec {
            messenger: "messenger_video",
            entity_type: "video/mp4",
            offset_get: true,
            post_timeout: 300,
            headers: &[
                ("video_type", "FILE_ATTACHMENT"),
                ("segment-start-offset", "0"),
                ("segment-type", "3"),
                ("ephemeral_media_view_mode", "2"),
                ("ig_raven_metadata", "{}"),
            ],
            waterfall_id: Some(waterfall_id),
        },
    )
    .await
}

/// `_voice_rupload` — resumable GET offset + POST audio bytes to
/// messenger_audio, return media_id.
async fn voice_rupload(client: &Client, audio_bytes: &[u8], entity: &str) -> Result<i64> {
    rupload(
        client,
        audio_bytes,
        entity,
        RuploadSpec {
            messenger: "messenger_audio",
            entity_type: "audio/mp4",
            offset_get: true,
            post_timeout: 120,
            headers: &[("audio_type", "FILE_ATTACHMENT")],
            waterfall_id: None,
        },
    )
    .await
}

// ---------------------------------------------------------------- sending

impl Client {
    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.inner.http
    }

    /// HTTP/1.1 client used only for FB rupload uploads (mirrors instagrapi's
    /// dedicated `requests.Session`; the rupload edge is HTTP/1.1-oriented).
    pub(crate) fn upload_http(&self) -> &reqwest::Client {
        &self.inner.http_h1
    }

    /// Shared direct-media broadcast envelope: the per-session state clones
    /// (`uuid`, `android_device_id`, `user_id`, `csrftoken`), the mutation
    /// token, and the form fields every media sender shares (`thread_ids`,
    /// `attachment_fbid`, `device_id`, `_uuid`, `_uid`, `_csrftoken`,
    /// `client_context`, `mutation_token`). Senders add their type-specific
    /// fields around it; the token return feeds `offline_threading_id`.
    async fn broadcast_seed(
        &self,
        thread_ids: &[&str],
        media_id: i64,
    ) -> (String, Map<String, Value>) {
        let state = self.state().await;
        let uuid = state.uuid.clone();
        let android_device_id = state.android_device_id.clone();
        let user_id = state.user_id().unwrap_or(0).to_string();
        let csrftoken = state.token();
        drop(state);
        let token = generate_mutation_token();
        let mut data = Map::new();
        data.insert(
            "thread_ids".to_string(),
            json!(dumps(&Value::Array(
                thread_ids.iter().map(|t| json!(t.to_string())).collect()
            ))),
        );
        data.insert("client_context".to_string(), json!(token.clone()));
        data.insert("attachment_fbid".to_string(), json!(media_id.to_string()));
        data.insert("device_id".to_string(), json!(android_device_id));
        data.insert("mutation_token".to_string(), json!(token.clone()));
        data.insert("_uuid".to_string(), json!(uuid));
        data.insert("_uid".to_string(), json!(user_id));
        data.insert("_csrftoken".to_string(), json!(csrftoken));
        (token, data)
    }

    /// The broadcast response's `payload`, extracted as a `DirectMessage`.
    fn broadcast_payload(result: &Value, what: &str) -> Result<DirectMessage> {
        let payload = result
            .get("payload")
            .ok_or_else(|| IgError::client_error(format!("{what}: missing payload")))?;
        Ok(extract_direct_message(payload))
    }

    /// `direct_send_photo` — normalize JPEG -> rupload -> photo_attachment.
    pub async fn direct_send_photo(
        &self,
        path: &Path,
        thread_ids: &[&str],
    ) -> Result<DirectMessage> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp") {
            return Err(IgError::client_error(
                "Invalid file format. Only JPG/JPEG/PNG/WEBP files are supported.",
            ));
        }
        let photo_bytes = prepare_image(path)?;
        let entity_name = format!("fb_uploader_{}", now_ms());
        let media_id = photo_rupload(self, photo_bytes, &entity_name).await?;

        let (token, mut data) = self.broadcast_seed(thread_ids, media_id).await;
        data.insert("action".to_string(), json!("send_item"));
        data.insert("is_x_transport_forward".to_string(), json!("false"));
        data.insert("is_shh_mode".to_string(), json!("0"));
        data.insert("send_attribution".to_string(), json!("inbox"));
        data.insert("allow_full_aspect_ratio".to_string(), json!("true"));
        data.insert("btt_dual_send".to_string(), json!("false"));
        data.insert("is_ae_dual_send".to_string(), json!("false"));
        data.insert("offline_threading_id".to_string(), json!(token));
        eprintln!("[igdm-media] photo broadcast: posting photo_attachment (media_id={media_id})");
        let result = self
            .private_request(
                "direct_v2/threads/broadcast/photo_attachment/",
                Some(Body::Form(Value::Object(data))),
                Req::default(),
            )
            .await?;
        eprintln!("[igdm-media] photo broadcast ok: {}", crate::utils::json_preview(&result, 300));
        Self::broadcast_payload(&result, "direct_send_photo")
    }

    /// `direct_send_video` — mp4 metadata -> rupload -> raven_attachment.
    pub async fn direct_send_video(
        &self,
        path: &Path,
        thread_ids: &[&str],
    ) -> Result<DirectMessage> {
        let video_bytes = tokio::fs::read(path).await?;
        let metadata = read_video_metadata(&video_bytes)?;
        let size = video_bytes.len();

        let hex_id = random_hex(16);
        let ms = now_ms();
        let entity = format!("{hex_id}-0-{size}-{ms}-{ms}");
        let upload_id: i64 = {
            use rand::Rng as _;
            rand::thread_rng().gen_range(100_000_000_000i64..1_000_000_000_000)
        };
        let waterfall_id = format!("{upload_id}_{}_Mixed_0", hex_id[..12].to_uppercase());
        let media_id = video_rupload(self, &video_bytes, &entity, &waterfall_id).await?;

        let width = metadata.width.to_string();
        let height = metadata.height.to_string();
        let duration = metadata.duration;
        let (_token, mut data) = self.broadcast_seed(thread_ids, media_id).await;
        data.insert("recipient_users".to_string(), json!("[]"));
        data.insert("view_mode".to_string(), json!("permanent"));
        data.insert("has_camera_metadata".to_string(), json!("1"));
        data.insert("camera_entry_point".to_string(), json!("3"));
        data.insert("reshare_mode".to_string(), json!("allow_reshare"));
        data.insert("original_media_type".to_string(), json!("2"));
        data.insert("send_attribution".to_string(), json!("direct_composer"));
        data.insert(
            "camera_session_id".to_string(),
            json!(crate::utils::generate_uuid()),
        );
        data.insert("include_e2ee_mentioned_user_list".to_string(), json!("1"));
        data.insert("hide_from_profile_grid".to_string(), json!("false"));
        data.insert("timezone_offset".to_string(), json!("0"));
        data.insert(
            "client_shared_at".to_string(),
            json!(crate::utils::now().to_string()),
        );
        data.insert("configure_mode".to_string(), json!("2"));
        data.insert("source_type".to_string(), json!("3"));
        data.insert("camera_position".to_string(), json!("back"));
        data.insert("video_result".to_string(), json!(media_id.to_string()));
        data.insert(
            "composition_id".to_string(),
            json!(crate::utils::generate_uuid()),
        );
        data.insert("creation_surface".to_string(), json!("camera"));
        data.insert("has_ig_camera_edits".to_string(), json!("false"));
        data.insert("capture_type".to_string(), json!("normal"));
        data.insert("audience".to_string(), json!("default"));
        data.insert("upload_id".to_string(), json!(upload_id.to_string()));
        data.insert(
            "client_timestamp".to_string(),
            json!(crate::utils::now().to_string()),
        );
        data.insert(
            "media_transformation_info".to_string(),
            json!(dumps(&json!({
                "width": width,
                "height": height,
                "x_transform": "0",
                "y_transform": "0",
                "zoom": "1.0",
                "rotation": "0.0",
                "background_coverage": "0.0",
            }))),
        );
        data.insert(
            "clips".to_string(),
            json!([{"length": duration, "source_type": "3", "camera_position": "back"}]),
        );
        data.insert("poster_frame_index".to_string(), json!(0));
        data.insert("length".to_string(), json!(duration));
        data.insert("audio_muted".to_string(), json!(false));
        data.insert(
            "edits".to_string(),
            json!({"filter_type": 0, "filter_strength": 1.0}),
        );
        data.insert(
            "extra".to_string(),
            json!({"source_width": metadata.width, "source_height": metadata.height}),
        );
        data.insert(
            "device".to_string(),
            json!({
                "manufacturer": "Google",
                "model": "sdk_gphone_arm64",
                "android_version": 30,
                "android_release": "11",
            }),
        );
        let result = self
            .private_request(
                "direct_v2/threads/broadcast/raven_attachment/?video=1",
                Some(Body::Form(Value::Object(data))),
                Req::signed(),
            )
            .await?;
        Self::broadcast_payload(&result, "direct_send_video")
    }

    /// `direct_send_voice` — rupload m4a -> voice_attachment broadcast.
    ///
    /// Mirrors `instagrapi.direct_send_voice`: upload AAC-in-MP4 bytes to
    /// messenger_audio, then broadcast `voice_attachment/` with the returned
    /// `media_id` as `attachment_fbid`. The server rejects non-m4a formats at
    /// upload_finish, so input must be AAC in an MP4 container. The waveform
    /// is cosmetic (server does not validate amplitudes).
    pub async fn direct_send_voice(&self, path: &Path, thread_ids: &[&str]) -> Result<DirectMessage> {
        let audio_bytes = tokio::fs::read(path).await?;
        let upload_id = now_ms().to_string();
        let rand_key: i64 = {
            use rand::Rng as _;
            rand::thread_rng().gen_range(-2_147_483_648i64..2_147_483_648)
        };
        let entity = format!("{upload_id}_0_{rand_key}");
        let media_id = voice_rupload(self, &audio_bytes, &entity).await?;

        let (token, mut data) = self.broadcast_seed(thread_ids, media_id).await;
        let waveform: Vec<f64> = {
            use rand::Rng as _;
            let mut rng = rand::thread_rng();
            (0..70)
                .map(|_| {
                    let v: f64 = rng.gen_range(0.2..0.95);
                    (v * 1000.0).round() / 1000.0
                })
                .collect()
        };
        data.insert("action".to_string(), json!("send_item"));
        data.insert("send_attribution".to_string(), json!("inbox"));
        data.insert("waveform".to_string(), json!(dumps(&json!(waveform))));
        data.insert("waveform_sampling_frequency_hz".to_string(), json!("10"));
        data.insert("upload_id".to_string(), json!(upload_id));
        data.insert("offline_threading_id".to_string(), json!(token));
        let result = self
            .private_request(
                "direct_v2/threads/broadcast/voice_attachment/",
                Some(Body::Form(Value::Object(data))),
                Req::default(),
            )
            .await?;
        Self::broadcast_payload(&result, "direct_send_voice")
    }

    /// `media_info_v1` — full media item from `media/{pk}/info/`. Used to
    /// resolve a reel share's video/caption: the `xma_clip` share payload
    /// only carries a static preview, and the actual mp4 lives here.
    pub async fn media_info(&self, media_pk: &str) -> Result<Value> {
        let result = self
            .private_request(&format!("media/{media_pk}/info/"), None, Req::signed())
            .await?;
        let items = result
            .get("items")
            .and_then(|i| i.as_array())
            .ok_or_else(|| IgError::client_error("media_info: missing items"))?;
        items
            .first()
            .cloned()
            .ok_or_else(|| IgError::client_error("media_info: empty items"))
    }

    /// `user_stories_v1` — the author's active stories
    /// (`feed/user/{uid}/story/`). Fetching a story reel does NOT mark the
    /// stories as seen — only explicit `story_seen` calls do.
    pub async fn user_stories(&self, user_id: &str) -> Result<Value> {
        let mut params = Map::new();
        params.insert(
            "supported_capabilities_new".to_string(),
            json!(crate::config::SUPPORTED_CAPABILITIES),
        );
        self.private_request(
            &format!("feed/user/{user_id}/story/"),
            None,
            Req::signed().params(Some(&params)),
        )
        .await
    }

    /// `story_info` — one story item (with `video_versions`) by pk, resolved
    /// from the author's active reel. Does not mark the story as seen.
    pub async fn story_info(&self, story_pk: &str, owner_user_id: &str) -> Result<Value> {
        let reel = self.user_stories(owner_user_id).await?;
        let items = reel
            .get("reel")
            .and_then(|r| r.get("items"))
            .and_then(|i| i.as_array())
            .ok_or_else(|| IgError::client_error("story_info: missing reel items"))?;
        items
            .iter()
            .find(|it| {
                it.get("item_id").and_then(|v| v.as_str()) == Some(story_pk)
                    || it.get("id").and_then(|v| v.as_str()) == Some(story_pk)
                    || it.get("pk").and_then(|v| v.as_str()) == Some(story_pk)
            })
            .cloned()
            .ok_or_else(|| {
                IgError::client_error(format!("story_info: story {story_pk} not found"))
            })
    }

    /// `media_comments` — one page of comments for a media item
    /// (`media/{pk}/comments/`). `max_id` paginates (`next_max_id` from the
    /// previous page). The raw response is returned; the UI reads the
    /// `comments` array plus `next_max_id`/`has_more_comments`.
    pub async fn media_comments(&self, media_pk: &str, max_id: Option<&str>) -> Result<Value> {
        let params = max_id.map(|m| {
            let mut p = Map::new();
            p.insert("max_id".to_string(), json!(m));
            p
        });
        self.private_request(
            &format!("media/{media_pk}/comments/"),
            None,
            Req::signed().params(params.as_ref()),
        )
        .await
    }

    /// Download a CDN media URL into `dest_dir`, returning the saved path.
    pub async fn download_media(&self, url: &str, dest_dir: &Path) -> Result<std::path::PathBuf> {
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.http().get(url).send(),
        )
        .await
        .map_err(|_| IgError::new(ErrorKind::ClientRequestTimeout, "media download timeout"))?
        .map_err(IgError::from)?;
        let status = response.status();
        if !status.is_success() {
            return Err(IgError::client_error(format!(
                "media download failed: {status}"
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = response.bytes().await.map_err(IgError::from)?;
        let ext = if content_type.contains("video/mp4") {
            ".mp4"
        } else if content_type.contains("audio") {
            ".m4a"
        } else if content_type.contains("webp") {
            ".webp"
        } else {
            ".jpg"
        };
        let name = format!("{}{ext}", stable_hash(url));
        let path = dest_dir.join(name);
        if !path.exists() {
            tokio::fs::write(&path, &bytes).await?;
        }
        Ok(path)
    }
}

/// Deterministic replacement for Python's salted `abs(hash(url))`.
fn stable_hash(url: &str) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(url.as_bytes());
    crate::utils::hex(&digest[..8])
}
