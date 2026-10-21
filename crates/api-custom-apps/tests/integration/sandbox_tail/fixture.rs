//! What the tail tests run on: the listener installed against the test
//! database, an app whose two sandboxes run a function that prints, a raw
//! `LISTEN` on the tail's channel, and a reader for the stream.

use std::time::Duration;

use agentic_runtime::router::{TlsVerification, connect_listener};
use axum::body::{Body, BodyDataStream};
use axum::http::{Request, StatusCode};
use entity::apps;
use futures::StreamExt;
use oxy::database::client::{SslMode, listener_factory_from_env, listener_ssl_mode_from_env};
use oxy_api_custom_apps::custom_apps_publish::{PublishTarget, publish_to};
use oxy_api_custom_apps::custom_apps_tail::{event, listener};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use crate::custom_app_functions_fixture::{FnCall, FunctionSpec, Tenant, call_function_with};
use crate::sandbox_publish::{app_with_two_sandboxes, bundle, input, sandbox};

pub(super) const APP: &str = "sbx-tail";

/// Two lines, at two levels, then an answer.
const CHATTY_JS: &str = r#"
export default async () => {
  console.log("one");
  console.warn("two");
  return Response.json({ ok: true });
};
"#;

fn chatty() -> FunctionSpec {
    FunctionSpec {
        name: "chatty",
        manifest: json!({ "route": true }),
        js: CHATTY_JS,
    }
}

fn verification() -> TlsVerification {
    match listener_ssl_mode_from_env().expect("the pool's ssl mode") {
        SslMode::Require => TlsVerification::RequireNoVerify,
        SslMode::VerifyFull => TlsVerification::VerifyFull,
    }
}

/// What `router::entry` does at boot, against the test database.
pub(crate) fn install_listener() {
    let factory = listener_factory_from_env().expect("the listener's connection factory");
    listener::install(factory, verification(), CancellationToken::new());
}

/// The app with `dev-a1` and `dev-b2`, each serving a build of `chatty`,
/// and the guest made staff.
pub(super) async fn tailed_app(t: &Tenant) -> apps::Model {
    let app = app_with_two_sandboxes(t, APP).await;
    for handle in ["a1", "b2"] {
        let tarball = bundle(APP, &[chatty()], json!({}), &[]);
        let build = format!("tail-{handle}");
        publish_to(
            input(t, APP, &build, tarball),
            PublishTarget::Sandbox(sandbox(handle)),
        )
        .await
        .expect("publish to the sandbox");
    }
    app
}

/// `chatty` called in `environment` by the guest.
pub(super) async fn call(t: &Tenant, environment: &str) -> FnCall {
    let headers = [
        ("authorization", "Bearer t"),
        ("x-oxy-app-env", environment),
    ];
    let call = call_function_with(&t.org_slug, APP, "chatty", json!({}), &headers).await;
    assert_eq!(call.status, StatusCode::OK, "{environment}: {}", call.raw);
    call
}

/// The invocation a call ran, as its response names it.
pub(super) fn invocation_of(call: &FnCall) -> String {
    call.headers["x-oxy-invocation-id"]
        .to_str()
        .expect("an invocation id")
        .to_string()
}

/// Every payload `NOTIFY`d on the tail's channel, heard on a connection of
/// the test's own. Keep the client alive while listening.
pub(super) async fn raw_listen() -> (tokio_postgres::Client, mpsc::UnboundedReceiver<String>) {
    let factory = listener_factory_from_env().expect("factory");
    let (client, mut connection) = connect_listener(&factory, verification())
        .await
        .expect("connect");
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut messages = futures::stream::poll_fn(move |cx| connection.poll_message(cx));
        while let Some(Ok(message)) = messages.next().await {
            if let tokio_postgres::AsyncMessage::Notification(n) = message {
                let _ = tx.send(n.payload().to_string());
            }
        }
    });
    client
        .batch_execute(&format!("LISTEN {}", event::NOTIFY_CHANNEL))
        .await
        .expect("listen");
    (client, rx)
}

/// One SSE event.
#[derive(Debug, Clone)]
pub(super) struct Sse {
    pub(super) event: String,
    pub(super) id: Option<String>,
    pub(super) data: Value,
}

/// A tail stream, read as it arrives.
pub(super) struct Tail {
    body: BodyDataStream,
    buffer: String,
}

/// `GET <uri>` on the flat `/api` tree as `oxy-app` mounts it. A refusal
/// comes back as its status and JSON body.
pub(super) async fn open(uri: &str, headers: &[(&str, &str)]) -> Result<Tail, (StatusCode, Value)> {
    let mut request = Request::builder().method("GET").uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let router = crate::custom_app_functions_fixture::flat_api_router();
    let response = router
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("oneshot");
    let status = response.status();
    if status != StatusCode::OK {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read the refusal");
        return Err((
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        ));
    }
    let headers = response.headers();
    assert_eq!(headers["content-type"], "text/event-stream");
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(headers["x-accel-buffering"], "no");
    Ok(Tail {
        body: response.into_body().into_data_stream(),
        buffer: String::new(),
    })
}

impl Tail {
    /// The next event; keep-alive comments are skipped. Fails the test
    /// after 20 s or when the stream ends.
    pub(super) async fn next(&mut self) -> Sse {
        tokio::time::timeout(Duration::from_secs(20), self.read())
            .await
            .expect("an event within 20 s")
    }

    async fn read(&mut self) -> Sse {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..end + 2).collect();
                if let Some(sse) = parse(&block) {
                    return sse;
                }
                continue;
            }
            let chunk = self
                .body
                .next()
                .await
                .expect("the stream ended")
                .expect("a chunk");
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }

    /// Whether the stream closes with nothing more in it.
    pub(super) async fn ended(&mut self) -> bool {
        let rest = tokio::time::timeout(Duration::from_secs(5), self.body.next())
            .await
            .expect("the stream closes");
        rest.is_none() && self.buffer.trim().is_empty()
    }

    /// Every event up to and including the first named `event`.
    pub(super) async fn until(&mut self, event: &str) -> Vec<Sse> {
        let mut read = Vec::new();
        loop {
            let sse = self.next().await;
            let last = sse.event == event;
            read.push(sse);
            if last {
                return read;
            }
        }
    }
}

fn parse(block: &str) -> Option<Sse> {
    let field = |name: &str| {
        block
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .map(|value| value.trim_start().to_string())
    };
    Some(Sse {
        event: field("event:")?,
        id: field("id:"),
        data: serde_json::from_str(&field("data:")?).ok()?,
    })
}
