//! Error hierarchy mirroring `instagrapi.exceptions`, raised from the
//! private API response mapping in `private_request`.

use std::fmt;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct IgError {
    pub kind: ErrorKind,
    pub message: String,
    /// Raw response JSON when available (used for challenge/2FA context).
    pub json: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    ClientError,
    ClientBadRequestError,
    ClientForbiddenError,
    ClientNotFoundError,
    ClientRequestTimeout,
    ClientConnectionError,
    ClientIncompleteReadError,
    ClientThrottledError,
    ClientUnauthorizedError,
    ClientJSONDecodeError,
    LoginRequired,
    ChallengeRequired,
    ChallengeError,
    TwoFactorRequired,
    BadPassword,
    FeedbackRequired,
    PleaseWaitFewMinutes,
    SentryBlock,
    RateLimitError,
    AccountSuspended,
    AccountContactPointRequired,
    AccountEditError,
    DirectMessageRequestsDisabled,
    PrivateAccount,
    InvalidTargetUser,
    InvalidMediaId,
    MediaUnavailable,
    UserNotFound,
    VideoTooLongException,
    ProxyAddressIsBlocked,
    UnknownError,
    DirectThreadNotFound,
    MqttNotConnected,
}

impl IgError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            json: Value::Null,
        }
    }

    pub fn with_json(kind: ErrorKind, message: impl Into<String>, json: Value) -> Self {
        Self {
            kind,
            message: message.into(),
            json,
        }
    }

    pub fn client_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ClientError, message)
    }

    pub fn is(&self, kind: ErrorKind) -> bool {
        self.kind == kind
    }
}

impl fmt::Display for IgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind_name(), self.message)
    }
}

impl std::error::Error for IgError {}

impl IgError {
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ErrorKind::ClientError => "ClientError",
            ErrorKind::ClientBadRequestError => "ClientBadRequestError",
            ErrorKind::ClientForbiddenError => "ClientForbiddenError",
            ErrorKind::ClientNotFoundError => "ClientNotFoundError",
            ErrorKind::ClientRequestTimeout => "ClientRequestTimeout",
            ErrorKind::ClientConnectionError => "ClientConnectionError",
            ErrorKind::ClientIncompleteReadError => "ClientIncompleteReadError",
            ErrorKind::ClientThrottledError => "ClientThrottledError",
            ErrorKind::ClientUnauthorizedError => "ClientUnauthorizedError",
            ErrorKind::ClientJSONDecodeError => "ClientJSONDecodeError",
            ErrorKind::LoginRequired => "LoginRequired",
            ErrorKind::ChallengeRequired => "ChallengeRequired",
            ErrorKind::ChallengeError => "ChallengeError",
            ErrorKind::TwoFactorRequired => "TwoFactorRequired",
            ErrorKind::BadPassword => "BadPassword",
            ErrorKind::FeedbackRequired => "FeedbackRequired",
            ErrorKind::PleaseWaitFewMinutes => "PleaseWaitFewMinutes",
            ErrorKind::SentryBlock => "SentryBlock",
            ErrorKind::RateLimitError => "RateLimitError",
            ErrorKind::AccountSuspended => "AccountSuspended",
            ErrorKind::AccountContactPointRequired => "AccountContactPointRequired",
            ErrorKind::AccountEditError => "AccountEditError",
            ErrorKind::DirectMessageRequestsDisabled => "DirectMessageRequestsDisabled",
            ErrorKind::PrivateAccount => "PrivateAccount",
            ErrorKind::InvalidTargetUser => "InvalidTargetUser",
            ErrorKind::InvalidMediaId => "InvalidMediaId",
            ErrorKind::MediaUnavailable => "MediaUnavailable",
            ErrorKind::UserNotFound => "UserNotFound",
            ErrorKind::VideoTooLongException => "VideoTooLongException",
            ErrorKind::ProxyAddressIsBlocked => "ProxyAddressIsBlocked",
            ErrorKind::UnknownError => "UnknownError",
            ErrorKind::DirectThreadNotFound => "DirectThreadNotFound",
            ErrorKind::MqttNotConnected => "MQTTNotConnected",
        }
    }
}

impl From<reqwest::Error> for IgError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            IgError::new(
                ErrorKind::ClientRequestTimeout,
                format!("Request timed out: {e}"),
            )
        } else if e.is_connect() {
            IgError::new(
                ErrorKind::ClientConnectionError,
                format!("Connection error: {e}"),
            )
        } else {
            IgError::new(ErrorKind::ClientError, format!("Request failed: {e}"))
        }
    }
}

impl From<serde_json::Error> for IgError {
    fn from(e: serde_json::Error) -> Self {
        IgError::new(
            ErrorKind::ClientJSONDecodeError,
            format!("JSON decode error: {e}"),
        )
    }
}

impl From<rustls::Error> for IgError {
    fn from(e: rustls::Error) -> Self {
        IgError::new(ErrorKind::ClientConnectionError, format!("TLS error: {e}"))
    }
}

impl From<std::io::Error> for IgError {
    fn from(e: std::io::Error) -> Self {
        IgError::new(ErrorKind::ClientConnectionError, format!("IO error: {e}"))
    }
}

/// Convenience alias so callers can use `Result<T, IgError>`.
pub type Result<T> = std::result::Result<T, IgError>;
