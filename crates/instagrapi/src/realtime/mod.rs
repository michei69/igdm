//! MQTToT realtime client ported from `instagrapi.realtime.client`:
//! TLS transport, CONNECT, read loop with dispatch of message/typing/seen
//! events, and direct command publishing (mark_seen, indicate_activity).

pub mod mqttot;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::{json, Map, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_rustls::TlsConnector;

use crate::client::Client;
use crate::error::{IgError, Result};
use mqttot::{
    compress_payload, decode_packet, try_decompress_payload, write_connect_packet,
    write_disconnect_packet, write_pingreq_packet, write_puback_packet, write_publish_packet,
    MQTToTTopics,
};

pub const REALTIME_HOST: &str = "edge-mqtt.facebook.com";
pub const IG_REALTIME_APP_ID: i64 = 567067343352427;
pub const REALTIME_SUBSCRIBE_TOPICS: &[i64] = &[88, 135, 149, 150, 133, 146];

/// Cap on a single inbound MQTT packet (IG messages never approach this).
const MAX_PACKET_SIZE: usize = 16 * 1024 * 1024;

type Handler = Arc<dyn Fn(Value) + Send + Sync>;

type ReadHalf = tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>;
type WriteHalf = tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>;

struct Transport {
    read: Mutex<ReadHalf>,
    write: Mutex<WriteHalf>,
}

impl Transport {
    async fn connect(host: &str, port: u16) -> Result<Self> {
        let addr = format!("{host}:{port}");
        let tcp = TcpStream::connect(&addr).await.map_err(|e| {
            IgError::new(
                crate::error::ErrorKind::ClientConnectionError,
                format!("MQTT connect: {e}"),
            )
        })?;
        let connector = TlsConnector::from(tls_config().clone());
        let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|_| IgError::client_error("bad MQTT server name"))?;
        let stream = connector.connect(server_name, tcp).await.map_err(|e| {
            IgError::new(
                crate::error::ErrorKind::ClientConnectionError,
                format!("MQTT TLS handshake: {e}"),
            )
        })?;
        let (read, write) = tokio::io::split(stream);
        Ok(Self {
            read: Mutex::new(read),
            write: Mutex::new(write),
        })
    }

    async fn send(&self, packet: &[u8]) -> Result<()> {
        let mut write = self.write.lock().await;
        write.write_all(packet).await.map_err(|e| {
            IgError::new(
                crate::error::ErrorKind::ClientConnectionError,
                format!("MQTT send: {e}"),
            )
        })?;
        write.flush().await.map_err(|e| {
            IgError::new(
                crate::error::ErrorKind::ClientConnectionError,
                format!("MQTT flush: {e}"),
            )
        })
    }

    /// Read one full packet; `timeout` guards the read (socket timeout).
    async fn recv_packet(&self, timeout: Duration) -> Result<Vec<u8>> {
        tokio::time::timeout(timeout, async {
            let mut read = self.read.lock().await;
            let mut first = [0u8; 1];
            read.read_exact(&mut first).await.map_err(|e| {
                IgError::new(
                    crate::error::ErrorKind::ClientConnectionError,
                    format!("MQTT read: {e}"),
                )
            })?;
            let mut remaining = Vec::new();
            loop {
                let mut byte = [0u8; 1];
                read.read_exact(&mut byte).await.map_err(|e| {
                    IgError::new(
                        crate::error::ErrorKind::ClientConnectionError,
                        format!("MQTT read: {e}"),
                    )
                })?;
                remaining.push(byte[0]);
                if byte[0] & 0x80 == 0 {
                    break;
                }
            }
            let (size, _) =
                mqttot::decode_remaining_length(&remaining, 0).map_err(IgError::client_error)?;
            if size > MAX_PACKET_SIZE {
                return Err(IgError::client_error("MQTT packet too large"));
            }
            let mut rest = vec![0u8; size];
            read.read_exact(&mut rest).await.map_err(|e| {
                IgError::new(
                    crate::error::ErrorKind::ClientConnectionError,
                    format!("MQTT read: {e}"),
                )
            })?;
            let mut packet = Vec::with_capacity(1 + remaining.len() + rest.len());
            packet.push(first[0]);
            packet.extend_from_slice(&remaining);
            packet.extend_from_slice(&rest);
            Ok(packet)
        })
        .await
        .map_err(|_| {
            IgError::new(
                crate::error::ErrorKind::ClientRequestTimeout,
                "MQTT read timeout",
            )
        })?
    }

    /// Force-close the socket (unblocks a pending read with EOF/error).
    async fn close(&self) {
        let mut write = self.write.lock().await;
        let _ = write.shutdown().await;
    }
}

/// Process-wide TLS config: native certs loaded once, reused across
/// reconnects (rustls ClientConfig is not Clone — cache the Arc).
fn tls_config() -> &'static Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let mut store = rustls::RootCertStore::empty();
        let result = rustls_native_certs::load_native_certs();
        for cert in result.certs {
            let _ = store.add(cert);
        }
        if store.is_empty() {
            store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(store)
            .with_no_client_auth();
        Arc::new(config)
    })
}

/// Realtime client: owns the transport + handlers. All socket writes go
/// through one mutex so the reader task and UI publishes serialize.
pub struct RealtimeClient {
    client: Client,
    transport: Mutex<Option<Transport>>,
    handlers: std::sync::Mutex<HashMap<String, Vec<Handler>>>,
    connected: std::sync::atomic::AtomicBool,
    packet_id: std::sync::atomic::AtomicU16,
}

impl RealtimeClient {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            transport: Mutex::new(None),
            handlers: std::sync::Mutex::new(HashMap::new()),
            connected: std::sync::atomic::AtomicBool::new(false),
            packet_id: std::sync::atomic::AtomicU16::new(0),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn on(&self, event: &str, handler: impl Fn(Value) + Send + Sync + 'static) {
        let mut handlers = self.handlers.lock().expect("handlers mutex");
        handlers
            .entry(event.to_string())
            .or_default()
            .push(Arc::new(handler));
    }

    fn has_handlers(&self, event: &str) -> bool {
        self.handlers
            .lock()
            .map(|h| h.contains_key(event))
            .unwrap_or(false)
    }

    fn emit(&self, event: &str, payload: Value) {
        let handlers = self.handlers.lock().expect("handlers mutex");
        let Some(list) = handlers.get(event) else {
            return;
        };
        let snapshot: Vec<Handler> = list.clone();
        for handler in snapshot {
            handler(payload.clone());
        }
    }

    pub async fn connect(&self) -> Result<()> {
        let mut transport = self.transport.lock().await;
        let t = Transport::connect(REALTIME_HOST, 443).await?;
        let connection = self.build_connection().await?;
        let packet = write_connect_packet(&connection, 20);
        t.send(&packet).await?;
        let reply = t.recv_packet(Duration::from_secs(30)).await?;
        let decoded = decode_packet(&reply).map_err(IgError::client_error)?;
        if decoded.packet_type != "connack" || decoded.return_code != Some(0) {
            return Err(IgError::client_error(format!(
                "Realtime MQTT connect failed: {:?}",
                decoded.return_code
            )));
        }
        *transport = Some(t);
        self.connected
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    pub async fn disconnect(&self) {
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let mut transport = self.transport.lock().await;
        if let Some(t) = transport.as_ref() {
            let _ = t.send(&write_disconnect_packet()).await;
            let _ = t.close().await;
        }
        *transport = None;
    }

    /// Close the socket without a DISCONNECT (used to unblock the reader).
    pub async fn shutdown_transport(&self) {
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let transport = self.transport.lock().await;
        if let Some(t) = transport.as_ref() {
            let _ = t.close().await;
        }
    }

    async fn build_connection(&self) -> Result<Map<String, Value>> {
        let state = self.client.state().await;
        let sessionid = state
            .sessionid()
            .ok_or_else(|| IgError::client_error("Login required"))?;
        let device_id = state.phone_id.clone();
        let user_agent = state.user_agent.clone();
        let app_version = state.app_version();
        let capabilities = "3brTv10=".to_string();
        let locale = state.locale.clone();
        let user_id = state
            .user_id()
            .ok_or_else(|| IgError::client_error("Login required"))?;
        drop(state);

        let mut client_info = Map::new();
        client_info.insert("userId".to_string(), json!(user_id));
        client_info.insert("userAgent".to_string(), json!(user_agent.clone()));
        client_info.insert("clientCapabilities".to_string(), json!(183));
        client_info.insert("endpointCapabilities".to_string(), json!(0));
        client_info.insert("publishFormat".to_string(), json!(1));
        client_info.insert("noAutomaticForeground".to_string(), json!(false));
        client_info.insert("makeUserAvailableInForeground".to_string(), json!(true));
        client_info.insert("deviceId".to_string(), json!(device_id.clone()));
        client_info.insert("isInitiallyForeground".to_string(), json!(true));
        client_info.insert("networkType".to_string(), json!(1));
        client_info.insert("networkSubtype".to_string(), json!(0));
        let now_ms = crate::utils::now_ms();
        client_info.insert(
            "clientMqttSessionId".to_string(),
            json!(now_ms & 0xFFFF_FFFF),
        );
        client_info.insert(
            "subscribeTopics".to_string(),
            json!(REALTIME_SUBSCRIBE_TOPICS),
        );
        client_info.insert("clientType".to_string(), json!("cookie_auth"));
        client_info.insert("appId".to_string(), json!(IG_REALTIME_APP_ID));
        client_info.insert("deviceSecret".to_string(), json!(""));
        client_info.insert("clientStack".to_string(), json!(3));

        let mut app_specific = Map::new();
        app_specific.insert("app_version".to_string(), json!(app_version));
        app_specific.insert("X-IG-Capabilities".to_string(), json!(capabilities));
        app_specific.insert(
            "everclear_subscriptions".to_string(),
            json!(serde_json::to_string(&json!({
                "inapp_notification_subscribe_comment": "17899377895239777",
                "inapp_notification_subscribe_comment_mention_and_reply": "17899377895239777",
                "video_call_participant_state_delivery": "17977239895057311",
                "presence_subscribe": "17846944882223835",
            }))
            .unwrap_or_default()),
        );
        app_specific.insert("User-Agent".to_string(), json!(user_agent));
        app_specific.insert(
            "Accept-Language".to_string(),
            json!(locale.replace('_', "-")),
        );
        app_specific.insert("platform".to_string(), json!("android"));
        app_specific.insert("ig_mqtt_route".to_string(), json!("django"));
        app_specific.insert(
            "pubsub_msg_type_blacklist".to_string(),
            json!("direct, typing_type"),
        );
        app_specific.insert("auth_cache_enabled".to_string(), json!("0"));

        let mut connection: Map<String, Value> = Map::new();
        connection.insert(
            "clientIdentifier".to_string(),
            json!(device_id.chars().take(20).collect::<String>()),
        );
        connection.insert("clientInfo".to_string(), json!(Value::Object(client_info)));
        connection.insert(
            "password".to_string(),
            json!(format!("sessionid={sessionid}")),
        );
        connection.insert(
            "appSpecificInfo".to_string(),
            json!(Value::Object(app_specific)),
        );
        Ok(connection)
    }

    // ------------------------------------------------------------ subscribe

    /// `direct_subscribe` — fetch the inbox seq state, then subscribe via IRIS.
    pub async fn direct_subscribe(&self) -> Result<Map<String, Value>> {
        let threads = self.client.direct_threads(1).await?;
        let _ = threads;
        let last_json = self.client.state().await.last_json.clone();
        let seq_id = last_json.get("seq_id").cloned();
        let snapshot_at_ms = last_json.get("snapshot_at_ms").cloned();
        let (Some(seq_id), Some(snapshot_at_ms)) = (seq_id, snapshot_at_ms) else {
            return Err(IgError::client_error(
                "Direct inbox did not return realtime sync state",
            ));
        };
        let mut payload = Map::new();
        payload.insert("seq_id".to_string(), seq_id);
        payload.insert("snapshot_at_ms".to_string(), snapshot_at_ms);
        payload.insert(
            "snapshot_app_version".to_string(),
            json!(self.client.state().await.app_version()),
        );
        self.publish_json(MQTToTTopics::IRIS_SUB, &Value::Object(payload.clone()))
            .await?;
        Ok(payload)
    }

    /// `iris_subscribe` (used directly by some flows).
    pub async fn iris_subscribe(&self, seq_id: Value, snapshot_at_ms: Value) -> Result<()> {
        let mut payload = Map::new();
        payload.insert("seq_id".to_string(), seq_id);
        payload.insert("snapshot_at_ms".to_string(), snapshot_at_ms);
        payload.insert(
            "snapshot_app_version".to_string(),
            json!(self.client.state().await.app_version()),
        );
        self.publish_json(MQTToTTopics::IRIS_SUB, &Value::Object(payload))
            .await
    }

    // ------------------------------------------------------------ commands

    pub async fn direct_indicate_activity(&self, thread_id: &str, is_active: bool) -> Result<()> {
        let mut payload = Map::new();
        payload.insert("action".to_string(), json!("indicate_activity"));
        payload.insert("thread_id".to_string(), json!(thread_id));
        payload.insert(
            "activity_status".to_string(),
            json!(if is_active { "1" } else { "0" }),
        );
        payload.insert(
            "client_context".to_string(),
            json!(crate::utils::generate_uuid()),
        );
        self.publish_bytes(
            MQTToTTopics::SEND_MESSAGE,
            &compress_payload(&crate::utils::dumps(&Value::Object(payload)).into_bytes()),
        )
        .await
    }

    async fn publish_json(&self, topic: &str, data: &Value) -> Result<()> {
        let raw = crate::utils::dumps(data);
        let payload = compress_payload(raw.as_bytes());
        self.publish_bytes(topic, &payload).await
    }

    async fn publish_bytes(&self, topic: &str, payload: &[u8]) -> Result<()> {
        let id = self
            .packet_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let packet = write_publish_packet(topic, payload, 1, id);
        let transport = self.transport.lock().await;
        match transport.as_ref() {
            Some(t) => t.send(&packet).await,
            None => Err(IgError::new(
                crate::error::ErrorKind::MqttNotConnected,
                "Realtime client is not connected",
            )),
        }
    }

    pub async fn ping(&self) -> Result<bool> {
        if !self.is_connected() {
            return Err(IgError::new(
                crate::error::ErrorKind::MqttNotConnected,
                "Realtime client is not connected",
            ));
        }
        {
            let transport = self.transport.lock().await;
            if let Some(t) = transport.as_ref() {
                t.send(&write_pingreq_packet()).await?;
            }
        }
        for _ in 0..5 {
            let packet = self.read_once().await?;
            match packet {
                Some(p) if p == "pingresp" => return Ok(true),
                Some(_) => continue,
                None => return Ok(false),
            }
        }
        Ok(false)
    }

    /// Read one packet and dispatch it (returns the packet kind for ping).
    pub async fn read_once(&self) -> Result<Option<String>> {
        let transport = self.transport.lock().await;
        let Some(t) = transport.as_ref() else {
            return Err(IgError::new(
                crate::error::ErrorKind::MqttNotConnected,
                "Realtime client is not connected",
            ));
        };
        let packet = t.recv_packet(Duration::from_secs(30)).await?;
        let decoded = decode_packet(&packet).map_err(IgError::client_error)?;
        if decoded.packet_type != "publish" {
            return Ok(Some(decoded.packet_type));
        }
        let topic = decoded.topic.unwrap_or_default();
        let payload = decoded.payload;
        self.dispatch_packet(&topic, &payload).await;
        if decoded.qos == 1 {
            if let Some(id) = decoded.packet_id {
                t.send(&write_puback_packet(id)).await?;
            }
        }
        Ok(Some(topic))
    }

    // ------------------------------------------------------------ dispatch

    async fn dispatch_packet(&self, topic: &str, payload: &[u8]) -> Option<Value> {
        let body = try_decompress_payload(payload);
        let parsed: Value = serde_json::from_slice(body.as_ref())
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body.as_ref()).into_owned()));
        // Verbose diagnostics: surface every inbound payload so unknown
        // event shapes (e.g. reactions from other devices) are visible.
        eprintln!(
            "[igdm-mqtt] recv topic={topic} payload={}",
            crate::utils::json_preview(&parsed, 600)
        );
        if self.has_handlers("receive") {
            self.emit(
                "receive",
                json!({"topic": topic, "payload": parsed.clone()}),
            );
        }
        if topic == MQTToTTopics::SEND_MESSAGE_RESPONSE {
            self.emit("send_response", parsed.clone());
        } else if topic == MQTToTTopics::IRIS_SUB_RESPONSE {
            self.emit("iris_sub_response", parsed.clone());
        } else if topic == MQTToTTopics::MESSAGE_SYNC {
            self.dispatch_message_sync(&parsed);
        } else if topic == MQTToTTopics::REALTIME_SUB {
            self.dispatch_realtime_sub(&parsed);
        }
        Some(parsed)
    }

    fn dispatch_message_sync(&self, payload: &Value) {
        let Some(items) = payload.as_array() else {
            self.emit("message", payload.clone());
            return;
        };
        for item in items {
            let Some(patches) = item.get("data").and_then(|d| d.as_array()) else {
                self.emit("iris", item.clone());
                continue;
            };
            let mut meta = Map::new();
            if let Some(obj) = item.as_object() {
                for (k, v) in obj {
                    if k != "data" {
                        meta.insert(k.clone(), v.clone());
                    }
                }
            }
            for patch in patches {
                let Some(patch_obj) = patch.as_object() else {
                    self.emit("iris", item.clone());
                    continue;
                };
                if patch_obj.is_empty() {
                    self.emit("iris", item.clone());
                    continue;
                }
                let path = patch_obj.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let value: Value = match patch_obj.get("value") {
                    Some(Value::String(s)) => {
                        serde_json::from_str(s).unwrap_or_else(|_| json!({"value": s}))
                    }
                    Some(other) => other.clone(),
                    None => Value::Null,
                };
                if path.is_empty() || value.is_null() {
                    let mut iris = meta.clone();
                    for (k, v) in patch_obj {
                        iris.insert(k.clone(), v.clone());
                    }
                    self.emit("iris", Value::Object(iris));
                    continue;
                }
                let op = patch_obj.get("op").cloned().unwrap_or(Value::Null);
                eprintln!(
                    "[igdm-mqtt] iris patch path={path} op={} value={}",
                    serde_json::to_string(&op).unwrap_or_default(),
                    crate::utils::json_preview(&value, 600)
                );
                let mut message = Map::new();
                message.insert("path".to_string(), json!(path));
                message.insert("op".to_string(), op.clone());
                if let Some(tid) = thread_id_from_path(path) {
                    message.insert("thread_id".to_string(), json!(tid));
                }
                if let Value::Object(v) = &value {
                    for (k, val) in v {
                        message.insert(k.clone(), val.clone());
                    }
                } else {
                    message.insert("value".to_string(), value.clone());
                }
                let mut wrapper = meta.clone();
                wrapper.insert("message".to_string(), Value::Object(message));
                if path.starts_with("/direct_v2/threads/") {
                    self.emit("message", Value::Object(wrapper));
                } else {
                    self.emit("thread_update", Value::Object(wrapper));
                }
                let mut event = Map::new();
                event.insert("path".to_string(), json!(path));
                event.insert("op".to_string(), op);
                event.insert("value".to_string(), value);
                self.dispatch_direct_realtime_event(Value::Object(event));
            }
        }
    }

    fn dispatch_realtime_sub(&self, payload: &Value) {
        self.emit("realtime_sub", payload.clone());
        let Some(message) = payload.get("message") else {
            return;
        };
        let direct_payload: Value = match message {
            Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({"value": s})),
            Value::Object(m) => {
                if m.get("topic").and_then(|t| t.as_str()) != Some("direct") {
                    return;
                }
                let inner = m.get("json").or_else(|| m.get("payload"));
                match inner {
                    Some(Value::String(s)) => {
                        serde_json::from_str(s).unwrap_or_else(|_| json!({"value": s}))
                    }
                    Some(other) => other.clone(),
                    None => Value::Null,
                }
            }
            _ => return,
        };
        self.dispatch_direct_realtime_payload(direct_payload);
    }

    fn dispatch_direct_realtime_payload(&self, payload: Value) {
        if !payload.is_object() {
            self.dispatch_direct_realtime_event(json!({"value": payload}));
            return;
        }
        let data = payload.get("data").cloned().unwrap_or(Value::Null);
        if !data.is_array() {
            self.dispatch_direct_realtime_event(payload);
            return;
        }
        let mut meta = Map::new();
        if let Some(obj) = payload.as_object() {
            for (k, v) in obj {
                if k != "data" {
                    meta.insert(k.clone(), v.clone());
                }
            }
        }
        let items = data.as_array().cloned().unwrap_or_default();
        for item in items {
            if item.is_object() {
                let mut event = meta.clone();
                if let Some(obj) = item.as_object() {
                    for (k, v) in obj {
                        event.insert(k.clone(), v.clone());
                    }
                }
                self.dispatch_direct_realtime_event(Value::Object(event));
            } else {
                let mut event = meta.clone();
                event.insert("value".to_string(), item);
                self.dispatch_direct_realtime_event(Value::Object(event));
            }
        }
    }

    fn dispatch_direct_realtime_event(&self, mut event: Value) {
        if let Some(obj) = event.as_object_mut() {
            if let Some(value) = obj.get("value").cloned() {
                let parsed = match value {
                    Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
                    other => other,
                };
                obj.insert("value".to_string(), parsed);
            }
            if !obj.contains_key("thread_id") {
                if let Some(path) = obj.get("path").and_then(|p| p.as_str()) {
                    if let Some(tid) = thread_id_from_path(path) {
                        obj.insert("thread_id".to_string(), json!(tid));
                    }
                }
            }
        }
        self.emit("direct", event.clone());
        eprintln!(
            "[igdm-mqtt] direct event: {}",
            crate::utils::json_preview(&event, 600)
        );
        match direct_realtime_event_kind(&event) {
            Some(kind) => self.emit(kind, event),
            None => eprintln!(
                "[instagrapi] unhandled realtime event: {}",
                crate::utils::dumps(&event)
            ),
        }
    }
}

fn thread_id_from_path(path: &str) -> Option<String> {
    if let Some(rest) = path.strip_prefix("/direct_v2/threads/") {
        return rest.split('/').next().map(|s| s.to_string());
    }
    if let Some(rest) = path.strip_prefix("/direct_v2/inbox/threads/") {
        return rest.split('/').next().map(|s| s.to_string());
    }
    None
}

/// Substring match over the serialized value text: keys and string values
/// anywhere in the tree (matches the previous whole-object `to_string`).
fn value_contains_text(value: &Value, needle: &str) -> bool {
    match value {
        Value::Object(o) => o.iter().any(|(k, v)| {
            k.to_lowercase().contains(needle) || value_contains_text(v, needle)
        }),
        Value::Array(a) => a.iter().any(|v| value_contains_text(v, needle)),
        Value::String(s) => s.to_lowercase().contains(needle),
        _ => false,
    }
}

fn direct_realtime_event_kind(event: &Value) -> Option<&'static str> {
    let path = event
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let action = event
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let value = event.get("value");
    let value_has = |needle: &str| value.is_some_and(|v| value_contains_text(v, needle));
    if value_has("activity_status")
        || path.contains("activity_indicator")
        || path.contains("typing")
    {
        return Some("typing");
    }
    // Reaction patches: `/items/{item_id}/reactions/likes/{user_id}` (and
    // `/reactions/emojis/...`). The reacting user rides in the path; the
    // value carries the emoji and the target `message_id`.
    if path.contains("/reactions/") {
        return Some("reaction");
    }
    if path.contains("presence")
        || value_has("is_active")
        || value_has("last_active")
    {
        return Some("presence");
    }
    if action == "mark_seen"
        || path.contains("/seen")
        || path.contains("seen_")
        || path.contains("read")
        || value_has("seen")
    {
        return Some("seen");
    }
    None
}
