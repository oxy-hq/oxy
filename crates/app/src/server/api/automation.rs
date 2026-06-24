//! Compile-boundary-backed automation (a.k.a. workflow) listing.
//!
//! The customer-nav sidebar's "Automations" list must render on a stateless serve
//! replica that has no working copy. The legacy `/agentic-workflows/files`
//! lister lives in the `agentic-http` crate (which by the layering rules cannot
//! reach `compiled_reader`) and walks the filesystem, so it 502s on a serve node
//! when the ide is unreachable. This FleetOk endpoint serves the same list from
//! the compile boundary (`procedure_definitions`), falling through to the
//! filesystem in local / not-yet-promoted mode — mirroring `app::list_apps`.

use axum::extract::{self, Query};
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::compiled_reader;
use crate::server::api::middlewares::workspace_context::WorkspaceManagerExtractor;
use crate::server::api::pipeline::decode_path_b64;

#[derive(Debug, Deserialize)]
pub struct ListAutomationsQuery {
    /// Active branch hint; the boundary only serves the workspace default branch.
    pub branch: Option<String>,
}

/// One automation file, matching the `agentic-http` `AutomationFile` shape the FE
/// already consumes: a workspace-relative `path` and its `URL_SAFE_NO_PAD`
/// base64 (`path_b64`) used in the workflow route + single-file fetch. The
/// single-file fetch decodes `path_b64` tolerantly, so emitting the same
/// `URL_SAFE_NO_PAD` form the legacy lister used keeps every link round-tripping.
#[derive(Debug, Serialize)]
pub struct AutomationFileInfo {
    pub path: String,
    pub path_b64: String,
}

fn to_file(relative_path: String) -> AutomationFileInfo {
    let path_b64 = URL_SAFE_NO_PAD.encode(relative_path.as_bytes());
    AutomationFileInfo {
        path: relative_path,
        path_b64,
    }
}

/// `GET /{workspace_id}/procedures` — FleetOk. Serves the automation list from the
/// compile boundary, falling through to the filesystem only in local /
/// not-yet-promoted mode (the boundary is best-effort, exactly like `list_apps`).
pub async fn list_automations(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Query(query): Query<ListAutomationsQuery>,
) -> Result<extract::Json<Vec<AutomationFileInfo>>, StatusCode> {
    match compiled_reader::list_automation_artifacts(
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
                "list_automations compile-boundary error; falling through to FS"
            );
        }
    }

    let workspace_path = workspace_manager.config_manager.workspace_path();
    let paths = workspace_manager
        .config_manager
        .list_workflows()
        .await
        .map_err(|e| {
            tracing::error!("Failed to list automations: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let files = paths
        .iter()
        .filter_map(|p| p.strip_prefix(workspace_path).ok())
        .map(|rel| to_file(rel.to_string_lossy().to_string()))
        .collect();
    Ok(extract::Json(files))
}

/// One automation's raw YAML — matches the `agentic-http` `AutomationFileContent`
/// the FE's `getFile` consumes.
#[derive(Debug, Serialize)]
pub struct AutomationContent {
    pub path: String,
    pub content: String,
}

/// `GET /{workspace_id}/procedures/{path_b64}` — FleetOk. Serves one automation's
/// YAML from the compile boundary so clicking an automation renders its diagram on
/// a stateless serve replica; falls through to the FS in local / not-promoted
/// mode. (`/agentic-workflows/files/{path_b64}` is boundary-CAPABLE but classed
/// IdeOnly, so a serve node proxies it to the dead ide and 502s.)
pub async fn get_automation(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    extract::Path((_workspace_id, path_b64)): extract::Path<(Uuid, String)>,
    Query(query): Query<ListAutomationsQuery>,
) -> Result<extract::Json<AutomationContent>, StatusCode> {
    let file_path = decode_path_b64(&path_b64).ok_or(StatusCode::BAD_REQUEST)?;

    match compiled_reader::resolve_automation(
        workspace_manager.workspace_id,
        query.branch.as_deref(),
        &file_path,
    )
    .await
    {
        Ok(Some(artifact)) => {
            // The compiled `definition` is the parsed YAML re-serialised to text:
            // semantically equal to the source file but NOT byte-identical
            // (comments dropped, key order + formatting normalised). RENDER-ONLY
            // — the FE parses this for the automation diagram. It is NOT a
            // source-of-truth read: the verbatim working-copy file is served by
            // the IdeOnly file/edit path, never from here.
            let content = serde_yaml::to_string(&artifact.definition).map_err(|e| {
                tracing::error!(file_path, "serialise compiled automation: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            return Ok(extract::Json(AutomationContent {
                path: file_path,
                content,
            }));
        }
        Ok(None) => {
            // Local / not-yet-promoted — fall through to the filesystem.
        }
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_manager.workspace_id,
                error = ?e,
                "get_automation compile-boundary error; falling through to FS"
            );
        }
    }

    let full_path = workspace_manager
        .config_manager
        .resolve_file(&file_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let content = tokio::fs::read_to_string(&full_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(extract::Json(AutomationContent {
        path: file_path,
        content,
    }))
}
