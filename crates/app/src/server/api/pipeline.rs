//! Compile-boundary-backed Airway pipeline (`.airway.yml`) listing.
//!
//! Airway pipelines are already compiled (`walker` → `CompiledRow::Pipeline` →
//! `airway_pipelines`), but the legacy `/agentic-airway/files` lister lives in
//! the `agentic-http` crate (which can't reach `compiled_reader`) and walks the
//! filesystem, so it 502s on a stateless serve node when the ide is down. This
//! FleetOk endpoint serves the list from the boundary, falling through to the
//! filesystem in local / not-yet-promoted mode — mirroring `automation.rs`.

use axum::extract::{self, Query};
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

use crate::server::api::compiled_reader;
use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;

/// Decode a `path_b64` route param tolerantly — accept both the
/// `URL_SAFE_NO_PAD` form our listers emit and the standard-padded form the FE
/// `btoa` path historically produced. Shared by the automation + pipeline
/// single-file fetches. Returns the decoded workspace-relative path.
pub(crate) fn decode_path_b64(path_b64: &str) -> Option<String> {
    let bytes = URL_SAFE_NO_PAD.decode(path_b64).ok().or_else(|| {
        let pad = (4 - path_b64.len() % 4) % 4;
        STANDARD
            .decode(format!("{path_b64}{}", "=".repeat(pad)))
            .ok()
    })?;
    String::from_utf8(bytes).ok()
}

#[derive(Debug, Deserialize)]
pub struct ListPipelinesQuery {
    /// Active branch hint; the boundary only serves the workspace default branch.
    pub branch: Option<String>,
}

/// One pipeline file, matching the `agentic-http` `AirwayFile` shape the FE
/// already consumes: a workspace-relative `path` and its `URL_SAFE_NO_PAD`
/// base64 (`path_b64`).
#[derive(Debug, Serialize)]
pub struct PipelineFile {
    pub path: String,
    pub path_b64: String,
}

fn to_file(relative_path: String) -> PipelineFile {
    let path_b64 = URL_SAFE_NO_PAD.encode(relative_path.as_bytes());
    PipelineFile {
        path: relative_path,
        path_b64,
    }
}

/// `GET /{workspace_id}/airway-pipelines` — FleetOk. Boundary first, FS fallback.
pub async fn list_pipelines(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Query(query): Query<ListPipelinesQuery>,
) -> Result<extract::Json<Vec<PipelineFile>>, StatusCode> {
    match compiled_reader::list_pipeline_artifacts(
        workspace_manager.workspace_id,
        query.branch.as_deref(),
    )
    .await
    {
        Ok(Some(rows)) => {
            let files = rows.into_iter().map(|r| to_file(r.file_path)).collect();
            return Ok(extract::Json(files));
        }
        Ok(None) => {
            // Local / not-yet-promoted — fall through to the filesystem.
        }
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_manager.workspace_id,
                error = ?e,
                "list_pipelines compile-boundary error; falling through to FS"
            );
        }
    }

    let workspace_path = workspace_manager.config_manager.workspace_path();
    let paths = workspace_manager
        .config_manager
        .list_pipelines()
        .await
        .map_err(|e| {
            tracing::error!("Failed to list pipelines: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let files = paths
        .iter()
        .filter_map(|p| p.strip_prefix(workspace_path).ok())
        .map(|rel| to_file(rel.to_string_lossy().to_string()))
        .collect();
    Ok(extract::Json(files))
}
