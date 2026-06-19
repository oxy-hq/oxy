//! World-model live event channel.
//!
//! One `broadcast` sender per workspace, keyed by `workspace_id` in a
//! `DashMap`. The Toast webhook + camera watcher publish to a specific
//! workspace's bus; the SSE endpoint subscribes to only its own workspace's
//! bus. Keying by workspace prevents an authenticated subscriber on one
//! workspace from receiving another workspace's order ripples / camera events
//! once the route is exposed on the multi-tenant cloud + external routers.

use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::sse::{KeepAlive, Sse};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use futures::StreamExt;
use oxy::utils::create_sse_broadcast;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::broadcast;
use uuid::Uuid;

use agentic_core::tools::ToolDef;
use agentic_llm::{AnthropicProvider, Chunk, LlmProvider, ThinkingConfig};
use agentic_pipeline::platform::PlatformContext;

use crate::server::router::WorkspaceExtractor;

/// Camera state-change event (from the UniFi watcher). Surfaces in the
/// CAMERAS tab of LIVE EVENTS — independent of `OrderEvent` so the FE can
/// distinguish them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraStateEvent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub camera_id: String,
    pub host_name: String,
    pub status: String,
}

/// Compliance report ingested from the edge worker. `type` is
/// `compliance_violation` when the verdict is non-compliant, else
/// `compliance_report` — alert on the former, tick a live detection
/// counter on the latter.
#[derive(Debug, Clone, Serialize)]
pub struct ComplianceEvent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub camera_id: String,
    pub report_id: String,
    pub violation: bool,
    pub missing_items: Vec<String>,
    pub confidence: Option<f64>,
    pub segment_start: DateTime<Utc>,
    pub segment_end: DateTime<Utc>,
    pub ts: DateTime<Utc>,
}

/// Camera health transition from the alerter tick (`ok`/`degraded`/`stale`).
#[derive(Debug, Clone, Serialize)]
pub struct CameraHealthEvent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub camera_id: String,
    pub from: String,
    pub to: String,
    pub reason: String,
    pub ts: DateTime<Utc>,
}

/// Tagged union the SSE channel actually broadcasts. Lets one channel carry
/// both order ripples and camera transitions without a second handler.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum WorldModelEvent {
    Order(OrderEvent),
    Camera(CameraStateEvent),
    Compliance(ComplianceEvent),
    Health(CameraHealthEvent),
}

/// One ripple's worth of data — what the world-model app needs to draw a
/// concentric ring + append a row to LIVE EVENTS. Mirrors the FE union arm
/// in `web-app/src/pages/world-model/types.ts` (`order_ripple`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OrderEvent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub key: String,
    pub store_name: Option<String>,
    pub amount: f64,
    pub order_id: String,
    pub ts: DateTime<Utc>,
}

/// Per-workspace broadcast buses. Each sender has capacity 1024 — well above
/// the chain's order velocity, so slow SSE consumers won't drop ripples in
/// normal operation. Entries are created lazily on first publish/subscribe and
/// never removed (a `Sender` is tiny, and the workspace set is bounded).
fn buses() -> &'static DashMap<Uuid, broadcast::Sender<WorldModelEvent>> {
    static BUSES: OnceLock<DashMap<Uuid, broadcast::Sender<WorldModelEvent>>> = OnceLock::new();
    BUSES.get_or_init(DashMap::new)
}

/// Get (or create) the broadcast sender for one workspace.
fn bus_for(workspace_id: Uuid) -> broadcast::Sender<WorldModelEvent> {
    buses()
        .entry(workspace_id)
        .or_insert_with(|| broadcast::channel::<WorldModelEvent>(1024).0)
        .clone()
}

/// Webhook-side entry point — called by the Toast receiver after it has
/// validated the payload, scoped to the webhook's workspace.
pub fn publish_order(workspace_id: Uuid, event: OrderEvent) {
    // `send` returns `Err` only when there are zero subscribers, which is
    // expected pre-page-load. Treat as a no-op.
    let _ = bus_for(workspace_id).send(WorldModelEvent::Order(event));
}

/// Camera-watcher entry point, scoped to the watcher's workspace.
pub fn publish_camera_event(workspace_id: Uuid, event: CameraStateEvent) {
    let _ = bus_for(workspace_id).send(WorldModelEvent::Camera(event));
}

/// Bridges `oxy_cameras` domain events onto the bus. Registered once at
/// startup (entry.rs) via the cameras crate's `service::events` sink —
/// that crate can't depend on this one.
pub fn publish_camera_domain_event(
    workspace_id: Uuid,
    event: oxy_cameras::service::events::CameraDomainEvent,
) {
    use oxy_cameras::service::events::CameraDomainEvent;
    let event = match event {
        CameraDomainEvent::ComplianceReport {
            camera_id,
            report_id,
            violation,
            missing_items,
            confidence,
            segment_start,
            segment_end,
        } => WorldModelEvent::Compliance(ComplianceEvent {
            kind: if violation {
                "compliance_violation"
            } else {
                "compliance_report"
            },
            camera_id: camera_id.to_string(),
            report_id: report_id.to_string(),
            violation,
            missing_items,
            confidence,
            segment_start,
            segment_end,
            ts: Utc::now(),
        }),
        CameraDomainEvent::HealthTransition {
            camera_id,
            from,
            to,
            reason,
        } => WorldModelEvent::Health(CameraHealthEvent {
            kind: "camera_health",
            camera_id: camera_id.to_string(),
            from,
            to,
            reason,
            ts: Utc::now(),
        }),
    };
    let _ = bus_for(workspace_id).send(event);
}

#[derive(Deserialize)]
pub struct WorkspacePath {
    pub workspace_id: Uuid,
}

/// `GET /api/{workspace_id}/world-model/events`
///
/// SSE stream that fans the broadcast bus out to one browser tab. Falls
/// through to the existing `create_sse_broadcast` helper so the keep-alive
/// + serialization behaviour matches the IDE/streaming endpoints.
///
/// Authentication: standard `X-API-Key` header — also accepted as a
/// `?api_key=…` query param via the `api_key_query` middleware (browsers
/// can't attach headers to `EventSource` requests).
#[axum::debug_handler]
#[tracing::instrument(skip_all)]
pub async fn world_model_events_sse(
    _workspace: WorkspaceExtractor,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, axum::Error>>> {
    tracing::info!(workspace = %workspace_id, "world-model SSE subscriber connected");
    let receiver = bus_for(workspace_id).subscribe();
    Sse::new(create_sse_broadcast::<WorldModelEvent>(
        Vec::new(),
        receiver,
    ))
    .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

// ── LLM proxy ────────────────────────────────────────────────────────────────
//
// A thin, single-shot LLM passthrough for standalone apps (the world-model
// voice assistant) that must NOT ship a raw provider key in the browser. The
// client sends an Anthropic-style Messages payload (`system` + `messages` +
// optional `tools`); we resolve the workspace's Anthropic key server-side
// (same secret the analytics pipeline uses), call the provider for ONE turn,
// and return the raw content blocks (`text` + `tool_use`) verbatim. There is no
// tool-execution loop — the client runs the returned actions itself.
//
// The model is pinned server-side and `max_tokens` is clamped: the `X-API-Key`
// is client-visible (it ships in the standalone app), so treat this as an
// untrusted, client-exposed token. There is no per-token rate limiting here —
// the model pin + token clamp only stop callers requesting arbitrary/expensive
// models or oversized completions; they do not throttle volume.

/// Pinned, tool-capable Anthropic model configured for this workspace
/// (`config.yml` → `claude-sonnet-4-6`, `key_var: ANTHROPIC_API_KEY`). The
/// client's `model` field is ignored.
const PROXY_MODEL: &str = "claude-sonnet-4-6";
/// Hard ceiling on `max_tokens` regardless of what the client requests.
const PROXY_MAX_TOKENS_CAP: u32 = 2048;
/// Upper bound on the number of distinct tool name/description strings the
/// interner will ever leak (see [`intern`]). Far above the fixed voice
/// vocabulary (~14 tools); trips only on abuse.
const MAX_INTERNED_STRINGS: usize = 1024;

fn default_max_tokens() -> u32 {
    1024
}

/// Inbound Anthropic-style Messages request. Unknown fields (e.g. the client's
/// `model`) are ignored — the model is pinned server-side.
#[derive(Debug, Deserialize)]
pub struct LlmProxyRequest {
    #[serde(default)]
    system: String,
    /// Provider-native message turns (`{role, content}`). Forwarded verbatim.
    #[serde(default)]
    messages: Vec<Value>,
    /// Anthropic tool definitions (`{name, description, input_schema}`). Empty
    /// for a plain text completion (e.g. the spoken-summary call).
    #[serde(default)]
    tools: Vec<LlmProxyTool>,
    #[serde(default = "default_max_tokens")]
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct LlmProxyTool {
    name: String,
    #[serde(default)]
    description: String,
    input_schema: Value,
}

/// `POST /world-model/llm/messages` — proxy one LLM turn to the provider.
///
/// Auth + workspace/project context come from the external router's
/// middleware; `Extension<Arc<dyn PlatformContext>>` is injected by
/// `workspace_middleware` (cloud) / `local_context_middleware` (local), so the
/// Anthropic key is resolved per-workspace exactly like the agentic pipeline.
#[tracing::instrument(skip_all)]
pub async fn proxy_llm_messages(
    // `Option` (not a bare `Extension`): the platform context is injected by the
    // workspace/local-context middleware only once the workspace config
    // resolves. A workspace with no resolvable config never gets it, so a bare
    // extractor would reject with an opaque 500 — surface a 503 instead.
    platform: Option<Extension<Arc<dyn PlatformContext>>>,
    Json(req): Json<LlmProxyRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let Extension(platform) = platform.ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "workspace context is unavailable".to_string(),
    ))?;

    // Anthropic requires at least one message; reject early with a clear 400
    // rather than forwarding an empty array and surfacing it as a 502.
    if req.messages.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "messages must be non-empty".to_string(),
        ));
    }

    // Resolve the workspace's Anthropic key (honors the cloud secret store, not
    // just a process env var) — same path the analytics runs take.
    let api_key = platform
        .resolve_secret("ANTHROPIC_API_KEY")
        .await
        .filter(|k| !k.is_empty())
        .ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "ANTHROPIC_API_KEY is not configured for this workspace".to_string(),
        ))?;

    let max_tokens = req.max_tokens.clamp(1, PROXY_MAX_TOKENS_CAP);
    let provider = AnthropicProvider::new(api_key, PROXY_MODEL);

    // `ToolDef` requires `'static` name/description; intern each distinct string
    // once (bounded — see `intern`). A full interner (abuse via unique strings)
    // rejects the request rather than leaking unbounded memory.
    let mut tools: Vec<ToolDef> = Vec::with_capacity(req.tools.len());
    for t in &req.tools {
        let (Some(name), Some(description)) = (intern(&t.name), intern(&t.description)) else {
            return Err((
                StatusCode::BAD_REQUEST,
                "too many distinct tool definitions".to_string(),
            ));
        };
        tools.push(ToolDef {
            name,
            description,
            parameters: t.input_schema.clone(),
            // Don't enforce strict structured-output validation on arbitrary
            // client schemas — Anthropic doesn't need it for tool-use here.
            strict: false,
        });
    }

    let mut stream = provider
        .stream(
            &req.system,
            "",
            &req.messages,
            &tools,
            &ThinkingConfig::Disabled,
            None,
            Some(max_tokens),
        )
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("LLM provider error: {e}")))?;

    // Fold the chunk stream into Anthropic Messages response shape: a single
    // text block (if any) followed by the tool_use blocks. The client reads
    // `content[].type` ("text" → concat, "tool_use" → action), so order within
    // is irrelevant beyond grouping.
    let mut text = String::new();
    let mut tool_blocks: Vec<Value> = Vec::new();
    let mut usage: Option<Value> = None;
    let mut stop_reason = "end_turn";

    while let Some(chunk) = stream.next().await {
        match chunk.map_err(|e| (StatusCode::BAD_GATEWAY, format!("LLM stream error: {e}")))? {
            Chunk::Text(t) => text.push_str(&t),
            Chunk::ToolCall(tc) => {
                stop_reason = "tool_use";
                tool_blocks.push(json!({
                    "type": "tool_use",
                    "id": tc.id,
                    "name": tc.name,
                    "input": tc.input,
                }));
            }
            Chunk::Done(u) => {
                usage = Some(json!({
                    "input_tokens": u.input_tokens,
                    "output_tokens": u.output_tokens,
                }));
            }
            Chunk::ThinkingSummary(_) | Chunk::RawBlock(_) => {}
        }
    }

    let mut content: Vec<Value> = Vec::new();
    if !text.is_empty() {
        content.push(json!({ "type": "text", "text": text }));
    }
    content.extend(tool_blocks);

    Ok(Json(json!({
        "content": content,
        "stop_reason": stop_reason,
        "usage": usage,
    })))
}

/// Intern a request-supplied string into a `&'static str` for [`ToolDef`],
/// returning `None` once the interner is full.
///
/// The voice client sends a fixed, small set of tool names/descriptions, so the
/// cache leaks each distinct string at most once. The [`MAX_INTERNED_STRINGS`]
/// cap bounds total leakage: a misbehaving client that spams unique strings
/// stops growing memory once the cap is hit (the caller then 400s) instead of
/// leaking unbounded heap.
fn intern(s: &str) -> Option<&'static str> {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().expect("intern cache mutex poisoned");
    if let Some(&existing) = guard.get(s) {
        return Some(existing);
    }
    if guard.len() >= MAX_INTERNED_STRINGS {
        return None;
    }
    let leaked: &'static str = Box::leak(s.to_owned().into_boxed_str());
    guard.insert(s.to_owned(), leaked);
    Some(leaked)
}
