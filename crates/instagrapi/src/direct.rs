//! Direct messaging endpoints ported from `instagrapi.mixins.direct`.

use serde_json::{json, Map, Value};

use crate::client::{Body, Client, Req};
use crate::error::{IgError, Result};
use crate::extract::{extract_direct_message, extract_direct_thread, extract_user_short};
use crate::types::{DirectMessage, DirectThread, UserShort};
use crate::utils::{dumps, generate_mutation_token, generate_uuid};

impl Client {
    fn tracking_params(&self) -> Map<String, Value> {
        let mut params = Map::new();
        params.insert("eb_device_id".to_string(), json!("0"));
        params.insert(
            "igd_request_log_tracking_id".to_string(),
            json!(generate_uuid()),
        );
        params
    }

    /// Shared inbox-page extraction: threads + next cursor from a raw
    /// `inbox` object of `direct_v2/inbox/` (or `pending_inbox/`).
    fn extract_inbox(inbox: &Value) -> (Vec<DirectThread>, Option<String>) {
        let threads = inbox
            .get("threads")
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().map(extract_direct_thread).collect())
            .unwrap_or_default();
        let cursor = inbox
            .get("oldest_cursor")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());
        (threads, cursor)
    }

    /// `direct_threads_chunk` — one page of the inbox.
    pub async fn direct_threads_chunk(
        &self,
        cursor: Option<&str>,
        thread_message_limit: Option<i64>,
    ) -> Result<(Vec<DirectThread>, Option<String>)> {
        let inbox = self.direct_inbox_raw(cursor, thread_message_limit).await?;
        Ok(Self::extract_inbox(&inbox))
    }

    /// One raw inbox page (`inbox` object), for callers that keep the
    /// per-thread payloads (nickname/avatar metadata, copy-raw-data).
    pub async fn direct_inbox_raw(
        &self,
        cursor: Option<&str>,
        thread_message_limit: Option<i64>,
    ) -> Result<Value> {
        let push_disabled = self.state().await.push_disabled;
        let mut params = self.tracking_params();
        params.insert("visual_message_return_type".to_string(), json!("unseen"));
        params.insert("thread_message_limit".to_string(), json!("10"));
        params.insert("persistentBadging".to_string(), json!("true"));
        params.insert("limit".to_string(), json!("20"));
        params.insert("is_prefetching".to_string(), json!("false"));
        params.insert("fetch_reason".to_string(), json!("initial_snapshot"));
        params.insert("include_old_mrs".to_string(), json!("false"));
        params.insert("no_pending_badge".to_string(), json!("true"));
        params.insert(
            "push_disabled".to_string(),
            json!(if push_disabled { "true" } else { "false" }),
        );
        if let Some(limit) = thread_message_limit {
            params.insert("thread_message_limit".to_string(), json!(limit.to_string()));
        }
        if let Some(cursor) = cursor {
            params.insert("cursor".to_string(), json!(cursor));
            params.insert("direction".to_string(), json!("older"));
            params.insert("fetch_reason".to_string(), json!("page_scroll"));
        }
        let result = self
            .private_request(
                "direct_v2/inbox/",
                None,
                Req::signed().params(Some(&params)),
            )
            .await?;
        Ok(result.get("inbox").cloned().unwrap_or(Value::Null))
    }

    /// `direct_threads` — paginated inbox list.
    pub async fn direct_threads(&self, amount: i64) -> Result<Vec<DirectThread>> {
        let mut cursor: Option<String> = None;
        let mut threads = Vec::new();
        loop {
            let (chunk, next) = self.direct_threads_chunk(cursor.as_deref(), None).await?;
            threads.extend(chunk);
            cursor = next;
            if cursor.is_none() || (amount > 0 && threads.len() as i64 >= amount) {
                break;
            }
        }
        if amount > 0 {
            threads.truncate(amount as usize);
        }
        Ok(threads)
    }

    /// `direct_pending_chunk` — message requests.
    pub async fn direct_pending_chunk(
        &self,
        cursor: Option<&str>,
    ) -> Result<(Vec<DirectThread>, Option<String>)> {
        let request_id = self.state().await.request_id.clone();
        let mut params = Map::new();
        params.insert("visual_message_return_type".to_string(), json!("unseen"));
        params.insert("persistentBadging".to_string(), json!("true"));
        params.insert("is_prefetching".to_string(), json!("false"));
        params.insert("request_session_id".to_string(), json!(request_id));
        if let Some(cursor) = cursor {
            params.insert("cursor".to_string(), json!(cursor));
        }
        let result = self
            .private_request(
                "direct_v2/pending_inbox/",
                None,
                Req::signed().params(Some(&params)),
            )
            .await?;
        let inbox = result.get("inbox").unwrap_or(&Value::Null);
        Ok(Self::extract_inbox(inbox))
    }

    /// `direct_pending_inbox` / `direct_requests`.
    pub async fn direct_requests(&self, amount: i64) -> Result<Vec<DirectThread>> {
        let mut cursor: Option<String> = None;
        let mut threads = Vec::new();
        loop {
            let (chunk, next) = self.direct_pending_chunk(cursor.as_deref()).await?;
            threads.extend(chunk);
            cursor = next;
            if cursor.is_none() || (amount > 0 && threads.len() as i64 >= amount) {
                break;
            }
        }
        if amount > 0 {
            threads.truncate(amount as usize);
        }
        Ok(threads)
    }

    /// `direct_pending_approve` / `direct_request_approve`.
    pub async fn direct_request_approve(&self, thread_id: &str) -> Result<bool> {
        let uuid = self.state().await.uuid.clone();
        let mut data = Map::new();
        data.insert("filter".to_string(), json!("DEFAULT"));
        data.insert("_uuid".to_string(), json!(uuid));
        let result = self
            .private_request(
                &format!("direct_v2/threads/{thread_id}/approve/"),
                Some(Body::Form(Value::Object(data))),
                Req::default(),
            )
            .await?;
        Ok(result.get("status").and_then(|v| v.as_str()).unwrap_or("") == "ok")
    }

    /// `direct_send` — text or link message.
    pub async fn direct_send(
        &self,
        text: &str,
        user_ids: &[i64],
        thread_ids: &[&str],
        reply_to: Option<(&str, Option<&str>)>,
    ) -> Result<DirectMessage> {
        let token = generate_mutation_token();
        let mut kwargs = Map::new();
        kwargs.insert("action".to_string(), json!("send_item"));
        kwargs.insert("is_x_transport_forward".to_string(), json!("false"));
        kwargs.insert("send_silently".to_string(), json!("false"));
        kwargs.insert("is_shh_mode".to_string(), json!("0"));
        kwargs.insert("send_attribution".to_string(), json!("message_button"));
        kwargs.insert("client_context".to_string(), json!(token.clone()));
        kwargs.insert("mutation_token".to_string(), json!(token.clone()));
        kwargs.insert("btt_dual_send".to_string(), json!("false"));
        kwargs.insert(
            "nav_chain".to_string(),
            json!("1qT:feed_timeline:1,1qT:feed_timeline:2,1qT:feed_timeline:3,7Az:direct_inbox:4,7Az:direct_inbox:5,5rG:direct_thread:7"),
        );
        kwargs.insert("is_ae_dual_send".to_string(), json!("false"));
        kwargs.insert("offline_threading_id".to_string(), json!(token));

        let method;
        if text.contains("http") {
            method = "link";
            let urls: Vec<String> = extract_urls(text);
            kwargs.insert("link_text".to_string(), json!(text));
            kwargs.insert(
                "link_urls".to_string(),
                json!(dumps(&Value::Array(
                    urls.iter().map(|u| json!(u)).collect()
                ))),
            );
        } else {
            method = "text";
            kwargs.insert("text".to_string(), json!(text));
        }
        if !thread_ids.is_empty() {
            kwargs.insert(
                "thread_ids".to_string(),
                json!(dumps(&Value::Array(
                    thread_ids.iter().map(|t| json!(t.to_string())).collect()
                ))),
            );
        }
        if !user_ids.is_empty() {
            kwargs.insert(
                "recipient_users".to_string(),
                json!(dumps(&Value::Array(vec![Value::Array(
                    user_ids.iter().map(|u| json!(u)).collect()
                )]))),
            );
        }
        if let Some((reply_id, reply_client_context)) = reply_to {
            kwargs.insert("replied_to_action_source".to_string(), json!("swipe"));
            kwargs.insert("replied_to_item_id".to_string(), json!(reply_id));
            if let Some(cc) = reply_client_context {
                kwargs.insert("replied_to_client_context".to_string(), json!(cc));
            }
        }
        let data = self.with_default_data(kwargs).await;

        let result = self
            .private_request(
                &format!("direct_v2/threads/broadcast/{method}/"),
                Some(Body::Form(Value::Object(data))),
                Req::default(),
            )
            .await?;
        let payload = result
            .get("payload")
            .ok_or_else(|| IgError::client_error("direct_send: missing payload"))?;
        Ok(extract_direct_message(payload))
    }

    /// `_direct_message_reaction` — send or delete an emoji reaction.
    pub async fn direct_reaction(
        &self,
        thread_id: &str,
        message_id: &str,
        emoji: &str,
        reaction_status: &str,
    ) -> Result<bool> {
        debug_assert!(reaction_status == "created" || reaction_status == "deleted");
        let token = generate_mutation_token();
        let mut data = Map::new();
        data.insert("action".to_string(), json!("send_item"));
        data.insert("is_x_transport_forward".to_string(), json!("false"));
        data.insert("send_silently".to_string(), json!("false"));
        data.insert("is_shh_mode".to_string(), json!("0"));
        data.insert("send_attribution".to_string(), json!("message_reaction"));
        data.insert("client_context".to_string(), json!(token.clone()));
        data.insert("mutation_token".to_string(), json!(token.clone()));
        data.insert("btt_dual_send".to_string(), json!("false"));
        data.insert(
            "nav_chain".to_string(),
            json!("1qT:feed_timeline:1,1qT:feed_timeline:2,1qT:feed_timeline:3,7Az:direct_inbox:4,7Az:direct_inbox:5,5rG:direct_thread:7"),
        );
        data.insert("is_ae_dual_send".to_string(), json!("false"));
        data.insert("offline_threading_id".to_string(), json!(token));
        data.insert(
            "thread_ids".to_string(),
            json!(dumps(&Value::Array(vec![json!(thread_id)]))),
        );
        data.insert("item_type".to_string(), json!("reaction"));
        data.insert("reaction_type".to_string(), json!("like"));
        data.insert("reaction_status".to_string(), json!(reaction_status));
        data.insert("node_type".to_string(), json!("item"));
        data.insert("item_id".to_string(), json!(message_id));
        data.insert("emoji".to_string(), json!(emoji));
        data.insert("reaction_action_source".to_string(), json!("double_tap"));
        let body = self.with_default_data(data).await;
        let result = self
            .private_request(
                "direct_v2/threads/broadcast/reaction/",
                Some(Body::Form(Value::Object(body))),
                Req::default(),
            )
            .await?;
        Ok(result.get("status").and_then(|v| v.as_str()).unwrap_or("") == "ok")
    }

    /// `direct_message_seen` — HTTP mark-seen.
    pub async fn direct_message_seen(&self, thread_id: &str, message_id: &str) -> Result<bool> {
        let state = self.state().await;
        let uuid = state.uuid.clone();
        drop(state);
        let token = generate_mutation_token();
        let mut data = Map::new();
        data.insert("thread_id".to_string(), json!(thread_id.to_string()));
        data.insert("action".to_string(), json!("mark_seen"));
        data.insert("client_context".to_string(), json!(token));
        data.insert("_uuid".to_string(), json!(uuid));
        data.insert("offline_threading_id".to_string(), json!(token));
        let result = self
            .private_request(
                &format!("direct_v2/threads/{thread_id}/items/{message_id}/seen/"),
                Some(Body::Form(Value::Object(data))),
                Req::default(),
            )
            .await?;
        Ok(result.get("status").and_then(|v| v.as_str()).unwrap_or("") == "ok")
    }

    /// `direct_search` — ranked recipients search.
    pub async fn direct_search(&self, query: &str) -> Result<Vec<UserShort>> {
        let mut params = Map::new();
        params.insert("max_ai_bot_results".to_string(), json!("0"));
        params.insert("max_ig_bus_results".to_string(), json!("10"));
        params.insert("mode".to_string(), json!("universal"));
        params.insert("show_threads".to_string(), json!("true"));
        params.insert("query".to_string(), json!(query));
        params.insert("max_ig_results".to_string(), json!("10"));
        params.insert("max_ibc_results".to_string(), json!("20"));
        params.insert("max_fb_results".to_string(), json!("0"));
        let result = self
            .private_request(
                "direct_v2/ranked_recipients/",
                None,
                Req::signed().params(Some(&params)),
            )
            .await?;
        let mut users = Vec::new();
        if let Some(recipients) = result.get("ranked_recipients").and_then(|r| r.as_array()) {
            for item in recipients {
                if let Some(user) = item.get("user") {
                    let username = user.get("username").and_then(|v| v.as_str()).unwrap_or("");
                    if !username.is_empty() {
                        users.push(extract_user_short(user));
                    }
                }
            }
        }
        Ok(users)
    }

    /// `direct_thread_by_participants` — raw result (thread may be absent).
    pub async fn direct_thread_by_participants(&self, user_ids: &[i64]) -> Result<Value> {
        let recipient_users = dumps(&Value::Array(user_ids.iter().map(|u| json!(u)).collect()));
        let mut params = Map::new();
        params.insert("recipient_users".to_string(), json!(recipient_users));
        params.insert("seq_id".to_string(), json!(2580572));
        params.insert("limit".to_string(), json!(20));
        self.private_request(
            "direct_v2/threads/get_by_participants/",
            None,
            Req::signed().params(Some(&params)),
        )
        .await
    }

    /// `with_default_data` + `_uuid` — the broadcast payload envelope.
    async fn with_default_data(&self, mut data: Map<String, Value>) -> Map<String, Value> {
        let state = self.state().await;
        data.insert("_uuid".to_string(), json!(state.uuid.clone()));
        data.insert(
            "device_id".to_string(),
            json!(state.android_device_id.clone()),
        );
        data
    }
}

/// `re.findall(r"(https?://[^\s]+)", text)`
fn extract_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for part in text.split_whitespace() {
        if part.starts_with("http://") || part.starts_with("https://") {
            urls.push(part.to_string());
        }
    }
    urls
}
