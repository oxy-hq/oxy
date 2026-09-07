//! World-model live event channel.
//!
//! One `broadcast` sender per workspace, keyed by `workspace_id` in a
//! `DashMap`. The SSE endpoint subscribes to only its own workspace's bus.
//! Keying by workspace prevents an authenticated subscriber on one workspace
//! from receiving another workspace's order ripples / camera events.
//!
//! ## Why publishing goes through Postgres
//!
//! The bus is **per process**. `POST /api/webhooks/toast/orders` is `FleetOk`
//! and runs on a `serve` replica; `GET /api/{ws}/world-model/events` was
//! `IdeOnly` and proxied to the ide. Publisher and subscriber were therefore
//! never in the same process, and every order ripple was dropped on the floor —
//! not "roughly one in three", but all of them. #2816 changed the webhook's
//! answer from `401` to `202`; the event still went nowhere.
//!
//! The subscriber side is `FleetOk` now, which it could only become once the
//! feed below stopped being process-local. That is a consequence of the fix,
//! not a second one: with the table in place any replica can serve a viewer,
//! and pinning the panel to the singleton bought nothing.
//!
//! Toast is the loudest case, not the only one. Both camera producers are
//! fleet-side too: `ComplianceReport` is emitted from the edge ingest handler
//! (`POST /control/compliance-reports` → `cameras::service::compliance::
//! write_reports`), and that whole `/control` tree is merged as a plain
//! `Router`, so it carries no role declaration and `classify` falls through to
//! its `FleetOk` default. Only `HealthTransition` reached a subscriber before
//! this change, and only by accident: `alerts::spawn` is started on every pod,
//! so the ide's own copy of the loop published into the ide's own bus. So do
//! not read the camera tab having looked half-alive as evidence that cameras
//! were fine.
//!
//! So publishers no longer touch the bus directly. They append a row to
//! `world_model_events`, and every pod tails that table and fans each row onto
//! its own local bus. Fan-out is the same single path everywhere, including on
//! the publishing pod, so no pod is a special case.
//!
//! One subscriber-side overlap survives that: a viewer subscribes *before*
//! reading its backfill (so nothing published in between is lost), which means
//! the receiver can hold rows the backfill also returned. Every event therefore
//! travels the bus as a [`LiveEvent`] carrying its row id, and the stream drops
//! anything at or below the last id the backfill already sent. Without it a
//! reconnect can show one order twice and count it twice in `orders/min`.
//!
//! A table rather than `LISTEN`/`NOTIFY` on purpose. The notify path costs a
//! second permanent LISTEN connection per pod, which
//! `internal-docs/adr-postgres-as-worker-queue.md` already flags as a
//! horizontal-scale cap (§3) and a PgBouncer footgun (§1) — and it would still
//! leave a viewer who connects mid-shift with an empty panel and `orders/min`
//! uncountable. Reading rows solves all three with no new connection.

use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use axum::response::sse::{KeepAlive, Sse};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use futures::StreamExt;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
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
    /// The org location `key` resolves to through `location_external_ids`
    /// (system `toast`), when it is mapped. Optional on both sides: rows
    /// published before the operating graph carry neither, and an unmapped
    /// store is still an order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_id: Option<uuid::Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_name: Option<String>,
}

/// How long a published event stays readable. Long enough that a viewer opening
/// the dashboard mid-shift sees a populated panel; short enough that the table
/// stays small and the reaper's delete is cheap.
const RETENTION: chrono::Duration = chrono::Duration::hours(6);

/// How many past events a newly-connected subscriber receives before the live
/// stream takes over.
const BACKFILL_LIMIT: u64 = 100;

/// Tailer poll interval. Sub-second, so "LIVE" is honest, and it is one small
/// indexed range scan per pod — independent of how many viewers are connected.
const POLL: std::time::Duration = std::time::Duration::from_millis(500);

/// One event as it travels this pod's bus.
///
/// `payload` is the JSON the client receives, read straight out of the row —
/// nothing on the read path deserialises back into a typed enum it would only
/// re-serialise a moment later. `id` is the row id, and it rides along **only**
/// so a subscriber can tell a live frame apart from one its backfill already
/// covered; it is deliberately not part of the wire format.
#[derive(Clone, Debug)]
pub struct LiveEvent {
    pub id: i64,
    pub payload: Value,
}

/// Per-workspace broadcast buses. Each sender has capacity 1024 — well above
/// the chain's order velocity, so slow SSE consumers won't drop ripples in
/// normal operation. Entries are created lazily on first publish/subscribe and
/// never removed (a `Sender` is tiny, and the workspace set is bounded).
fn buses() -> &'static DashMap<Uuid, broadcast::Sender<LiveEvent>> {
    static BUSES: OnceLock<DashMap<Uuid, broadcast::Sender<LiveEvent>>> = OnceLock::new();
    BUSES.get_or_init(DashMap::new)
}

/// Get (or create) the broadcast sender for one workspace.
fn bus_for(workspace_id: Uuid) -> broadcast::Sender<LiveEvent> {
    buses()
        .entry(workspace_id)
        .or_insert_with(|| broadcast::channel::<LiveEvent>(1024).0)
        .clone()
}

/// Subscribe to one workspace's live feed on **this** pod.
///
/// Events arrive here via the tailer, so a subscriber sees what every other pod
/// published as well as what this one did — that is the whole point of routing
/// publishes through the table.
pub fn subscribe(workspace_id: Uuid) -> broadcast::Receiver<LiveEvent> {
    bus_for(workspace_id).subscribe()
}

/// Queue of events waiting to be appended to `world_model_events`.
///
/// Publishers are synchronous — the cameras crate's sink is a plain
/// `Fn(Uuid, CameraDomainEvent)`, and the Toast handler publishes on the
/// response path — so they hand off here instead of awaiting a write. Bounded:
/// if the writer ever falls behind, dropping a live-feed frame is the correct
/// failure, not stalling a webhook that Toast will retry.
fn outbox() -> tokio::sync::mpsc::Sender<(Uuid, Value)> {
    static OUTBOX: OnceLock<tokio::sync::mpsc::Sender<(Uuid, Value)>> = OnceLock::new();
    OUTBOX
        .get_or_init(|| {
            let (tx, rx) = tokio::sync::mpsc::channel(4096);
            spawn_writer(rx);
            tx
        })
        .clone()
}

/// Hand an event to the outbox. Never blocks, never fails the caller.
fn publish(workspace_id: Uuid, event: WorldModelEvent) {
    let value = match serde_json::to_value(&event) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "world-model: event failed to serialise; dropped");
            return;
        }
    };
    if outbox().try_send((workspace_id, value)).is_err() {
        tracing::warn!(
            workspace = %workspace_id,
            "world-model outbox full; dropping a live-feed event (the dashboard \
             will be missing a frame, nothing durable is lost)"
        );
    }
}

/// Webhook-side entry point — called by the Toast receiver after it has
/// validated the payload, scoped to the webhook's workspace.
pub fn publish_order(workspace_id: Uuid, event: OrderEvent) {
    publish(workspace_id, WorldModelEvent::Order(event));
}

/// Camera-watcher entry point, scoped to the watcher's workspace.
pub fn publish_camera_event(workspace_id: Uuid, event: CameraStateEvent) {
    publish(workspace_id, WorldModelEvent::Camera(event));
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
    publish(workspace_id, event);
}

/// Drains the outbox into `world_model_events`.
///
/// Batched: under a burst this collapses many single-row inserts into one
/// statement, which is what keeps the hot path off the webhook's response time.
/// A write failure is logged and the batch dropped — the feed is disposable by
/// design, and blocking or retrying forever would turn a cosmetic outage into a
/// backpressure one.
fn spawn_writer(mut rx: tokio::sync::mpsc::Receiver<(Uuid, Value)>) {
    tokio::spawn(async move {
        let mut batch: Vec<(Uuid, Value)> = Vec::with_capacity(64);
        loop {
            let Some(first) = rx.recv().await else {
                return; // sender dropped — process is shutting down
            };
            batch.push(first);
            while let Ok(next) = rx.try_recv() {
                batch.push(next);
                if batch.len() >= 256 {
                    break;
                }
            }

            let db = match oxy::database::client::establish_connection().await {
                Ok(db) => db,
                Err(e) => {
                    tracing::warn!(error = %e, dropped = batch.len(),
                        "world-model writer: no database; dropping events");
                    batch.clear();
                    continue;
                }
            };

            let rows: Vec<entity::world_model_events::ActiveModel> = batch
                .drain(..)
                .map(
                    |(workspace_id, payload)| entity::world_model_events::ActiveModel {
                        workspace_id: sea_orm::ActiveValue::Set(workspace_id),
                        payload: sea_orm::ActiveValue::Set(payload),
                        ..Default::default()
                    },
                )
                .collect();
            let n = rows.len();
            if let Err(e) = entity::world_model_events::Entity::insert_many(rows)
                .exec(&db)
                .await
            {
                tracing::warn!(error = %e, dropped = n, "world-model writer: insert failed");
            }
        }
    });
}

/// Tails `world_model_events` and fans new rows onto this pod's buses.
///
/// Runs on **every** pod, including the one that published — that is what makes
/// the delivery path identical everywhere and keeps a subscriber from seeing an
/// event twice.
///
/// The cursor starts at the current MAX(id) rather than 0: on restart we want
/// the live feed, not a replay of the retained window into every open tab.
/// A viewer that wants history gets it from the backfill on connect instead.
pub fn spawn_world_model_tailer() {
    // One tailer per process. A second one would fan every row onto the same
    // bus again, so a subscriber would see each event as many times as this was
    // called — `api_router` calls it once today, and this makes that a property
    // of the function rather than of its one call site.
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(async move {
        let mut cursor: i64 = -1;
        loop {
            tokio::time::sleep(POLL).await;

            let Ok(db) = oxy::database::client::establish_connection().await else {
                continue; // transient; try again next tick
            };

            // First successful connection sets the high-water mark.
            if cursor < 0 {
                cursor = entity::world_model_events::Entity::find()
                    .order_by_desc(entity::world_model_events::Column::Id)
                    .one(&db)
                    .await
                    .ok()
                    .flatten()
                    .map(|m| m.id)
                    .unwrap_or(0);
                continue;
            }

            let rows = match entity::world_model_events::Entity::find()
                .filter(entity::world_model_events::Column::Id.gt(cursor))
                .order_by_asc(entity::world_model_events::Column::Id)
                .limit(1000)
                .all(&db)
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::debug!(error = %e, "world-model tailer: poll failed");
                    continue;
                }
            };

            for row in rows {
                cursor = row.id;
                // `send` errors only when nobody is subscribed on this pod,
                // which is the normal case for most pods most of the time.
                let _ = bus_for(row.workspace_id).send(LiveEvent {
                    id: row.id,
                    payload: row.payload,
                });
            }
        }
    });
}

/// Trims events past [`RETENTION`]. Cheap and idempotent, so it is safe to run
/// on every pod; whoever gets there first does the work.
pub fn spawn_world_model_reaper() {
    // Idempotent for the same reason as the tailer; duplicate reapers would only
    // race each other to delete the same rows.
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
        loop {
            tick.tick().await;
            let Ok(db) = oxy::database::client::establish_connection().await else {
                continue;
            };
            let cutoff = Utc::now() - RETENTION;
            if let Err(e) = entity::world_model_events::Entity::delete_many()
                .filter(entity::world_model_events::Column::CreatedAt.lt(cutoff))
                .exec(&db)
                .await
            {
                tracing::debug!(error = %e, "world-model reaper: delete failed");
            }
        }
    });
}

/// The most recent events for one workspace, oldest-first, for the backfill a
/// newly-connected subscriber receives before the live stream takes over.
///
/// Returns the highest row id it covered alongside them (`0` when there is no
/// history yet). That id is the boundary [`live_events`] filters against, so it
/// has to come from the same read as the rows — recomputing it later would
/// reopen the very gap it exists to close.
///
/// `pub` alongside [`live_events`], for the same test.
pub async fn recent_events(workspace_id: Uuid) -> (Vec<LiveEvent>, i64) {
    let Ok(db) = oxy::database::client::establish_connection().await else {
        return (Vec::new(), 0);
    };
    let mut rows = entity::world_model_events::Entity::find()
        .filter(entity::world_model_events::Column::WorkspaceId.eq(workspace_id))
        .order_by_desc(entity::world_model_events::Column::Id)
        .limit(BACKFILL_LIMIT)
        .all(&db)
        .await
        .unwrap_or_default();
    rows.reverse();
    // `rows` is oldest-first now, so the newest id is the last one.
    let through_id = rows.last().map(|r| r.id).unwrap_or(0);
    let events = rows
        .into_iter()
        .map(|r| LiveEvent {
            id: r.id,
            payload: r.payload,
        })
        .collect();
    (events, through_id)
}

/// The backfill, then the live feed with everything the backfill already
/// covered filtered out.
///
/// The handler subscribes before it reads the backfill, so nothing published in
/// between is lost — but that same overlap lets the receiver hold rows the
/// backfill also returned. Dropping `id <= through_id` is what keeps a viewer
/// from seeing one order twice and counting it twice in `orders/min`.
///
/// `pub` for the `world_model_cross_pod` integration test, which drives the
/// overlap directly — the handler above it needs a `WorkspaceExtractor` and a
/// live axum stack to reach, and an `Sse` frame exposes no way to read its data
/// back.
pub fn live_events(
    backfill: Vec<LiveEvent>,
    mut receiver: broadcast::Receiver<LiveEvent>,
    through_id: i64,
) -> impl futures::Stream<Item = LiveEvent> {
    async_stream::stream! {
        for event in backfill {
            yield event;
        }
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if event.id <= through_id {
                        // Published between our subscribe and our backfill read;
                        // the backfill already sent it.
                        continue;
                    }
                    yield event;
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "world-model SSE receiver stopped");
                    break;
                }
            }
        }
    }
}

/// [`live_events`] as SSE frames.
///
/// Only `payload` goes on the wire, so a client sees byte-for-byte what it saw
/// before the id started riding along. `Value::to_string` cannot fail, so unlike
/// the generic `create_sse_broadcast` there is no serialization-error branch.
fn live_stream(
    backfill: Vec<LiveEvent>,
    receiver: broadcast::Receiver<LiveEvent>,
    through_id: i64,
) -> impl futures::Stream<Item = Result<axum::response::sse::Event, axum::Error>> {
    live_events(backfill, receiver, through_id).map(|event| {
        Ok(axum::response::sse::Event::default()
            .event("message")
            .data(event.payload.to_string()))
    })
}

#[derive(Deserialize)]
pub struct WorkspacePath {
    pub workspace_id: Uuid,
}

/// `GET /api/{workspace_id}/world-model/events`
///
/// SSE stream that fans the broadcast bus out to one browser tab, after a
/// backfill of recent history. Uses [`live_stream`] rather than the generic
/// `create_sse_broadcast` because the two halves overlap and the duplicates
/// have to be filtered; the frames on the wire are identical either way.
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
    // Subscribe BEFORE reading history so an event published between the two is
    // delivered by the stream rather than lost in the gap. That trades the gap
    // for an overlap, which `live_stream` closes with `through_id`.
    let receiver = subscribe(workspace_id);
    let (backfill, through_id) = recent_events(workspace_id).await;
    Sse::new(live_stream(backfill, receiver, through_id))
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
