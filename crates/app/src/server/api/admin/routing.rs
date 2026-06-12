//! `/admin/routing-manifest` — returns the IdeOnly classification table plus
//! the current process's role. Auth-gated by the same Owner / Global Admin
//! guard as the rest of `/admin/*` so the table isn't world-readable.

use axum::{Json, Router, routing::get};

use crate::server::router::AppState;

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/routing-manifest", get(routing_manifest))
}

async fn routing_manifest() -> Json<serde_json::Value> {
    let role = crate::server::role_manifest::current_process_role();
    let entries: Vec<serde_json::Value> = crate::server::role_manifest::dump_manifest()
        .into_iter()
        .map(|(method, path, role_kind)| {
            serde_json::json!({
                "method": method,
                "path": path,
                "role": role_kind,
            })
        })
        .collect();
    Json(serde_json::json!({
        "process_role": role.as_str(),
        "ide_only": entries,
    }))
}
