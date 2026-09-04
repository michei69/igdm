//! Core `Client`: session state, header construction, private API requests.
//!
//! Ported from instagrapi `mixins/private.py`, `mixins/auth.py` (state side)
//! and `mixins/public.py` (public GET used for password encryption keys).
//!
//! The whole client is `Send + Sync` and serializes every request through an
//! internal async mutex — the same "one lock" model instagrapi's callers use.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, COOKIE, SET_COOKIE};
use serde_json::{json, Map, Value};
use tokio::sync::Mutex;

use crate::config;
use crate::error::{ErrorKind, IgError, Result};
use crate::utils::{
    dumps, form_encode, gen_token, generate_android_device_id, generate_uuid, generate_uuid_prefix,
    get_str_m, now, quote_plus,
};

/// Persisted session state — the Rust twin of instagrapi's `settings` dict.
#[derive(Clone, Debug, Default)]
pub struct SessionState {
    pub phone_id: String,
    pub uuid: String,
    pub client_session_id: String,
    pub advertising_id: String,
    pub android_device_id: String,
    pub request_id: String,
    pub tray_session_id: String,
    pub mid: String,
    pub ig_u_rur: String,
    pub ig_www_claim: String,
    pub authorization_data: Map<String, Value>,
    pub cookies: HashMap<String, String>,
    pub last_login: Option<f64>,
    pub device_settings: Map<String, Value>,
    pub user_agent: String,
    pub country: String,
    pub country_code: i64,
    pub locale: String,
    pub timezone_offset: i64,
    pub timezone_name: String,
    pub push_disabled: bool,
    pub username: String,
    pub password: String,
    pub request_timeout: f64,
    pub public_request_retries_count: i64,
    pub public_request_retries_timeout: f64,
    pub session_retry_total: i64,
    pub session_retry_backoff_factor: f64,
    pub session_retry_statuses: Vec<i64>,
    pub tls_verify: bool,
    /// Last response JSON (Sentry-style context; also read by challenge flows).
    pub last_json: Value,
    /// `ig-set-authorization` header from the most recent response (consumed
    /// by the login flows; not persisted).
    pub last_set_authorization: Option<String>,
}

impl SessionState {
    fn fresh() -> Self {
        let mut device_settings = Map::new();
        for (k, v) in config::DEVICE_SETTINGS {
            device_settings.insert(k.to_string(), json!(v));
        }
        device_settings.insert(
            "app_version".to_string(),
            json!(config::DEFAULT_APP_VERSION),
        );
        device_settings.insert("version_code".to_string(), json!("961145276"));
        device_settings.insert(
            "bloks_versioning_id".to_string(),
            json!("7189b949425f9bf80ea8bd880cf5a3080b292d9b1c4b38a18d112f7c4b71e7a8"),
        );
        let mut state = SessionState {
            device_settings,
            user_agent: String::new(),
            country: "US".to_string(),
            country_code: 1,
            locale: "en_US".to_string(),
            timezone_offset: -14400,
            timezone_name: String::new(),
            push_disabled: true,
            request_timeout: 0.0,
            public_request_retries_count: 3,
            public_request_retries_timeout: 2.0,
            session_retry_total: 3,
            session_retry_backoff_factor: 2.0,
            session_retry_statuses: vec![429, 500, 502, 503, 504],
            tls_verify: true,
            last_json: Value::Null,
            ..SessionState::default()
        };
        state.set_uuids(&Map::new());
        state.set_user_agent(None);
        state
    }

    fn set_uuids(&mut self, uuids: &Map<String, Value>) {
        self.phone_id = get_str_m(uuids, "phone_id").unwrap_or_else(generate_uuid);
        self.uuid = get_str_m(uuids, "uuid").unwrap_or_else(generate_uuid);
        self.client_session_id = get_str_m(uuids, "client_session_id").unwrap_or_else(generate_uuid);
        self.advertising_id = get_str_m(uuids, "advertising_id").unwrap_or_else(generate_uuid);
        self.android_device_id =
            get_str_m(uuids, "android_device_id").unwrap_or_else(generate_android_device_id);
        self.request_id = get_str_m(uuids, "request_id").unwrap_or_else(generate_uuid);
        self.tray_session_id = get_str_m(uuids, "tray_session_id").unwrap_or_else(generate_uuid);
    }

    fn set_user_agent(&mut self, user_agent: Option<String>) {
        self.user_agent = user_agent.unwrap_or_else(|| {
            let d = &self.device_settings;
            config::USER_AGENT_BASE
                .replace(
                    "{app_version}",
                    get_str_m(d, "app_version").unwrap_or_default().as_str(),
                )
                .replace(
                    "{android_version}",
                    get_str_m(d, "android_version").unwrap_or_default().as_str(),
                )
                .replace(
                    "{android_release}",
                    get_str_m(d, "android_release").unwrap_or_default().as_str(),
                )
                .replace("{dpi}", get_str_m(d, "dpi").unwrap_or_default().as_str())
                .replace(
                    "{resolution}",
                    get_str_m(d, "resolution").unwrap_or_default().as_str(),
                )
                .replace(
                    "{manufacturer}",
                    get_str_m(d, "manufacturer").unwrap_or_default().as_str(),
                )
                .replace("{model}", get_str_m(d, "model").unwrap_or_default().as_str())
                .replace("{device}", get_str_m(d, "device").unwrap_or_default().as_str())
                .replace("{cpu}", get_str_m(d, "cpu").unwrap_or_default().as_str())
                .replace("{locale}", self.locale.as_str())
                .replace(
                    "{version_code}",
                    get_str_m(d, "version_code").unwrap_or_default().as_str(),
                )
        });
    }

    pub fn user_id(&self) -> Option<i64> {
        let id = self.cookies.get("ds_user_id").cloned().or_else(|| {
            self.authorization_data
                .get("ds_user_id")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        });
        id.and_then(|s| s.parse().ok())
    }

    pub fn sessionid(&self) -> Option<String> {
        self.cookies
            .get("sessionid")
            .cloned()
            .or_else(|| {
                self.authorization_data
                    .get("sessionid")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
            .filter(|s| !s.is_empty())
    }

    pub fn token(&self) -> String {
        self.cookies
            .get("csrftoken")
            .cloned()
            .unwrap_or_else(|| gen_token(64))
    }

    pub fn bloks_versioning_id(&self) -> String {
        get_str_m(&self.device_settings, "bloks_versioning_id").unwrap_or_default()
    }

    pub fn app_version(&self) -> String {
        get_str_m(&self.device_settings, "app_version").unwrap_or_default()
    }

    /// `get_settings()` — the persisted JSON shape (instagrapi compatible).
    pub fn to_settings_json(&self) -> Value {
        json!({
            "uuids": {
                "phone_id": self.phone_id,
                "uuid": self.uuid,
                "client_session_id": self.client_session_id,
                "advertising_id": self.advertising_id,
                "android_device_id": self.android_device_id,
                "request_id": self.request_id,
                "tray_session_id": self.tray_session_id,
            },
            "mid": self.mid,
            "ig_u_rur": self.ig_u_rur,
            "ig_www_claim": self.ig_www_claim,
            "authorization_data": Value::Object(self.authorization_data.clone()),
            "cookies": Value::Object(self.cookies.iter().map(|(k, v)| (k.clone(), json!(v))).collect()),
            "last_login": self.last_login,
            "device_settings": Value::Object(self.device_settings.clone()),
            "user_agent": self.user_agent,
            "country": self.country,
            "country_code": self.country_code,
            "locale": self.locale,
            "timezone_offset": self.timezone_offset,
            "timezone_name": self.timezone_name,
            "push_disabled": self.push_disabled,
            "request_timeout": self.request_timeout,
            "public_request_retries_count": self.public_request_retries_count,
            "public_request_retries_timeout": self.public_request_retries_timeout,
            "session_retry_total": self.session_retry_total,
            "session_retry_backoff_factor": self.session_retry_backoff_factor,
            "session_retry_statuses": Value::Array(
                self.session_retry_statuses.iter().map(|v| json!(v)).collect()
            ),
            "tls_verify": self.tls_verify,
        })
    }
}

/// Interactive handlers the GUI hooks into (challenge codes, password change).
pub type CodeHandler = Arc<
    dyn Fn(
            String,
            String,
        ) -> std::pin::Pin<Box<dyn futures_util::Future<Output = Option<String>> + Send>>
        + Send
        + Sync,
>;
pub type PasswordHandler = Arc<
    dyn Fn(String) -> std::pin::Pin<Box<dyn futures_util::Future<Output = Option<String>> + Send>>
        + Send
        + Sync,
>;

fn noop_code(
    _username: String,
    _choice: String,
) -> std::pin::Pin<Box<dyn futures_util::Future<Output = Option<String>> + Send>> {
    Box::pin(async { None })
}

fn noop_password(
    _username: String,
) -> std::pin::Pin<Box<dyn futures_util::Future<Output = Option<String>> + Send>> {
    Box::pin(async { None })
}

pub(crate) struct ClientInner {
    pub(crate) http: reqwest::Client,
    /// HTTP/1.1-only client for FB rupload (mirrors instagrapi's dedicated
    /// `requests.Session`; the rupload edge is HTTP/1.1-oriented).
    pub(crate) http_h1: reqwest::Client,
    state: Mutex<SessionState>,
    code_handler: std::sync::RwLock<CodeHandler>,
    change_password_handler: std::sync::RwLock<PasswordHandler>,
}

/// Thread-safe Instagram private API client (safe Rust, no `unsafe`).
#[derive(Clone)]
pub struct Client {
    pub(crate) inner: Arc<ClientInner>,
}

/// Body of a private request: a form dict (signed or plain) or a raw string.
#[derive(Debug, Clone)]
pub enum Body {
    Form(Value),
    Raw(String),
}

/// Options for one private API request (replaces an 8-arg call signature).
#[derive(Clone, Copy, Default)]
pub struct Req<'a> {
    pub params: Option<&'a Map<String, Value>>,
    pub login: bool,
    pub with_signature: bool,
    pub headers: Option<&'a Map<String, Value>>,
    pub extra_sig: Option<&'a [String]>,
    pub domain: Option<&'a str>,
}

impl<'a> Req<'a> {
    /// Signed request (SIGNATURE. prefixed body); the common case.
    pub fn signed() -> Self {
        Self {
            with_signature: true,
            ..Self::default()
        }
    }

    pub fn login(mut self, login: bool) -> Self {
        self.login = login;
        self
    }

    pub fn params(mut self, params: Option<&'a Map<String, Value>>) -> Self {
        self.params = params;
        self
    }
}

impl Client {
    pub fn new() -> Self {
        Self::with_state(SessionState::fresh())
    }

    pub fn with_state(state: SessionState) -> Self {
        // reqwest enables rustls' `ring` provider feature while tokio-rustls
        // enables `aws_lc_rs`; rustls refuses to pick between two providers
        // automatically, so choose one explicitly at startup.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .gzip(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .expect("reqwest client");
        let http_h1 = reqwest::Client::builder()
            .gzip(true)
            .http1_only()
            .build()
            .expect("reqwest upload client");
        Self {
            inner: Arc::new(ClientInner {
                http,
                http_h1,
                state: Mutex::new(state),
                code_handler: std::sync::RwLock::new(Arc::new(noop_code)),
                change_password_handler: std::sync::RwLock::new(Arc::new(noop_password)),
            }),
        }
    }

    pub fn set_code_handler(&self, handler: CodeHandler) {
        *self.inner.code_handler.write().expect("handler rwlock") = handler;
    }

    pub fn set_change_password_handler(&self, handler: PasswordHandler) {
        *self
            .inner
            .change_password_handler
            .write()
            .expect("handler rwlock") = handler;
    }

    pub(crate) async fn call_code_handler(&self, username: &str, choice: &str) -> Option<String> {
        let handler = self
            .inner
            .code_handler
            .read()
            .expect("handler rwlock")
            .clone();
        handler(username.to_string(), choice.to_string()).await
    }

    pub(crate) async fn call_password_handler(&self, username: &str) -> Option<String> {
        let handler = self
            .inner
            .change_password_handler
            .read()
            .expect("handler rwlock")
            .clone();
        handler(username.to_string()).await
    }

    pub async fn state(&self) -> tokio::sync::MutexGuard<'_, SessionState> {
        self.inner.state.lock().await
    }

    // ------------------------------------------------------------------ state

    pub async fn set_settings(&self, settings: &Value) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let obj = settings.as_object().cloned().unwrap_or_default();
        let uuids = obj
            .get("uuids")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        state.set_uuids(&uuids);
        if let Some(cookies) = obj.get("cookies").and_then(|c| c.as_object()) {
            state.cookies = cookies
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect();
        }
        state.authorization_data = obj
            .get("authorization_data")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        state.last_login = obj.get("last_login").and_then(|v| v.as_f64());
        if let Some(dev) = obj.get("device_settings").and_then(|v| v.as_object()) {
            state.device_settings = dev.clone();
        }
        if let Some(ua) = obj.get("user_agent").and_then(|v| v.as_str()) {
            state.user_agent = ua.to_string();
        } else {
            state.set_user_agent(None);
        }
        if let Some(v) = obj.get("country").and_then(|v| v.as_str()) {
            state.country = v.to_string();
        }
        if let Some(v) = obj.get("country_code") {
            state.country_code = v.as_i64().unwrap_or(1);
        }
        if let Some(v) = obj.get("locale").and_then(|v| v.as_str()) {
            state.locale = v.to_string();
        }
        if let Some(v) = obj.get("timezone_offset") {
            state.timezone_offset = v.as_i64().unwrap_or(-14400);
        }
        if let Some(v) = obj.get("timezone_name").and_then(|v| v.as_str()) {
            state.timezone_name = v.to_string();
        }
        if let Some(v) = obj.get("push_disabled") {
            state.push_disabled = v.as_bool().unwrap_or(true);
        }
        state.mid = get_str_m(&obj, "mid").unwrap_or_default();
        state.ig_u_rur = get_str_m(&obj, "ig_u_rur").unwrap_or_default();
        state.ig_www_claim = get_str_m(&obj, "ig_www_claim").unwrap_or_default();
        if let Some(v) = obj.get("request_timeout") {
            state.request_timeout = v.as_f64().unwrap_or(0.0);
        }
        if let Some(v) = obj.get("public_request_retries_count") {
            state.public_request_retries_count = v.as_i64().unwrap_or(3);
        }
        if let Some(v) = obj.get("public_request_retries_timeout") {
            state.public_request_retries_timeout = v.as_f64().unwrap_or(2.0);
        }
        if let Some(v) = obj.get("session_retry_total") {
            state.session_retry_total = v.as_i64().unwrap_or(3);
        }
        if let Some(v) = obj.get("session_retry_backoff_factor") {
            state.session_retry_backoff_factor = v.as_f64().unwrap_or(2.0);
        }
        if let Some(v) = obj.get("session_retry_statuses").and_then(|v| v.as_array()) {
            state.session_retry_statuses = v.iter().filter_map(|s| s.as_i64()).collect();
        }
        if let Some(v) = obj.get("tls_verify") {
            state.tls_verify = v.as_bool().unwrap_or(true);
        }
        Ok(())
    }

    pub async fn get_settings(&self) -> Value {
        self.inner.state.lock().await.to_settings_json()
    }

    pub async fn sessionid(&self) -> Option<String> {
        self.inner.state.lock().await.sessionid()
    }

    pub async fn user_id(&self) -> Option<i64> {
        self.inner.state.lock().await.user_id()
    }

    pub async fn username(&self) -> String {
        self.inner.state.lock().await.username.clone()
    }

    // ------------------------------------------------------------------ auth

    /// `parse_authorization`: decode the `ig-set-authorization` header value.
    pub fn parse_authorization(authorization: &str) -> Map<String, Value> {
        if authorization.is_empty() {
            return Map::new();
        }
        let Some(b64) = authorization.rsplit(':').next() else {
            return Map::new();
        };
        if b64.is_empty() {
            return Map::new();
        }
        use base64::Engine as _;
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
            return Map::new();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    /// Build the `Authorization` header value.
    pub async fn authorization(&self) -> String {
        let state = self.inner.state.lock().await;
        self.authorization_header(&state).await
    }

    async fn authorization_header(&self, state: &SessionState) -> String {
        if state.authorization_data.is_empty() {
            return String::new();
        }
        let payload = serde_json::to_string(&state.authorization_data).unwrap_or_default();
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(payload.as_bytes());
        format!("Bearer IGT:2:{b64}")
    }

    /// Private API request with error mapping, challenge resolution and
    /// retry-on-timeout semantics from `_send_private_request`.
    pub async fn private_request(
        &self,
        endpoint: &str,
        body: Option<Body>,
        req: Req<'_>,
    ) -> Result<Value> {
        // Serialize all client use (like instagrapi's `_inbox_lock`).
        // The guard must be released before `challenge_resolve`: it locks the
        // same (non-reentrant) mutex itself, and holding the guard across it
        // would deadlock the task while the guard blocks every other request
        // forever.
        // `send_with_retry` borrows the body (cloning only per attempt); the
        // original stays here so the post-challenge re-send below has it.
        let challenge_json = {
            let mut state = self.inner.state.lock().await;
            match self.send_with_retry(&mut state, endpoint, &body, req).await {
                Ok(json) => return Ok(json),
                Err(e) if e.is(ErrorKind::ChallengeRequired) => e.json,
                Err(e) => return Err(e),
            }
        };
        self.challenge_resolve(&challenge_json).await?;
        let mut state = self.inner.state.lock().await;
        self.send_private_request(&mut state, endpoint, body, req)
            .await
    }

    /// `send_private_request` plus timeout / incomplete-read retries.
    /// The body is borrowed so the caller keeps the original for a possible
    /// post-challenge re-send; each attempt clones it (retries are rare).
    pub(crate) async fn send_with_retry(
        &self,
        state: &mut SessionState,
        endpoint: &str,
        body: &Option<Body>,
        req: Req<'_>,
    ) -> Result<Value> {
        match self.send_private_request(state, endpoint, body.clone(), req).await {
            Ok(json) => Ok(json),
            Err(e) if e.is(ErrorKind::ClientRequestTimeout) => {
                log::info!("Wait 60 seconds and try one more time (ClientRequestTimeout)");
                tokio::time::sleep(Duration::from_secs(60)).await;
                self.send_private_request(state, endpoint, body.clone(), req).await
            }
            Err(e) if e.is(ErrorKind::ClientIncompleteReadError) => {
                log::info!("Wait 2 seconds and try one more time (ClientIncompleteReadError)");
                tokio::time::sleep(Duration::from_secs(2)).await;
                self.send_private_request(state, endpoint, body.clone(), req).await
            }
            Err(e) => Err(e),
        }
    }

    pub(crate) async fn send_private_request(
        &self,
        state: &mut SessionState,
        endpoint: &str,
        body: Option<Body>,
        req: Req<'_>,
    ) -> Result<Value> {
        state.last_json = Value::Null;

        let host = req.domain.unwrap_or(config::API_DOMAIN);
        let api_url = if endpoint == "/challenge/" {
            format!("https://{host}/api/v1/challenge/")
        } else if let Some(rest) = endpoint.strip_prefix('/') {
            format!("https://{host}/api/{rest}")
        } else {
            format!("https://{host}/api/v1/{endpoint}")
        };

        let base = build_base_headers(state)?;
        let mut request_headers = base;
        if let Some(h) = req.headers {
            for (k, v) in h {
                if let Ok(name) = HeaderName::from_bytes(k.as_bytes()) {
                    if let Ok(value) = HeaderValue::from_str(v.as_str().unwrap_or_default()) {
                        request_headers.insert(name, value);
                    }
                }
            }
        }
        let auth = self.authorization_header(state).await;
        if !auth.is_empty() && !request_headers.contains_key("authorization") {
            request_headers.insert("authorization", HeaderValue::from_str(&auth).unwrap());
        }
        if let Some(cookie) = cookie_header(&state.cookies) {
            request_headers.insert(COOKIE, HeaderValue::from_str(&cookie).unwrap());
        }

        let method = if body.is_some() {
            reqwest::Method::POST
        } else {
            reqwest::Method::GET
        };
        let mut req_builder = self.inner.http.request(method, &api_url);
        if let Some(p) = req.params {
            req_builder = req_builder.query(&encode_query(p));
        }
        if let Some(body) = body {
            let content = match body {
                Body::Form(data) => {
                    if req.with_signature {
                        let json = dumps(&data);
                        format!("signed_body=SIGNATURE.{}", quote_plus(&json))
                    } else {
                        data.as_object().map(form_encode).unwrap_or_default()
                    }
                }
                Body::Raw(raw) => raw,
            };
            let mut sig_suffix = String::new();
            if req.with_signature {
                if let Some(extra) = req.extra_sig {
                    for s in extra {
                        sig_suffix.push('&');
                        sig_suffix.push_str(s);
                    }
                }
            }
            request_headers.insert(
                "content-type",
                HeaderValue::from_static("application/x-www-form-urlencoded; charset=UTF-8"),
            );
            let body = format!("{content}{sig_suffix}");
            log::debug!("POST {api_url} body_len={}", body.len());
            req_builder = req_builder.body(body);
        } else {
            request_headers.remove("content-type");
        }
        req_builder = req_builder.headers(request_headers);

        let timeout_secs = if req.login { 60.0 } else { 30.0 };
        let response =
            match tokio::time::timeout(Duration::from_secs_f64(timeout_secs), req_builder.send())
                .await
            {
                Ok(Ok(resp)) => resp,
                Ok(Err(e)) => {
                    return Err(if e.is_timeout() {
                        IgError::new(ErrorKind::ClientRequestTimeout, "Request timed out")
                    } else if e.is_connect() {
                        IgError::new(
                            ErrorKind::ClientConnectionError,
                            format!("ClientConnectionError {e}"),
                        )
                    } else {
                        IgError::new(ErrorKind::ClientError, format!("Request failed: {e}"))
                    });
                }
                Err(_) => {
                    return Err(IgError::new(
                        ErrorKind::ClientRequestTimeout,
                        "Request timed out",
                    ));
                }
            };

        // ingest cookies + mid
        for set_cookie in response.headers().get_all(SET_COOKIE) {
            if let Ok(value) = set_cookie.to_str() {
                if let Some((name, rest)) = value.split_once('=') {
                    let value = rest.split(';').next().unwrap_or("").trim().to_string();
                    state.cookies.insert(name.trim().to_string(), value);
                }
            }
        }
        if let Some(mid) = response.headers().get("ig-set-x-mid") {
            if let Ok(mid) = mid.to_str() {
                state.mid = mid.to_string();
            }
        }
        // Server-issued bearer token; the login flows promote it into
        // `authorization_data` (upstream: `parse_authorization(ig-set-authorization)`).
        if let Some(auth) = response.headers().get("ig-set-authorization") {
            if let Ok(auth) = auth.to_str() {
                state.last_set_authorization = Some(auth.to_string());
            }
        }
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        log::debug!("private_request {endpoint} ({status})");

        let last_json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        state.last_json = last_json;

        if !status.is_success() {
            return Err(map_http_error(
                status.as_u16(),
                &text,
                &state.last_json,
                endpoint,
            ));
        }
        if let Some(obj) = state.last_json.as_object() {
            if obj.get("status").and_then(|v| v.as_str()) == Some("fail") {
                let message = obj.get("message").cloned().unwrap_or(Value::Null);
                if contains(&message, "need an email or confirmed phone number")
                    && endpoint.contains("accounts/edit_profile")
                {
                    return Err(IgError::with_json(
                        ErrorKind::AccountContactPointRequired,
                        message_text(&message),
                        state.last_json.clone(),
                    ));
                }
                if endpoint.contains("accounts/edit_profile") {
                    return Err(IgError::with_json(
                        ErrorKind::AccountEditError,
                        message_text(&message),
                        state.last_json.clone(),
                    ));
                }
                if is_direct_requests_disabled(endpoint, &message) {
                    return Err(IgError::with_json(
                        ErrorKind::DirectMessageRequestsDisabled,
                        message_text(&message),
                        state.last_json.clone(),
                    ));
                }
                return Err(IgError::with_json(
                    ErrorKind::ClientError,
                    message_text(&message),
                    state.last_json.clone(),
                ));
            }
            if obj.contains_key("error_title") {
                return Err(IgError::with_json(
                    ErrorKind::ClientError,
                    message_text(&obj.get("message").cloned().unwrap_or(Value::Null)),
                    state.last_json.clone(),
                ));
            }
        }
        // One materialization: the caller owns the response while `state`
        // keeps its copy (challenge flows and realtime read `last_json`
        // after successful requests).
        Ok(state.last_json.clone())
    }

    /// Public GET used for password encryption keys (`qe/sync`).
    pub async fn public_get(&self, url: &str) -> Result<(u16, HeaderMap, Vec<u8>)> {
        let state = self.inner.state.lock().await;
        let mut headers = HeaderMap::new();
        headers.insert("Connection", HeaderValue::from_static("Keep-Alive"));
        headers.insert("Accept", HeaderValue::from_static("*/*"));
        headers.insert("Accept-Encoding", HeaderValue::from_static("gzip,deflate"));
        headers.insert("Accept-Language", HeaderValue::from_static("en-US"));
        headers.insert(
            "User-Agent",
            HeaderValue::from_str(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_13_6) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/11.1.2 Safari/605.1.15",
            )
            .unwrap(),
        );
        if let Some(sessionid) = state.sessionid() {
            if let Ok(v) = HeaderValue::from_str(&format!("sessionid={sessionid}")) {
                headers.insert(COOKIE, v);
            }
        }
        let response = self
            .inner
            .http
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(IgError::from)?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.map_err(IgError::from)?.to_vec();
        Ok((status, headers, bytes))
    }
}

fn message_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Object(obj) => {
            if let Some(errors) = obj.get("errors") {
                if let Some(arr) = errors.as_array() {
                    return arr
                        .iter()
                        .map(|e| e.as_str().unwrap_or_default().to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            }
            obj.values().map(message_text).collect::<Vec<_>>().join(" ")
        }
        Value::Array(arr) => arr.iter().map(message_text).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

fn contains(value: &Value, needle: &str) -> bool {
    message_text(value)
        .to_lowercase()
        .contains(&needle.to_lowercase())
}

fn is_direct_requests_disabled(endpoint: &str, message: &Value) -> bool {
    if !endpoint.contains("direct_v2/") {
        return false;
    }
    let text = message_text(message).to_lowercase().replace('’', "'");
    const MARKERS: &[&str] = &[
        "can't message this account unless they follow you",
        "can't receive your message because they don't allow new message requests",
        "doesn't allow new message requests",
        "don't allow new message requests",
        "does not allow new message requests",
    ];
    MARKERS.iter().any(|m| text.contains(m))
}

fn map_http_error(status: u16, text: &str, last_json: &Value, endpoint: &str) -> IgError {
    let message = get_message(last_json);
    if message.contains("Please wait a few minutes") {
        return IgError::with_json(ErrorKind::PleaseWaitFewMinutes, message, last_json.clone());
    }
    match status {
        403 => {
            if message == "login_required" {
                return IgError::with_json(ErrorKind::LoginRequired, message, last_json.clone());
            }
            if text.len() < 512 {
                let mut json = last_json.clone();
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("message".to_string(), json!(text));
                }
                return IgError::with_json(ErrorKind::ClientForbiddenError, text.to_string(), json);
            }
            IgError::with_json(ErrorKind::ClientForbiddenError, message, last_json.clone())
        }
        400 => {
            let error_type = last_json
                .get("error_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if last_json.get("two_factor_info").is_some() || error_type == "two_factor_required" {
                let mut msg = message;
                if msg.is_empty() {
                    msg = "Two-factor authentication required".to_string();
                }
                return IgError::with_json(ErrorKind::TwoFactorRequired, msg, last_json.clone());
            }
            if message == "challenge_required" {
                let challenge = last_json.get("challenge").cloned().unwrap_or(Value::Null);
                let url = challenge.get("url").and_then(|v| v.as_str()).unwrap_or("");
                if url.contains("/suspended/") {
                    return IgError::with_json(
                        ErrorKind::AccountSuspended,
                        message,
                        last_json.clone(),
                    );
                }
                return IgError::with_json(
                    ErrorKind::ChallengeRequired,
                    message,
                    last_json.clone(),
                );
            }
            if message == "feedback_required" {
                let feedback = last_json
                    .get("feedback_message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                return IgError::with_json(
                    ErrorKind::FeedbackRequired,
                    format!("feedback_required: {feedback}"),
                    last_json.clone(),
                );
            }
            match error_type {
                "sentry_block" => {
                    return IgError::with_json(ErrorKind::SentryBlock, message, last_json.clone());
                }
                "rate_limit_error" => {
                    return IgError::with_json(
                        ErrorKind::RateLimitError,
                        message,
                        last_json.clone(),
                    );
                }
                "bad_password" => {
                    let hint = "This can also happen when Instagram rejects the proxy/IP, device fingerprint, or login context, even if the password is correct.";
                    let full = if message.is_empty() {
                        format!("Instagram rejected the login credentials. {hint}")
                    } else {
                        format!("{message} {hint}")
                    };
                    return IgError::with_json(ErrorKind::BadPassword, full, last_json.clone());
                }
                _ => {}
            }
            if endpoint.contains("accounts/edit_profile") {
                if message
                    .to_lowercase()
                    .contains("need an email or confirmed phone number")
                {
                    return IgError::with_json(
                        ErrorKind::AccountContactPointRequired,
                        message,
                        last_json.clone(),
                    );
                }
                return IgError::with_json(ErrorKind::AccountEditError, message, last_json.clone());
            }
            if is_direct_requests_disabled(endpoint, last_json) {
                return IgError::with_json(
                    ErrorKind::DirectMessageRequestsDisabled,
                    message,
                    last_json.clone(),
                );
            }
            if message.contains("VideoTooLongException") {
                return IgError::with_json(
                    ErrorKind::VideoTooLongException,
                    message,
                    last_json.clone(),
                );
            }
            if message.contains("Not authorized to view user") {
                return IgError::with_json(ErrorKind::PrivateAccount, message, last_json.clone());
            }
            if message.contains("Invalid target user") {
                return IgError::with_json(
                    ErrorKind::InvalidTargetUser,
                    message,
                    last_json.clone(),
                );
            }
            if message.contains("Invalid media_id") {
                return IgError::with_json(ErrorKind::InvalidMediaId, message, last_json.clone());
            }
            if message.contains("Media is unavailable")
                || message.contains("Media not found or unavailable")
                || message.contains("has been deleted")
            {
                return IgError::with_json(ErrorKind::MediaUnavailable, message, last_json.clone());
            }
            if message.contains("unable to fetch followers") {
                return IgError::with_json(ErrorKind::UserNotFound, message, last_json.clone());
            }
            if message.contains("The username you entered") {
                return IgError::with_json(
                    ErrorKind::ProxyAddressIsBlocked,
                    "Instagram has blocked your IP address, use a quality proxy provider (not free, not shared)",
                    last_json.clone(),
                );
            }
            if !error_type.is_empty() || !message.is_empty() {
                return IgError::with_json(ErrorKind::UnknownError, message, last_json.clone());
            }
            IgError::with_json(ErrorKind::ClientBadRequestError, message, last_json.clone())
        }
        429 => IgError::with_json(
            ErrorKind::ClientThrottledError,
            "Too many requests",
            last_json.clone(),
        ),
        401 => IgError::with_json(
            ErrorKind::ClientUnauthorizedError,
            format!("Unauthorized {endpoint}"),
            last_json.clone(),
        ),
        404 => {
            if text.trim() == "Not Found" {
                return IgError::with_json(
                    ErrorKind::ChallengeRequired,
                    "challenge_required",
                    last_json.clone(),
                );
            }
            IgError::with_json(
                ErrorKind::ClientNotFoundError,
                format!("Endpoint {endpoint} does not exist"),
                last_json.clone(),
            )
        }
        408 => IgError::with_json(
            ErrorKind::ClientRequestTimeout,
            "Request Timeout",
            last_json.clone(),
        ),
        _ => IgError::with_json(
            ErrorKind::ClientError,
            format!("HTTP {status}"),
            last_json.clone(),
        ),
    }
}

fn get_message(json: &Value) -> String {
    json.get("message").map(message_text).unwrap_or_default()
}

fn cookie_header(cookies: &HashMap<String, String>) -> Option<String> {
    if cookies.is_empty() {
        return None;
    }
    Some(
        cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn encode_query(params: &Map<String, Value>) -> Vec<(String, String)> {
    params
        .iter()
        .map(|(k, v)| {
            let text = match v {
                Value::Bool(b) => {
                    if *b {
                        "True".to_string()
                    } else {
                        "False".to_string()
                    }
                }
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), text)
        })
        .collect()
}

fn build_base_headers(state: &SessionState) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    let locale = state.locale.replace('-', "_");
    let mut accept_language = vec!["en-US"];
    let lang = if locale.is_empty() {
        None
    } else {
        Some(locale.replace('_', "-"))
    };
    if let Some(lang) = lang.as_deref() {
        if lang != "en-US" {
            accept_language.insert(0, lang);
        }
    }
    let pigeon = generate_uuid_prefix("UFS-", "-1");
    let now_f = now();
    let user_id = state.user_id();
    headers.insert("X-IG-App-Locale", HeaderValue::from_str(&locale).unwrap());
    headers.insert(
        "X-IG-Device-Locale",
        HeaderValue::from_str(&locale).unwrap(),
    );
    headers.insert(
        "X-IG-Mapped-Locale",
        HeaderValue::from_str(&locale).unwrap(),
    );
    headers.insert(
        "X-Pigeon-Session-Id",
        HeaderValue::from_str(&pigeon).unwrap(),
    );
    headers.insert(
        "X-Pigeon-Rawclienttime",
        HeaderValue::from_str(&format!("{:.3}", now_f as f64 + 0.123)).unwrap(),
    );
    headers.insert(
        "X-IG-Bandwidth-Speed-KBPS",
        HeaderValue::from_str(&format!("{:.3}", 2500.0 + (now_f % 500) as f64 / 1000.0)).unwrap(),
    );
    headers.insert(
        "X-IG-Bandwidth-TotalBytes-B",
        HeaderValue::from_str(&format!("{}", 5_000_000 + (now_f % 85_000_000))).unwrap(),
    );
    headers.insert(
        "X-IG-Bandwidth-TotalTime-MS",
        HeaderValue::from_str(&format!("{}", 2_000 + (now_f % 7_000))).unwrap(),
    );
    headers.insert(
        "X-IG-App-Startup-Country",
        HeaderValue::from_str(&state.country.to_uppercase()).unwrap(),
    );
    headers.insert(
        "X-Bloks-Version-Id",
        HeaderValue::from_str(&state.bloks_versioning_id()).unwrap(),
    );
    headers.insert("X-IG-WWW-Claim", HeaderValue::from_static("0"));
    headers.insert("X-Bloks-Is-Layout-RTL", HeaderValue::from_static("false"));
    headers.insert(
        "X-Bloks-Is-Panorama-Enabled",
        HeaderValue::from_static("true"),
    );
    headers.insert(
        "X-IG-Device-ID",
        HeaderValue::from_str(&state.uuid).unwrap(),
    );
    headers.insert(
        "X-IG-Family-Device-ID",
        HeaderValue::from_str(&state.phone_id).unwrap(),
    );
    headers.insert(
        "X-IG-Android-ID",
        HeaderValue::from_str(&state.android_device_id).unwrap(),
    );
    headers.insert(
        "X-IG-Timezone-Offset",
        HeaderValue::from_str(&state.timezone_offset.to_string()).unwrap(),
    );
    headers.insert("X-IG-Connection-Type", HeaderValue::from_static("WIFI"));
    headers.insert("X-IG-Capabilities", HeaderValue::from_static("3brTv10="));
    headers.insert("X-IG-App-ID", HeaderValue::from_static(config::APP_ID));
    headers.insert("Priority", HeaderValue::from_static("u=3"));
    headers.insert(
        "User-Agent",
        HeaderValue::from_str(&state.user_agent).unwrap(),
    );
    headers.insert(
        "Accept-Language",
        if accept_language.len() == 1 {
            HeaderValue::from_static("en-US")
        } else {
            HeaderValue::from_str(&accept_language.join(", ")).unwrap()
        },
    );
    headers.insert("X-MID", HeaderValue::from_str(&state.mid).unwrap());
    headers.insert("Accept-Encoding", HeaderValue::from_static("gzip, deflate"));
    headers.insert("Host", HeaderValue::from_static(config::API_DOMAIN));
    headers.insert(
        "X-FB-HTTP-Engine",
        HeaderValue::from_static("Tigon/MNS/TCP"),
    );
    headers.insert("X-Tigon-Is-Retry", HeaderValue::from_static("False"));
    headers.insert("X-Zero-Balance", HeaderValue::from_static("INIT"));
    headers.insert("X-Zero-Eh", HeaderValue::from_static(""));
    headers.insert("X-Zero-State", HeaderValue::from_static("unknown"));
    headers.insert(
        "Zero-HTTP-Network-Interface",
        HeaderValue::from_static("wifi"),
    );
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert("X-FB-Client-IP", HeaderValue::from_static("True"));
    headers.insert("X-FB-Server-Cluster", HeaderValue::from_static("True"));
    headers.insert(
        "IG-INTENDED-USER-ID",
        HeaderValue::from_str(&user_id.unwrap_or(0).to_string()).unwrap(),
    );
    headers.insert(
        "X-IG-Nav-Chain",
        HeaderValue::from_static(
            "9MV:self_profile:2,ProfileMediaTabFragment:self_profile:3,9Xf:self_following:4",
        ),
    );
    headers.insert(
        "X-IG-SALT-IDS",
        HeaderValue::from_str(&format!("{}", 1061162222 + (now_f % 100_000))).unwrap(),
    );
    if let Some(user_id) = user_id {
        let next_year = now_f + 31_536_000;
        headers.insert(
            "IG-U-DS-USER-ID",
            HeaderValue::from_str(&user_id.to_string()).unwrap(),
        );
        headers.insert(
            "IG-U-IG-DIRECT-REGION-HINT",
            HeaderValue::from_str(&format!(
                "LLA,{user_id},{next_year}:01f7bae7d8b131877d8e0ae1493252280d72f6d0d554447cb1dc9049b6b2c507c08605b7"
            ))
            .unwrap(),
        );
        headers.insert(
            "IG-U-SHBID",
            HeaderValue::from_str(&format!(
                "12695,{user_id},{next_year}:01f778d9c9f7546cf3722578fbf9b85143cd6e5132723e5c93f40f55ca0459c8ef8a0d9f"
            ))
            .unwrap(),
        );
        headers.insert(
            "IG-U-SHBTS",
            HeaderValue::from_str(&format!(
                "{},{user_id},{next_year}:01f7ace11925d0388080078d0282b75b8059844855da27e23c90a362270fddfb3fae7e28",
                now_f
            ))
            .unwrap(),
        );
        headers.insert(
            "IG-U-RUR",
            HeaderValue::from_str(&format!(
                "RVA,{user_id},{next_year}:01f7f627f9ae4ce2874b2e04463efdb184340968b1b006fa88cb4cc69a942a04201e544c"
            ))
            .unwrap(),
        );
    }
    if !state.ig_u_rur.is_empty() {
        headers.insert("IG-U-RUR", HeaderValue::from_str(&state.ig_u_rur).unwrap());
    }
    if !state.ig_www_claim.is_empty() {
        headers.insert(
            "X-IG-WWW-Claim",
            HeaderValue::from_str(&state.ig_www_claim).unwrap(),
        );
    }
    Ok(headers)
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
