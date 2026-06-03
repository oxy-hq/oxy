//! World-model live event channel.
//!
//! One `broadcast` sender per workspace, keyed by `workspace_id` in a
//! `DashMap`. The Toast webhook + camera watcher publish to a specific
//! workspace's bus; the SSE endpoint subscribes to only its own workspace's
//! bus. Keying by workspace prevents an authenticated subscriber on one
//! workspace from receiving another workspace's order ripples / camera events
//! once the route is exposed on the multi-tenant cloud + external routers.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::sse::{KeepAlive, Sse};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use oxy::utils::create_sse_broadcast;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;
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

/// Tagged union the SSE channel actually broadcasts. Lets one channel carry
/// both order ripples and camera transitions without a second handler.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum WorldModelEvent {
    Order(OrderEvent),
    Camera(CameraStateEvent),
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

/// Workspace object inventory for the Graph-mode ribbon.
#[derive(Debug, Serialize)]
pub struct WorldModelObjects {
    /// Number of analytics agents (`*.agentic.yml`).
    pub agents: usize,
    /// Number of data apps (`*.app.yml`).
    pub apps: usize,
    /// Number of semantic views (`semantics/views/*.view.yml`).
    pub views: usize,
    /// Number of semantic topics (`semantics/topics/*.topic.yml`).
    pub topics: usize,
    /// `agents + apps + views + topics` — the single "objects" total.
    pub objects: usize,
}

/// Count files directly under `dir` whose name ends with `ext`. Missing dir or
/// a read error yields 0 — the ribbon prefers a 0 over erroring the request.
fn count_files_with_ext(dir: &std::path::Path, ext: &str) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.path()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(ext))
                })
                .count()
        })
        .unwrap_or(0)
}

/// `GET /api/{workspace_id}/world-model/objects`
///
/// Counts the agents, data apps, semantic views, and semantic topics defined in
/// the project so the world-model app's Graph ribbon can show a real inventory.
/// Mirrors the IDE's `/agents` + `/apps` lists and the `semantics/{views,topics}`
/// file scan, but lives on the world-model surface so it's reachable from the
/// external API-key router too (the IDE list endpoints are not).
///
/// A failure to enumerate any kind degrades that count to 0 rather than
/// erroring the whole request, matching the "show what we can" ribbon ethos.
pub async fn world_model_objects(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
) -> Result<Json<WorldModelObjects>, (StatusCode, String)> {
    let config_manager = &workspace_manager.config_manager;
    let agents = config_manager
        .list_analytics_agents()
        .await
        .map(|paths| paths.len())
        .unwrap_or(0);
    let apps = config_manager
        .list_apps()
        .await
        .map(|paths| paths.len())
        .unwrap_or(0);

    // Semantic views/topics have no list-all endpoint, so scan the
    // `semantics/{views,topics}` dirs the same way the CLI's
    // `list_semantic_files` does.
    let semantics = config_manager.semantics_path();
    let views = count_files_with_ext(&semantics.join("views"), ".view.yml");
    let topics = count_files_with_ext(&semantics.join("topics"), ".topic.yml");

    Ok(Json(WorldModelObjects {
        agents,
        apps,
        views,
        topics,
        objects: agents + apps + views + topics,
    }))
}
