//! Login flows ported from `instagrapi.mixins.auth` + `mixins.challenge`:
//! password login (with pre/post flows), two-factor, challenge resolution
//! (email/SMS verification code via the interactive handler), sessionid
//! login, session persistence and logout.

use std::path::Path;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::Engine as _;
use rsa::pkcs1v15::Pkcs1v15Encrypt;
use rsa::{pkcs8::DecodePublicKey, RsaPublicKey};
use serde_json::{json, Map, Value};
use tokio::time::Duration;

use crate::client::{Body, Client, Req};
use crate::config;
use crate::error::{ErrorKind, IgError, Result};
use crate::extract::{extract_account, extract_user_short, extract_user_v1};
use crate::types::{Account, UserShort};
use crate::utils::{dumps, generate_jazoest, generate_uuid, now};

const WAIT_SECONDS: u64 = 5;
const CHALLENGE_ATTEMPTS: usize = 24;

/// `password_publickeys` — fetch the RSA key for `enc_password` from the
/// public `qe/sync` endpoint (response headers).
async fn password_publickeys(client: &Client) -> Result<(i64, String)> {
    let (status, headers, _body) = client
        .public_get("https://i.instagram.com/api/v1/qe/sync/")
        .await?;
    if status != 200 {
        return Err(IgError::client_error(format!(
            "password_publickeys qe/sync failed: HTTP {status}"
        )));
    }
    let key_id = headers
        .get("ig-set-password-encryption-key-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or_else(|| IgError::client_error("missing ig-set-password-encryption-key-id"))?;
    let pub_key = headers
        .get("ig-set-password-encryption-pub-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| IgError::client_error("missing ig-set-password-encryption-pub-key"))?
        .to_string();
    Ok((key_id, pub_key))
}

/// `password_encrypt` — `#PWD_INSTAGRAM:4:` AES-256-GCM + RSA PKCS#1 v1.5.
pub(crate) async fn password_encrypt(client: &Client, password: &str) -> Result<String> {
    let (publickeyid, publickey) = password_publickeys(client).await?;
    let session_key: [u8; 32] = rand::random();
    let iv: [u8; 12] = rand::random();
    let timestamp = now().to_string();

    let decoded_publickey = base64::engine::general_purpose::STANDARD
        .decode(publickey.as_bytes())
        .map_err(|e| IgError::client_error(format!("bad RSA pubkey base64: {e}")))?;
    let recipient_key = RsaPublicKey::from_public_key_der(&decoded_publickey)
        .map_err(|e| IgError::client_error(format!("bad RSA pubkey DER: {e}")))?;

    let mut rng = rand::thread_rng();
    let rsa_encrypted = recipient_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, &session_key)
        .map_err(|e| IgError::client_error(format!("RSA encrypt failed: {e}")))?;

    let cipher = Aes256Gcm::new_from_slice(&session_key)
        .map_err(|e| IgError::client_error(format!("AES key: {e}")))?;
    let aes_encrypted = cipher
        .encrypt(
            Nonce::from_slice(&iv),
            Payload {
                msg: password.as_bytes(),
                aad: timestamp.as_bytes(),
            },
        )
        .map_err(|e| IgError::client_error(format!("AES encrypt failed: {e}")))?;
    // aes-gcm returns ciphertext || tag; split the 16-byte tag back off.
    let split = aes_encrypted.len().saturating_sub(16);
    let (aes_ciphertext, tag) = aes_encrypted.split_at(split);

    let mut payload = Vec::new();
    payload.push(0x01);
    payload.push((publickeyid & 0xff) as u8);
    payload.extend_from_slice(&iv);
    payload.extend_from_slice(&(rsa_encrypted.len() as u16).to_le_bytes());
    payload.extend_from_slice(&rsa_encrypted);
    payload.extend_from_slice(tag);
    payload.extend_from_slice(aes_ciphertext);

    Ok(format!(
        "#PWD_INSTAGRAM:4:{timestamp}:{}",
        base64::engine::general_purpose::STANDARD.encode(payload)
    ))
}

/// `sync_launcher(login=True)` — pre-login device sync.
async fn sync_launcher(client: &Client) -> Result<Value> {
    let mut data = Map::new();
    data.insert("id".to_string(), json!(client.inner_uuid().await));
    data.insert("server_config_retrieval".to_string(), json!("1"));
    client
        .private_request(
            "launcher/sync/",
            Some(Body::Form(Value::Object(data))),
            Req::signed().login(true),
        )
        .await
}

/// `get_reels_tray_feed("cold_start")` — post-login flow.
async fn get_reels_tray_feed(client: &Client) -> Result<Value> {
    let state = client.state().await;
    let mut data = Map::new();
    data.insert(
        "supported_capabilities_new".to_string(),
        serde_json::from_str(config::SUPPORTED_CAPABILITIES).unwrap_or(Value::Null),
    );
    data.insert("reason".to_string(), json!("cold_start"));
    data.insert(
        "timezone_offset".to_string(),
        json!(state.timezone_offset.to_string()),
    );
    data.insert(
        "tray_session_id".to_string(),
        json!(state.tray_session_id.clone()),
    );
    data.insert("request_id".to_string(), json!(state.request_id.clone()));
    data.insert("page_size".to_string(), json!(50));
    data.insert("_uuid".to_string(), json!(state.uuid.clone()));
    data.insert("reel_tray_impressions".to_string(), json!({}));
    drop(state);
    client
        .private_request(
            "feed/reels_tray/",
            Some(Body::Form(Value::Object(data))),
            Req::signed(),
        )
        .await
}

/// `_timeline_session_level_signals_json` — parsed once, reused per login.
fn session_level_signals() -> Value {
    static SIGNALS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SIGNALS
        .get_or_init(|| {
            serde_json::from_str(
                r#"{"time_since_current_surface_session_start":0,"time_since_fg_session_start":0,"time_since_last_background":0,"num_ad_seen_current_surface_current_session":0,"app_entry":"normal","last_surfaces_visited_current_session":[],"video_play_count":0,"video_pause_count":0,"video_dwell_time_sum":0,"video_dwell_time_max":0,"video_view_count":0,"video_intentional_audio_on":0,"video_intentional_audio_off":0,"video_audio_on_count":0,"feed_to_reels_iv_entry":0,"time_since_last_ad_click":-1,"time_since_last_ad_like":-1,"time_since_last_organic_like":-1,"time_since_last_like":-1,"time_since_last_organic_business_profile_visit":-1,"time_since_last_ad_imp":-1,"time_since_last_search":-1,"time_since_last_organic_engagement_event":-1,"time_since_last_ad_profile_visit":-1,"time_since_last_ad_cta":-1,"time_since_last_ad_caption_more_click":-1,"time_since_last_ad_comment_button":-1,"time_since_last_ad_share":-1,"time_since_last_ad_media_tap":-1,"time_since_last_ad_gesture":-1,"time_since_last_search_result_click":-1,"time_since_last_serp_click":-1,"time_since_last_organic_share":-1,"time_since_last_organic_comment":-1,"time_since_last_organic_caption_click":-1,"time_since_last_organic_media_tap":-1,"time_since_last_organic_gesture":-1,"num_search_clicks_current_session":0}"#,
            )
            .unwrap_or(Value::Null)
        })
        .clone()
}

/// `get_timeline_feed(["cold_start_fetch"])` — post-login flow.
async fn get_timeline_feed(client: &Client) -> Result<Value> {
    let state = client.state().await;
    let request_time_ms = (now() * 1000).to_string();
    let mut data = Map::new();
    data.insert("app_start_time".to_string(), json!(request_time_ms));
    data.insert("has_camera_permission".to_string(), json!("1"));
    data.insert("feed_view_info".to_string(), json!("[]"));
    data.insert(
        "client_recorded_request_time_ms".to_string(),
        json!(request_time_ms),
    );
    data.insert("client_seen_store_media_list".to_string(), json!(""));
    data.insert("client_view_state_media_list".to_string(), json!("[]"));
    data.insert(
        "device_timezone_name".to_string(),
        json!(state.timezone_name.clone()),
    );
    data.insert("feed_reshare_info".to_string(), json!(""));
    data.insert("phone_id".to_string(), json!(state.phone_id.clone()));
    data.insert("reason".to_string(), json!("cold_start_fetch"));
    data.insert("battery_level".to_string(), json!(100));
    data.insert(
        "timezone_offset".to_string(),
        json!(state.timezone_offset.to_string()),
    );
    data.insert("device_id".to_string(), json!(state.uuid.clone()));
    data.insert("include_attribution_ui_data".to_string(), json!("true"));
    data.insert(
        "push_disabled".to_string(),
        json!(if state.push_disabled { "true" } else { "false" }),
    );
    data.insert("request_id".to_string(), json!(state.request_id.clone()));
    data.insert("request_build_time".to_string(), json!(request_time_ms));
    data.insert("_uuid".to_string(), json!(state.uuid.clone()));
    data.insert("is_charging".to_string(), json!(0));
    data.insert("is_dark_mode".to_string(), json!(1));
    data.insert("will_sound_on".to_string(), json!(0));
    data.insert(
        "session_id".to_string(),
        json!(state.client_session_id.clone()),
    );
    data.insert("session_level_signals".to_string(), session_level_signals());
    data.insert(
        "bloks_versioning_id".to_string(),
        json!(state.bloks_versioning_id()),
    );
    data.insert("is_pull_to_refresh".to_string(), json!("0"));
    drop(state);
    client
        .private_request(
            "feed/timeline/",
            Some(Body::Raw(dumps(&Value::Object(data)))),
            Req::default(),
        )
        .await
}

/// Post-login feed emulation (`login_flow`).
async fn login_flow(client: &Client) -> Result<bool> {
    let _ = get_reels_tray_feed(client).await;
    let _ = get_timeline_feed(client).await;
    Ok(true)
}

/// `two_factor_login` via `accounts/two_factor_login/`.
async fn two_factor_login(
    client: &Client,
    username: &str,
    verification_code: &str,
    two_factor_identifier: &str,
) -> Result<Value> {
    let state = client.state().await;
    let mut data = Map::new();
    data.insert("verification_code".to_string(), json!(verification_code));
    data.insert("phone_id".to_string(), json!(state.phone_id.clone()));
    data.insert("_csrftoken".to_string(), json!(state.token()));
    data.insert(
        "two_factor_identifier".to_string(),
        json!(two_factor_identifier),
    );
    data.insert("username".to_string(), json!(username));
    data.insert("trust_this_device".to_string(), json!("0"));
    data.insert("guid".to_string(), json!(state.uuid.clone()));
    data.insert(
        "device_id".to_string(),
        json!(state.android_device_id.clone()),
    );
    data.insert("waterfall_id".to_string(), json!(generate_uuid()));
    data.insert("verification_method".to_string(), json!("3"));
    drop(state);
    client
        .private_request(
            "accounts/two_factor_login/",
            Some(Body::Form(Value::Object(data))),
            Req::signed().login(true),
        )
        .await
}

impl Client {
    pub(crate) async fn inner_uuid(&self) -> String {
        self.state().await.uuid.clone()
    }

    /// Promote the server-issued `ig-set-authorization` bearer into
    /// `authorization_data` (upstream: `parse_authorization(...)` right after
    /// login). Rupload and other media-changing endpoints authenticate with
    /// the bearer only; the self-built sessionid token is not enough there.
    async fn promote_set_authorization(&self) {
        let mut state = self.state().await;
        let Some(auth) = state.last_set_authorization.take() else {
            return;
        };
        let parsed = Client::parse_authorization(&auth);
        if !parsed.is_empty() {
            state.authorization_data = parsed;
        }
    }

    /// Main password login. `verification_code` is set on the retry after a
    /// `TwoFactorRequired` error (the GUI prompts for it).
    pub async fn login(
        &self,
        username: &str,
        password: &str,
        verification_code: Option<&str>,
    ) -> Result<bool> {
        {
            let mut state = self.state().await;
            state.username = username.trim().to_string();
            state.password = password.to_string();
        }
        // Existing session validation (relogin path).
        if self.state().await.user_id().is_some() {
            match self.account_info().await {
                Ok(_) => return Ok(true),
                Err(e) if e.is(ErrorKind::LoginRequired) => {
                    let mut state = self.state().await;
                    state.cookies.clear();
                    state.authorization_data.clear();
                }
                Err(_) => return Err(IgError::client_error("Failed to validate saved session")),
            }
        }

        // Pre-login flow; 429s are ignored like the official client.
        match sync_launcher(self).await {
            Ok(_) => {}
            Err(e)
                if e.is(ErrorKind::PleaseWaitFewMinutes)
                    || e.is(ErrorKind::ClientThrottledError) =>
            {
                log::warn!("Ignore 429: Continue login");
            }
            Err(e) => return Err(e),
        }

        let state = self.state().await;
        let enc_password = password_encrypt(self, password).await?;
        let mut data = Map::new();
        data.insert(
            "jazoest".to_string(),
            json!(generate_jazoest(&state.phone_id)),
        );
        data.insert(
            "country_codes".to_string(),
            json!(format!(
                "[{{\"country_code\":\"{code}\",\"source\":[\"default\"]}}]",
                code = state.country_code
            )),
        );
        data.insert("phone_id".to_string(), json!(state.phone_id.clone()));
        data.insert("enc_password".to_string(), json!(enc_password));
        data.insert("username".to_string(), json!(username.trim()));
        data.insert("adid".to_string(), json!(state.advertising_id.clone()));
        data.insert("guid".to_string(), json!(state.uuid.clone()));
        data.insert(
            "device_id".to_string(),
            json!(state.android_device_id.clone()),
        );
        data.insert("google_tokens".to_string(), json!("[]"));
        data.insert("login_attempt_count".to_string(), json!("0"));
        drop(state);

        let result = self
            .private_request(
                "accounts/login/",
                Some(Body::Form(Value::Object(data))),
                Req::signed().login(true),
            )
            .await;

        let logged = match result {
            Ok(_) => true,
            Err(e) if e.is(ErrorKind::TwoFactorRequired) => {
                let message = e.message.clone();
                let code = verification_code
                    .map(|c| c.to_string())
                    .ok_or_else(|| IgError::new(ErrorKind::TwoFactorRequired, message.clone()))?;
                let identifier = e
                    .json
                    .pointer("/two_factor_info/two_factor_identifier")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                two_factor_login(self, username.trim(), &code, &identifier)
                    .await
                    .map_err(|e2| {
                        if e2.is(ErrorKind::UnknownError)
                            && e2.message.trim().to_lowercase() == "invalid parameters"
                        {
                            IgError::new(ErrorKind::TwoFactorRequired, message)
                        } else {
                            e2
                        }
                    })?;
                true
            }
            Err(e) => return Err(e),
        };

        if logged {
            let _ = login_flow(self).await;
            self.promote_set_authorization().await;
            let mut state = self.state().await;
            state.last_login = Some(now() as f64);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// `login_by_sessionid` — cookie-only login.
    pub async fn login_by_sessionid(&self, sessionid: &str) -> Result<bool> {
        if sessionid.len() <= 30 {
            return Err(IgError::client_error("Invalid sessionid"));
        }
        let user_id = sessionid
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or("")
            .to_string();
        if user_id.is_empty() {
            return Err(IgError::client_error("Invalid sessionid"));
        }
        {
            let mut state = self.state().await;
            state.cookies.clear();
            state
                .cookies
                .insert("sessionid".to_string(), sessionid.to_string());
            state.authorization_data = json!({
                "ds_user_id": user_id,
                "sessionid": sessionid,
                "should_use_header_over_cookies": true,
            })
            .as_object()
            .cloned()
            .unwrap_or_default();
        }
        let user = self.user_info_v1(&user_id).await?;
        {
            let mut state = self.state().await;
            state.username = user.username.clone().unwrap_or_default();
            state
                .cookies
                .insert("ds_user_id".to_string(), user.pk.clone());
            state
                .authorization_data
                .insert("ds_user_id".to_string(), json!(user.pk.clone()));
        }
        // A browser sessionid is enough for read flows; if Instagram answers
        // with a server-issued bearer, use it (media uploads need it).
        self.promote_set_authorization().await;
        Ok(true)
    }

    /// `logout`
    pub async fn logout(&self) -> Result<bool> {
        let mut data = Map::new();
        data.insert("one_tap_app_login".to_string(), json!(true));
        let result = self
            .private_request(
                "accounts/logout/",
                Some(Body::Form(Value::Object(data))),
                Req::signed(),
            )
            .await?;
        let ok = result.get("status").and_then(|v| v.as_str()).unwrap_or("") == "ok";
        if ok {
            let mut state = self.state().await;
            state.authorization_data.clear();
            state.cookies.clear();
            state.last_login = None;
        }
        Ok(ok)
    }

    // ------------------------------------------------------------- challenge

    /// `challenge_resolve` — start resolving a `ChallengeRequired` response.
    pub(crate) async fn challenge_resolve(&self, last_json: &Value) -> Result<bool> {
        let challenge = last_json
            .get("challenge")
            .and_then(|c| c.as_object())
            .ok_or_else(|| IgError::new(ErrorKind::ChallengeError, "Challenge payload missing"))?;
        let api_path = challenge
            .get("api_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let challenge_url = normalize_challenge_api_path(&api_path);
        if challenge_url.starts_with("/auth_platform/") {
            return Err(IgError::with_json(
                ErrorKind::ChallengeRequired,
                "Manual verification required via Instagram auth platform flow. \
                 This challenge is not yet supported automatically.",
                last_json.clone(),
            ));
        }
        if challenge
            .get("native_flow")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && challenge_url.starts_with("/challenge/")
        {
            return Err(IgError::with_json(
                ErrorKind::ChallengeRequired,
                "Manual verification required via Instagram native challenge flow. \
                 This checkpoint is not handled by challenge_code_handler; complete it \
                 in the official Instagram app or web flow on a trusted device.",
                last_json.clone(),
            ));
        }

        let state = self.state().await;
        let uuid = state.uuid.clone();
        let android_device_id = state.android_device_id.clone();
        let params = parse_challenge_params(&challenge_url, last_json, &uuid, &android_device_id);
        let params_owned = params.unwrap_or_default();
        drop(state);

        let result = self
            .private_request_raw(
                challenge_url.trim_start_matches('/'),
                None,
                Some(&params_owned),
                false,
            )
            .await;
        match result {
            Err(e) if e.is(ErrorKind::ChallengeRequired) => self.challenge_resolve_simple().await,
            Err(e) => Err(e),
            Ok(_) => self.challenge_resolve_simple().await,
        }
    }

    /// Old-style private-API challenge resolver (verify email/sms + code).
    pub(crate) async fn challenge_resolve_simple(&self) -> Result<bool> {
        let last_json = self.state().await.last_json.clone();
        let step_name = last_json
            .get("step_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let step_data = last_json.get("step_data").cloned().unwrap_or(Value::Null);
        let challenge_url = last_json
            .get("challenge")
            .and_then(|c| c.get("api_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let state = self.state().await;
        let username = state.username.clone();
        drop(state);

        match step_name.as_str() {
            "delta_login_review" | "delta_acknowledge_approved" | "scraping_warning" => {
                let mut data = Map::new();
                data.insert("choice".to_string(), json!("0"));
                self.private_request_raw(
                    &challenge_url,
                    Some(Body::Form(Value::Object(data))),
                    None,
                    true,
                )
                .await?;
                Ok(true)
            }
            "add_birthday" => {
                // rand::random does not retain ThreadRng across the await below,
                // keeping the future Send.
                let year = 1970 + (rand::random::<u32>() % 35);
                let month = 1 + (rand::random::<u32>() % 12);
                let day = 1 + (rand::random::<u32>() % 28);
                let mut data = Map::new();
                data.insert("birthday_year".to_string(), json!(year.to_string()));
                data.insert("birthday_month".to_string(), json!(month.to_string()));
                data.insert("birthday_day".to_string(), json!(day.to_string()));
                self.private_request_raw(
                    &challenge_url,
                    Some(Body::Form(Value::Object(data))),
                    None,
                    true,
                )
                .await?;
                Ok(true)
            }
            "verify_email"
            | "verify_email_code"
            | "verify_phone"
            | "verify_phone_code"
            | "verify_sms"
            | "verify_sms_code"
            | "select_verify_method" => {
                let choice = if step_name == "select_verify_method" {
                    let steps = step_data
                        .as_object()
                        .map(|o| o.keys().cloned().collect::<Vec<_>>())
                        .unwrap_or_default();
                    let picked = if steps.iter().any(|k| k == "email") {
                        "EMAIL"
                    } else if steps.iter().any(|k| k == "phone_number") {
                        "SMS"
                    } else {
                        return Err(IgError::new(
                            ErrorKind::ChallengeError,
                            "ChallengeResolve: Choice email or phone_number (sms) not available to this account",
                        ));
                    };
                    let mut data = Map::new();
                    data.insert(
                        "choice".to_string(),
                        json!(if picked == "EMAIL" { "1" } else { "0" }),
                    );
                    self.private_request_raw(
                        challenge_url.trim_start_matches('/'),
                        Some(Body::Form(Value::Object(data))),
                        None,
                        true,
                    )
                    .await?;
                    picked
                } else if step_name.contains("phone") || step_name.contains("sms") {
                    "SMS"
                } else {
                    "EMAIL"
                };
                let code = self.challenge_code_or_raised(&username, choice).await?;
                let mut data = Map::new();
                data.insert("security_code".to_string(), json!(code));
                self.private_request_raw(
                    challenge_url.trim_start_matches('/'),
                    Some(Body::Form(Value::Object(data))),
                    None,
                    true,
                )
                .await?;
                let last_json = self.state().await.last_json.clone();
                let action = last_json
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status = last_json
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if action != "close" || status != "ok" {
                    return Err(IgError::new(
                        ErrorKind::ChallengeError,
                        format!("Unexpected challenge response: action={action} status={status}"),
                    ));
                }
                Ok(true)
            }
            "change_password" => {
                let pwd = self.call_password_handler(&username).await;
                if pwd.is_none() {
                    return Err(IgError::new(
                        ErrorKind::ChallengeRequired,
                        "Password change required. Provide a new password via change_password_handler \
                         or complete the flow manually.",
                    ));
                }
                Err(IgError::new(
                    ErrorKind::ChallengeError,
                    "Bloks password-change challenge is not supported by this client; \
                     complete it in the official app",
                ))
            }
            "" => {
                let last_json = self.state().await.last_json.clone();
                let action = last_json
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status = last_json
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if action == "close" && status == "ok" {
                    Ok(true)
                } else {
                    Err(IgError::new(
                        ErrorKind::ChallengeError,
                        format!("Unexpected challenge response: action={action} status={status}"),
                    ))
                }
            }
            other => Err(IgError::new(
                ErrorKind::ChallengeError,
                format!("Unsupported challenge step: {other} (Please manual login)"),
            )),
        }
    }

    async fn challenge_code_or_raised(&self, username: &str, choice: &str) -> Result<String> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            if let Some(code) = self.call_code_handler(username, choice).await {
                return Ok(code);
            }
            if attempts > CHALLENGE_ATTEMPTS {
                return Err(IgError::new(
                    ErrorKind::ChallengeRequired,
                    "Challenge code was not provided before the retry window expired.",
                ));
            }
            tokio::time::sleep(Duration::from_secs(WAIT_SECONDS)).await;
        }
    }

    /// Raw request used by challenge steps (no challenge recursion).
    pub(crate) async fn private_request_raw(
        &self,
        endpoint: &str,
        body: Option<Body>,
        params: Option<&Map<String, Value>>,
        with_signature: bool,
    ) -> Result<Value> {
        let mut state = self.state().await;
        let req = Req {
            params,
            with_signature,
            ..Req::default()
        };
        self.send_with_retry(&mut state, endpoint, &body, req).await
    }

    // ------------------------------------------------------- session files

    /// Load a dumped session JSON file into the client state.
    pub async fn load_settings(&self, path: &Path) -> Result<Value> {
        let text = tokio::fs::read_to_string(path).await?;
        let settings: Value = serde_json::from_str(&text)?;
        self.set_settings(&settings).await?;
        Ok(settings)
    }

    /// Serialize the current session settings to a JSON file.
    pub async fn dump_settings(&self, path: &Path) -> Result<()> {
        let settings = self.get_settings().await;
        let text = serde_json::to_string_pretty(&settings)?;
        tokio::fs::write(path, text).await?;
        Ok(())
    }

    // -------------------------------------------------------------- account

    /// Fetch the authenticated account profile (`accounts/current_user/`).
    pub async fn account_info(&self) -> Result<Account> {
        let result = self
            .private_request("accounts/current_user/?edit=true", None, Req::signed())
            .await?;
        let user = result
            .get("user")
            .ok_or_else(|| IgError::client_error("account_info: missing user"))?;
        Ok(extract_account(user))
    }

    /// Fetch a user profile via the private API (`users/{id}/info/`).
    pub async fn user_info_v1(&self, user_id: &str) -> Result<crate::types::User> {
        let mut params = Map::new();
        params.insert("is_prefetch".to_string(), json!("false"));
        params.insert("entry_point".to_string(), json!("self_profile"));
        params.insert("from_module".to_string(), json!("self_profile"));
        params.insert("is_app_start".to_string(), json!(false));
        let result = self
            .private_request(
                &format!("users/{user_id}/info/"),
                None,
                Req::signed().params(Some(&params)),
            )
            .await
            .map_err(|e| {
                if e.is(ErrorKind::ClientNotFoundError) {
                    IgError::with_json(ErrorKind::UserNotFound, e.message, e.json)
                } else {
                    e
                }
            })?;
        let user = result
            .get("user")
            .ok_or_else(|| IgError::client_error("user_info_v1: missing user"))?;
        Ok(extract_user_v1(user))
    }

    /// Search users via the private API (`search_users_v1`).
    pub async fn search_users_v1(&self, query: &str, count: i64) -> Result<Vec<UserShort>> {
        let timezone_offset = self.state().await.timezone_offset;
        let mut params = Map::new();
        params.insert("q".to_string(), json!(query));
        params.insert("timezone_offset".to_string(), json!(timezone_offset));
        params.insert("count".to_string(), json!(count));
        let result = self
            .private_request("users/search/", None, Req::signed().params(Some(&params)))
            .await?;
        let users = result
            .get("users")
            .and_then(|u| u.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(users.iter().map(extract_user_short).collect())
    }
}

fn normalize_challenge_api_path(api_path: &str) -> String {
    if let Some(rest) = api_path.strip_prefix("/api/v1/") {
        return format!("/{rest}");
    }
    if let Some(rest) = api_path.strip_prefix("/api/") {
        return format!("/{rest}");
    }
    api_path.to_string()
}

/// Build the GET params for the initial challenge fetch.
fn parse_challenge_params(
    challenge_url: &str,
    last_json: &Value,
    uuid: &str,
    android_device_id: &str,
) -> Option<Map<String, Value>> {
    let parts: Vec<&str> = challenge_url.split('/').collect();
    // ["", "challenge", user_id, nonce_code, ...]
    if parts.len() < 4 {
        return None;
    }
    let user_id = parts.get(2)?.to_string();
    let nonce_code = parts.get(3)?.to_string();
    let challenge_context = last_json
        .get("challenge")
        .and_then(|c| c.get("challenge_context"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            dumps(&json!({
                "step_name": "",
                "nonce_code": nonce_code,
                "user_id": user_id.parse::<i64>().unwrap_or(0),
                "is_stateless": false,
            }))
        });
    let mut params = Map::new();
    params.insert("guid".to_string(), json!(uuid));
    params.insert("device_id".to_string(), json!(android_device_id));
    params.insert("challenge_context".to_string(), json!(challenge_context));
    Some(params)
}
