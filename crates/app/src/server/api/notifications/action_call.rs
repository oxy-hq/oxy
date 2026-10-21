//! A notification action that runs an app function, as the person who
//! tapped it.
//!
//! Not a second way to run a function: the call goes through the app's own
//! function route (the runtime in `oxy-api-custom-apps`, reached through
//! `FunctionRuntime`) exactly
//! as `POST /customer-apps/<org>/<app>/fn/<name>` would, on the caller's own
//! credential, so authentication, app access, the environment gate, the
//! function's capability gates, its rate limit, the invocation row and its
//! logs all apply. The route answers an SSE stream; this reads its terminal
//! frame back into JSON.
//!
//! An idempotency key per (notification, action) makes a double tap run the
//! function once: the second answers with the first's result.

use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::server::api::middlewares::workspace_context::PreaggCacheCtx;
use oxy_app_core::function_seam::{FunctionRequest, FunctionRuntime};

/// The most of a function's answer read back. Its result is JSON for a
/// client, not a download.
const MAX_ANSWER_BYTES: usize = 8 * 1024 * 1024;

/// One function action, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCall {
    pub org_slug: String,
    pub app_slug: String,
    pub function: String,
    pub notification_id: Uuid,
    pub action: String,
    /// The notification's app-relative link, as the app gave it.
    pub link: Option<String>,
}

/// How the function answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// It ran and answered 2xx: its JSON, `null` for none.
    Done(Value),
    /// The function route refused before running it: its status and body.
    Refused(StatusCode, Value),
    /// It ran and failed, or answered with a non-2xx status.
    Failed(String),
}

impl FunctionCall {
    /// What the function receives as its request body.
    pub fn body(&self) -> Bytes {
        Bytes::from(
            json!({
                "notificationId": self.notification_id,
                "action": self.action,
                "link": self.link,
            })
            .to_string(),
        )
    }

    pub fn idempotency_key(&self) -> String {
        format!(
            "notification-action:{}:{}",
            self.notification_id, self.action
        )
    }

    pub fn uri(&self) -> String {
        format!(
            "/customer-apps/{}/{}/fn/{}",
            self.org_slug, self.app_slug, self.function
        )
    }

    /// The caller's headers — their credential, host and origin — for a JSON
    /// POST of [`Self::body`]. Production only: the environment header is
    /// dropped, and the host already passed the environment guard.
    pub fn headers(&self, caller: &HeaderMap, body_len: usize) -> HeaderMap {
        let mut headers = caller.clone();
        headers.remove(oxy_app_core::custom_app_env_request::ENV_HEADER);
        headers.remove(header::CONTENT_LENGTH);
        headers.remove(header::TRANSFER_ENCODING);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(body_len));
        if let Ok(key) = HeaderValue::from_str(&self.idempotency_key()) {
            headers.insert("idempotency-key", key);
        }
        headers
    }
}

/// Run `call` through the function route as the caller.
pub async fn invoke(
    call: &FunctionCall,
    caller: &HeaderMap,
    preagg: PreaggCacheCtx,
    runtime: &dyn FunctionRuntime,
) -> Outcome {
    let body = call.body();
    let headers = call.headers(caller, body.len());
    let Ok(uri) = call.uri().parse() else {
        return Outcome::Failed("the function's address is not a valid path".into());
    };
    let response = runtime
        .handle(FunctionRequest {
            org_slug: &call.org_slug,
            app_slug: &call.app_slug,
            function_name: &call.function,
            method: Method::POST,
            uri,
            headers,
            body,
            preagg,
        })
        .await;
    let status = response.status();
    let bytes = match axum::body::to_bytes(response.into_body(), MAX_ANSWER_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return Outcome::Failed("the function's answer was too large to read".into()),
    };
    if !status.is_success() {
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        return Outcome::Refused(status, body);
    }
    read_stream(&String::from_utf8_lossy(&bytes))
}

/// The terminal frames of the function route's SSE answer: `data` (the
/// result), `done` (the status the function answered) or `error`. `log`
/// frames are skipped.
pub fn read_stream(text: &str) -> Outcome {
    let mut result = Value::Null;
    for frame in text.split("\n\n") {
        let mut event = "";
        let mut data = String::new();
        for line in frame.lines() {
            if let Some(name) = line.strip_prefix("event: ") {
                event = name.trim();
            } else if let Some(d) = line.strip_prefix("data: ") {
                data.push_str(d);
            }
        }
        let value: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        match event {
            // A function that answered with no body frames it as `""`.
            "data" if value.as_str() == Some("") => result = Value::Null,
            "data" => result = value,
            "error" => {
                let message = value["message"].as_str().unwrap_or("the function failed");
                return Outcome::Failed(message.to_string());
            }
            "done" => {
                let status = value["status"].as_u64().unwrap_or(200);
                if !(200..300).contains(&status) {
                    return Outcome::Failed(refusal_message(status, &result));
                }
                return Outcome::Done(result);
            }
            _ => {}
        }
    }
    Outcome::Failed("the function's answer ended before it finished".into())
}

/// What a function said when it answered non-2xx: its `message` or `error`
/// when it gave one.
fn refusal_message(status: u64, body: &Value) -> String {
    let said = ["message", "error"]
        .iter()
        .find_map(|k| body[*k].as_str())
        .map(|s| s.chars().take(200).collect::<String>());
    match said {
        Some(said) => format!("the function answered {status}: {said}"),
        None => format!("the function answered {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call() -> FunctionCall {
        FunctionCall {
            org_slug: "acme".into(),
            app_slug: "store-ops".into(),
            function: "ack-alarm".into(),
            notification_id: Uuid::nil(),
            action: "ack".into(),
            link: Some("/alarms/7".into()),
        }
    }

    #[test]
    fn the_function_gets_the_notification_the_action_and_the_link() {
        let body: Value = serde_json::from_slice(&call().body()).unwrap();
        assert_eq!(
            body,
            json!({ "notificationId": Uuid::nil(), "action": "ack", "link": "/alarms/7" })
        );
        assert_eq!(call().uri(), "/customer-apps/acme/store-ops/fn/ack-alarm");
    }

    #[test]
    fn the_callers_credential_goes_and_the_environment_header_does_not() {
        let mut caller = HeaderMap::new();
        caller.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer t"));
        caller.insert("x-oxy-app-env", HeaderValue::from_static("staging"));
        caller.insert("idempotency-key", HeaderValue::from_static("theirs"));
        caller.insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        let h = call().headers(&caller, 42);
        assert_eq!(h[header::AUTHORIZATION], "Bearer t");
        assert!(h.get("x-oxy-app-env").is_none());
        assert_eq!(
            h["idempotency-key"],
            format!("notification-action:{}:ack", Uuid::nil()).as_str()
        );
        assert_eq!(h[header::CONTENT_LENGTH], "42");
        assert_eq!(h[header::CONTENT_TYPE], "application/json");
    }

    #[test]
    fn the_stream_reads_back_into_a_result_or_a_failure() {
        let ok = "event: log\ndata: {\"level\":\"info\",\"message\":\"hi\"}\n\n\
                  event: data\ndata: {\"acked\":true}\n\n\
                  event: done\ndata: {\"status\":200}\n\n";
        assert_eq!(read_stream(ok), Outcome::Done(json!({ "acked": true })));
        let refused = "event: data\ndata: {\"error\":\"already acked\"}\n\n\
                       event: done\ndata: {\"status\":409}\n\n";
        assert_eq!(
            read_stream(refused),
            Outcome::Failed("the function answered 409: already acked".into())
        );
        let thrown = "event: error\ndata: {\"error\":\"error\",\"message\":\"boom\"}\n\n";
        assert_eq!(read_stream(thrown), Outcome::Failed("boom".into()));
        assert!(matches!(read_stream(""), Outcome::Failed(_)));
        let nothing = "event: data\ndata: null\n\nevent: done\ndata: {\"status\":204}\n\n";
        assert_eq!(read_stream(nothing), Outcome::Done(Value::Null));
    }
}
