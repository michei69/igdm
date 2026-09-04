//! MQTToT packet codec ported from `instagrapi.realtime.mqttot`:
//! MQTT 3.1.1 packets with a zlib-compressed Thrift CONNECT payload.
//! Pure safe Rust.

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::borrow::Cow;
use std::io::Read;
use std::io::Write;

use serde_json::{Map, Value};

pub struct MQTToTTopics;

impl MQTToTTopics {
    pub const PUBSUB: &'static str = "88";
    pub const FOREGROUND_STATE: &'static str = "102";
    pub const SEND_MESSAGE: &'static str = "132";
    pub const SEND_MESSAGE_RESPONSE: &'static str = "133";
    pub const IRIS_SUB: &'static str = "134";
    pub const IRIS_SUB_RESPONSE: &'static str = "135";
    pub const MESSAGE_SYNC: &'static str = "146";
    pub const REALTIME_SUB: &'static str = "149";
    pub const REGION_HINT: &'static str = "150";
}

pub(crate) struct ThriftTypes;

impl ThriftTypes {
    pub const STOP: u8 = 0x00;
    pub const TRUE: u8 = 0x01;
    pub const FALSE: u8 = 0x02;
    pub const BYTE: u8 = 0x03;
    pub const INT_16: u8 = 0x04;
    pub const INT_32: u8 = 0x05;
    pub const INT_64: u8 = 0x06;
    pub const BINARY: u8 = 0x08;
    pub const LIST: u8 = 0x09;
    pub const MAP: u8 = 0x0B;
    pub const STRUCT: u8 = 0x0C;
    pub const BOOLEAN: u8 = 0xA1;
    pub const LIST_INT_32: u16 = (ThriftTypes::INT_32 as u16) << 8 | ThriftTypes::LIST as u16;
    pub const LIST_BINARY: u16 = (ThriftTypes::BINARY as u16) << 8 | ThriftTypes::LIST as u16;
    pub const MAP_BINARY_BINARY: u16 = (0x88 << 8) | ThriftTypes::MAP as u16;
}

#[derive(Clone, Debug)]
pub(crate) struct ThriftDescriptor {
    pub name: &'static str,
    pub field: u8,
    pub kind: u16,
    pub children: Vec<ThriftDescriptor>,
}

pub(crate) fn desc(name: &'static str, field: u8, kind: u16) -> ThriftDescriptor {
    ThriftDescriptor {
        name,
        field,
        kind,
        children: Vec::new(),
    }
}

pub(crate) fn struct_desc(
    name: &'static str,
    field: u8,
    children: Vec<ThriftDescriptor>,
) -> ThriftDescriptor {
    ThriftDescriptor {
        name,
        field,
        kind: ThriftTypes::STRUCT as u16,
        children,
    }
}

/// Thrift field descriptors for the MQTToT CONNECT payload.
pub(crate) fn connection_descriptors() -> &'static [ThriftDescriptor] {
    use std::sync::OnceLock;
    static DESCRIPTORS: OnceLock<Vec<ThriftDescriptor>> = OnceLock::new();
    DESCRIPTORS.get_or_init(|| {
        vec![
            desc("clientIdentifier", 1, ThriftTypes::BINARY as u16),
            desc("willTopic", 2, ThriftTypes::BINARY as u16),
            desc("willMessage", 3, ThriftTypes::BINARY as u16),
            struct_desc(
                "clientInfo",
                4,
                vec![
                    desc("userId", 1, ThriftTypes::INT_64 as u16),
                    desc("userAgent", 2, ThriftTypes::BINARY as u16),
                    desc("clientCapabilities", 3, ThriftTypes::INT_64 as u16),
                    desc("endpointCapabilities", 4, ThriftTypes::INT_64 as u16),
                    desc("publishFormat", 5, ThriftTypes::INT_32 as u16),
                    desc("noAutomaticForeground", 6, ThriftTypes::BOOLEAN as u16),
                    desc(
                        "makeUserAvailableInForeground",
                        7,
                        ThriftTypes::BOOLEAN as u16,
                    ),
                    desc("deviceId", 8, ThriftTypes::BINARY as u16),
                    desc("isInitiallyForeground", 9, ThriftTypes::BOOLEAN as u16),
                    desc("networkType", 10, ThriftTypes::INT_32 as u16),
                    desc("networkSubtype", 11, ThriftTypes::INT_32 as u16),
                    desc("clientMqttSessionId", 12, ThriftTypes::INT_64 as u16),
                    desc("clientIpAddress", 13, ThriftTypes::BINARY as u16),
                    desc("subscribeTopics", 14, ThriftTypes::LIST_INT_32),
                    desc("clientType", 15, ThriftTypes::BINARY as u16),
                    desc("appId", 16, ThriftTypes::INT_64 as u16),
                    desc("overrideNectarLogging", 17, ThriftTypes::BOOLEAN as u16),
                    desc("connectTokenHash", 18, ThriftTypes::BINARY as u16),
                    desc("regionPreference", 19, ThriftTypes::BINARY as u16),
                    desc("deviceSecret", 20, ThriftTypes::BINARY as u16),
                    desc("clientStack", 21, ThriftTypes::BYTE as u16),
                    desc("fbnsConnectionKey", 22, ThriftTypes::INT_64 as u16),
                    desc("fbnsConnectionSecret", 23, ThriftTypes::BINARY as u16),
                    desc("fbnsDeviceId", 24, ThriftTypes::BINARY as u16),
                    desc("fbnsDeviceSecret", 25, ThriftTypes::BINARY as u16),
                    desc("anotherUnknown", 26, ThriftTypes::INT_64 as u16),
                ],
            ),
            desc("password", 5, ThriftTypes::BINARY as u16),
            desc("getDiffsRequests", 6, ThriftTypes::LIST_BINARY),
            desc("zeroRatingTokenHash", 9, ThriftTypes::BINARY as u16),
            desc("appSpecificInfo", 10, ThriftTypes::MAP_BINARY_BINARY),
        ]
    })
}

/// zlib level 9 (`compress_payload`).
pub fn compress_payload(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(9));
    let _ = encoder.write_all(data);
    encoder.finish().expect("zlib compress")
}

/// `try_decompress_payload`: zlib-decompress only if it looks compressed.
/// Borrows the input when no decompression is needed (zero-copy).
pub fn try_decompress_payload(data: &[u8]) -> Cow<'_, [u8]> {
    if data.is_empty() || data[0] != 0x78 {
        return Cow::Borrowed(data);
    }
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    match decoder.read_to_end(&mut out) {
        Ok(_) => Cow::Owned(out),
        Err(_) => Cow::Borrowed(data),
    }
}

fn write_utf8(value: &str) -> Vec<u8> {
    let raw = value.as_bytes();
    let mut out = Vec::with_capacity(2 + raw.len());
    out.extend_from_slice(&(raw.len() as u16).to_be_bytes());
    out.extend_from_slice(raw);
    out
}

fn encode_remaining_length(value: usize) -> Vec<u8> {
    let mut encoded = Vec::new();
    let mut v = value;
    loop {
        let mut byte = (v % 128) as u8;
        v /= 128;
        if v > 0 {
            byte |= 0x80;
        }
        encoded.push(byte);
        if v == 0 {
            return encoded;
        }
    }
}

/// MQTT remaining-length varint (max 4 bytes per spec).
pub(crate) fn decode_remaining_length(data: &[u8], offset: usize) -> Result<(usize, usize), String> {
    let mut multiplier = 1usize;
    let mut value = 0usize;
    let mut pos = offset;
    loop {
        if pos - offset >= 4 {
            return Err("malformed remaining length".to_string());
        }
        let byte = *data.get(pos).ok_or("malformed remaining length")?;
        pos += 1;
        value += ((byte & 0x7f) as usize) * multiplier;
        if byte & 0x80 == 0 {
            return Ok((value, pos));
        }
        multiplier *= 128;
    }
}

/// `write_connect_packet` — protocol "MQTToT", level 3, flags 0xC2, keepalive.
pub fn write_connect_packet(connection: &Map<String, Value>, keep_alive: u16) -> Vec<u8> {
    let thrift = write_thrift_object(connection);
    let payload = compress_payload(&thrift);
    let mut packet = Vec::with_capacity(1 + 4 + 2 + 6 + 4 + payload.len());
    packet.push(0x10);
    packet.extend_from_slice(&encode_remaining_length(payload.len() + 12));
    packet.extend_from_slice(&(b"MQTToT".len() as u16).to_be_bytes());
    packet.extend_from_slice(b"MQTToT");
    packet.extend_from_slice(&[3, 0xC2]);
    packet.extend_from_slice(&keep_alive.to_be_bytes());
    packet.extend_from_slice(&payload);
    packet
}

pub fn write_publish_packet(topic: &str, payload: &[u8], qos: u8, packet_id: u16) -> Vec<u8> {
    assert!(qos <= 1, "Only QoS 0 and QoS 1 are supported");
    let remaining_len = 2 + topic.len() + if qos > 0 { 2 } else { 0 } + payload.len();
    let mut packet = Vec::with_capacity(1 + 4 + remaining_len);
    packet.push(0x30 | (qos << 1));
    packet.extend_from_slice(&encode_remaining_length(remaining_len));
    packet.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    packet.extend_from_slice(topic.as_bytes());
    if qos > 0 {
        packet.extend_from_slice(&packet_id.to_be_bytes());
    }
    packet.extend_from_slice(payload);
    packet
}

pub fn write_subscribe_packet(topic: &str, packet_id: u16, qos: u8) -> Vec<u8> {
    let mut body = packet_id.to_be_bytes().to_vec();
    body.extend_from_slice(&write_utf8(topic));
    body.push(qos);
    let mut packet = vec![0x82];
    packet.extend_from_slice(&encode_remaining_length(body.len()));
    packet.extend_from_slice(&body);
    packet
}

pub fn write_pingreq_packet() -> Vec<u8> {
    vec![0xC0, 0x00]
}

pub fn write_disconnect_packet() -> Vec<u8> {
    vec![0xE0, 0x00]
}

pub fn write_puback_packet(packet_id: u16) -> Vec<u8> {
    vec![0x40, 0x02, (packet_id >> 8) as u8, (packet_id & 0xff) as u8]
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DecodedPacket {
    pub packet_type: String,
    pub payload: Vec<u8>,
    pub topic: Option<String>,
    pub qos: u8,
    pub packet_id: Option<u16>,
    pub return_code: Option<i64>,
}

/// `decode_packet` — parse one full MQTT packet.
pub(crate) fn decode_packet(packet: &[u8]) -> Result<DecodedPacket, String> {
    if packet.is_empty() {
        return Err("empty packet".to_string());
    }
    let packet_type_id = packet[0] >> 4;
    let flags = packet[0] & 0x0F;
    let (remaining_length, offset) = decode_remaining_length(packet, 1)?;
    let body = packet
        .get(offset..offset + remaining_length)
        .ok_or("packet shorter than remaining length")?;
    match packet_type_id {
        1 => {
            let (protocol_name, pos) = read_utf8(body, 0)?;
            let keep_alive = body
                .get(pos + 2..pos + 4)
                .map(|b| u16::from_be_bytes([b[0], b[1]]))
                .ok_or("truncated connect")?;
            let payload = body.get(pos + 4..).unwrap_or_default().to_vec();
            Ok(DecodedPacket {
                packet_type: "connect".to_string(),
                payload,
                topic: Some(protocol_name),
                qos: 0,
                packet_id: None,
                return_code: Some(keep_alive as i64),
            })
        }
        2 => Ok(DecodedPacket {
            packet_type: "connack".to_string(),
            payload: body[2.min(body.len())..].to_vec(),
            topic: None,
            qos: 0,
            packet_id: None,
            return_code: body.get(1).copied().map(|b| b as i64),
        }),
        3 => {
            let (topic, pos) = read_utf8(body, 0)?;
            let qos = (flags >> 1) & 0x03;
            let mut pos = pos;
            let mut packet_id = None;
            if qos > 0 {
                packet_id = body
                    .get(pos..pos + 2)
                    .map(|b| u16::from_be_bytes([b[0], b[1]]));
                pos += 2;
            }
            Ok(DecodedPacket {
                packet_type: "publish".to_string(),
                payload: body[pos..].to_vec(),
                topic: Some(topic),
                qos,
                packet_id,
                return_code: None,
            })
        }
        12 => Ok(DecodedPacket {
            packet_type: "pingreq".to_string(),
            payload: body.to_vec(),
            topic: None,
            qos: 0,
            packet_id: None,
            return_code: None,
        }),
        13 => Ok(DecodedPacket {
            packet_type: "pingresp".to_string(),
            payload: body.to_vec(),
            topic: None,
            qos: 0,
            packet_id: None,
            return_code: None,
        }),
        14 => Ok(DecodedPacket {
            packet_type: "disconnect".to_string(),
            payload: body.to_vec(),
            topic: None,
            qos: 0,
            packet_id: None,
            return_code: None,
        }),
        other => Ok(DecodedPacket {
            packet_type: other.to_string(),
            payload: body.to_vec(),
            topic: None,
            qos: 0,
            packet_id: None,
            return_code: None,
        }),
    }
}

fn read_utf8(data: &[u8], offset: usize) -> Result<(String, usize), String> {
    let size = data
        .get(offset..offset + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
        .ok_or("truncated utf8 field")?;
    let start = offset + 2;
    let raw = data
        .get(start..start + size)
        .ok_or("truncated utf8 field")?;
    let text = std::str::from_utf8(raw).map_err(|_| "invalid utf8")?;
    Ok((text.to_string(), start + size))
}

// ------------------------------------------------------------------ thrift

struct ThriftWriter {
    buffer: Vec<u8>,
    field_stack: Vec<u8>,
    field: u8,
}

impl ThriftWriter {
    fn new() -> Self {
        Self {
            buffer: Vec::new(),
            field_stack: Vec::new(),
            field: 0,
        }
    }

    fn write_stop(&mut self) {
        self.buffer.push(ThriftTypes::STOP);
        if let Some(prev) = self.field_stack.pop() {
            self.field = prev;
        }
    }

    fn write_field(&mut self, field: u8, field_type: u8) {
        let delta = field as i16 - self.field as i16;
        let thrift_type = field_type & 0x0F;
        if delta > 0 && delta <= 15 {
            self.buffer.push(((delta as u8) << 4) | thrift_type);
        } else {
            self.buffer.push(thrift_type);
            self.write_varint(zigzag(field as i64));
        }
        self.field = field;
    }

    fn write_varint(&mut self, value: i64) {
        let mut value = value as u64;
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                self.buffer.push(byte);
                return;
            }
            self.buffer.push(byte | 0x80);
        }
    }

    fn write_string_direct(&mut self, value: &str) {
        let raw = value.as_bytes();
        self.write_varint(raw.len() as i64);
        self.buffer.extend_from_slice(raw);
    }

    fn write_bool(&mut self, field: u8, value: bool) {
        self.write_field(
            field,
            if value {
                ThriftTypes::TRUE
            } else {
                ThriftTypes::FALSE
            },
        );
    }

    fn write_int(&mut self, field: u8, value: i64, bits: u8) {
        let field_type = match bits {
            8 => ThriftTypes::BYTE,
            16 => ThriftTypes::INT_16,
            32 => ThriftTypes::INT_32,
            64 => ThriftTypes::INT_64,
            _ => unreachable!(),
        };
        self.write_field(field, field_type);
        if bits == 8 {
            self.buffer.push(value as i8 as u8);
        } else {
            self.write_varint(zigzag(value));
        }
    }

    fn push_struct(&mut self, field: u8) {
        self.write_field(field, ThriftTypes::STRUCT);
        self.field_stack.push(self.field);
        self.field = 0;
    }
}

fn zigzag(value: i64) -> i64 {
    value.wrapping_shl(1) ^ (value >> 63)
}

/// `write_thrift_object` — serialize the connection dict to Thrift.
pub(crate) fn write_thrift_object(data: &Map<String, Value>) -> Vec<u8> {
    let mut writer = ThriftWriter::new();
    write_thrift_struct(&mut writer, data, connection_descriptors());
    writer.write_stop();
    writer.buffer
}

fn write_thrift_struct(
    writer: &mut ThriftWriter,
    data: &Map<String, Value>,
    descriptors: &[ThriftDescriptor],
) {
    for descriptor in descriptors {
        let Some(value) = data.get(descriptor.name) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let thrift_type = (descriptor.kind & 0xFF) as u8;
        match thrift_type {
            t if t == ThriftTypes::BOOLEAN => {
                writer.write_bool(descriptor.field, value.as_bool().unwrap_or(false));
            }
            t if t == ThriftTypes::BYTE => {
                writer.write_int(descriptor.field, value.as_i64().unwrap_or(0), 8);
            }
            t if t == ThriftTypes::INT_16 => {
                writer.write_int(descriptor.field, value.as_i64().unwrap_or(0), 16);
            }
            t if t == ThriftTypes::INT_32 => {
                writer.write_int(descriptor.field, value.as_i64().unwrap_or(0), 32);
            }
            t if t == ThriftTypes::INT_64 => {
                writer.write_int(descriptor.field, value.as_i64().unwrap_or(0), 64);
            }
            t if t == ThriftTypes::BINARY => {
                writer.write_field(descriptor.field, ThriftTypes::BINARY);
                let text = match value {
                    serde_json::Value::String(s) => Cow::Borrowed(s.as_str()),
                    serde_json::Value::Number(n) => Cow::Owned(n.to_string()),
                    serde_json::Value::Bool(b) => Cow::Owned(b.to_string()),
                    _ => Cow::Owned(serde_json::to_string(value).unwrap_or_default()),
                };
                writer.write_string_direct(&text);
            }
            t if t == ThriftTypes::STRUCT => {
                writer.push_struct(descriptor.field);
                if let Some(inner) = value.as_object() {
                    write_thrift_struct(writer, inner, &descriptor.children);
                }
                writer.write_stop();
            }
            t if t == ThriftTypes::LIST => {
                writer.write_field(descriptor.field, ThriftTypes::LIST);
                let item_type = (descriptor.kind >> 8) as u8;
                write_thrift_list(writer, item_type, value);
            }
            t if t == ThriftTypes::MAP => {
                write_thrift_map(writer, descriptor.field, value);
            }
            _ => {}
        }
    }
}

fn write_thrift_list(writer: &mut ThriftWriter, item_type: u8, value: &serde_json::Value) {
    let values = value.as_array();
    let size = values.map_or(0, |a| a.len());
    if size < 0x0F {
        writer.buffer.push(((size as u8) << 4) | item_type);
    } else {
        writer.buffer.push(0xF0 | item_type);
        writer.write_varint(size as i64);
    }
    if let Some(values) = values {
        for value in values {
            if item_type == ThriftTypes::INT_32 {
                writer.write_varint(zigzag(value.as_i64().unwrap_or(0)));
            } else if item_type == ThriftTypes::BINARY {
                writer.write_string_direct(value.as_str().unwrap_or_default());
            }
        }
    }
}

fn write_thrift_map(writer: &mut ThriftWriter, field: u8, value: &serde_json::Value) {
    writer.write_field(field, ThriftTypes::MAP);
    let map = value.as_object();
    writer.write_varint(map.map_or(0, |m| m.len()) as i64);
    if let Some(map) = map {
        if map.is_empty() {
            return;
        }
        writer
            .buffer
            .push((ThriftTypes::BINARY << 4) | ThriftTypes::BINARY);
        for (key, value) in map {
            writer.write_string_direct(key);
            writer.write_string_direct(value.as_str().unwrap_or_default());
        }
    }
}
