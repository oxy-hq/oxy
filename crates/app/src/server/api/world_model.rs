//! World-model live event channel.
//!
//! One `broadcast` sender per workspace, keyed by `workspace_id` in a
//! `DashMap`. The Toast webhook + camera watcher publish to a specific
//! workspace's bus; the SSE endpoint subscribes to only its own workspace's
//! bus. Keying by workspace prevents an authenticated subscriber on one
//! workspace from receiving another workspace's order ripples / camera events
//! once the route is exposed on the multi-tenant cloud + external routers.

use axum::extract::Path;
use axum::response::sse::{KeepAlive, Sse};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use oxy::utils::create_sse_broadcast;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::sync::broadcast;
use uuid::Uuid;

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
