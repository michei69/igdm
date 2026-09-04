//! instagrapi — a safe-Rust Instagram Private API client.
//!
//! A port of the subset of [instagrapi](https://github.com/subzeroid/instagrapi)
//! needed for direct messaging: private request layer, password/2FA/challenge
//! login, session persistence, direct inbox/threads/messages, reactions,
//! seen/typing, photo/video sending and the MQTToT realtime connection.
//!
//! The entire crate is `forbid(unsafe_code)`.

#![forbid(unsafe_code)]

pub mod auth;
pub mod client;
pub mod config;
pub mod direct;
pub mod error;
pub mod extract;
pub mod media;
pub mod realtime;
pub mod types;
pub mod utils;

pub use client::{Body, Client};
pub use error::{ErrorKind, IgError, Result};
pub use types::*;
