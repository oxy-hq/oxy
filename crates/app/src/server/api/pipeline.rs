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
    /// `source.kind` — what the UI keys source-specific surfaces on (the
    /// report-upload tab is only meaningful for a `ubereats` pipeline).
    ///
    /// Read from the compiled definition when there is one, and parsed
    /// straight out of the YAML on the filesystem fallback below, so the two
    /// backends answer the same kind for the same pipeline —
    /// `the_filesystem_fallback_reads_the_same_kind_as_the_boundary` pins
    /// that. This doc used to say the fallback returned `None`, which was
    /// true before the fallback learned to read it and is exactly the sort of
    /// claim a gating decision gets reasoned from.
    ///
    /// `None` now means only that the file could not be read or did not
    /// parse. A surface gated on this stays hidden in that case rather than
    /// appearing and then failing — and the server refuses an upload for a
    /// non-`ubereats` pipeline regardless, so a missing kind costs a hidden
    /// tab, never a wrong write.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
}

fn to_file(relative_path: String) -> PipelineFile {
    to_file_with_kind(relative_path, None)
}

fn to_file_with_kind(relative_path: String, source_kind: Option<String>) -> PipelineFile {
    let path_b64 = URL_SAFE_NO_PAD.encode(relative_path.as_bytes());
    PipelineFile {
        path: relative_path,
        path_b64,
        source_kind,
    }
}

/// `source.kind` out of a compiled pipeline definition, if it names one.
fn source_kind_of(definition: &serde_json::Value) -> Option<String> {
    definition
        .get("source")
        .and_then(|src| src.get("kind"))
        .and_then(|k| k.as_str())
        .filter(|k| !k.is_empty())
        .map(str::to_string)
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
            let files = rows
                .into_iter()
                .map(|r| {
                    let kind = source_kind_of(&r.definition);
                    to_file_with_kind(r.file_path, kind)
                })
                .collect();
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
        .filter_map(|p| {
            let rel = p.strip_prefix(workspace_path).ok()?;
            // Read the kind here too, not just on the boundary path.
            //
            // Local mode NEVER promotes a revision — `open_compiled_revision`
            // returns `None` for `LOCAL_WORKSPACE_ID` unconditionally — so a
            // `source_kind` supplied only by the boundary is absent for every
            // pipeline, forever, and any UI gated on it can never appear
            // locally. That is not "hidden until compiled", it is "hidden".
            //
            // One small YAML read per pipeline, on a path that only runs in
            // local or briefly before promotion. A file that will not parse
            // yields `None` rather than failing the listing: a broken pipeline
            // should still be visible in the sidebar so it can be fixed.
            let kind = std::fs::read_to_string(p)
                .ok()
                .and_then(|text| serde_yaml::from_str::<serde_json::Value>(&text).ok())
                .and_then(|def| source_kind_of(&def));
            Some(to_file_with_kind(rel.to_string_lossy().to_string(), kind))
        })
        .collect();
    Ok(extract::Json(files))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_kind_is_read_from_the_definition() {
        let def = serde_json::json!({
            "name": "ue",
            "source": { "kind": "ubereats", "config": { "base_path": "s3://z" } },
        });
        assert_eq!(source_kind_of(&def).as_deref(), Some("ubereats"));
    }

    /// Absent rather than empty-string: the UI gates a surface on this, and
    /// `Some("")` would read as "there is a kind, and it is nothing".
    #[test]
    fn a_definition_without_a_kind_yields_none() {
        for def in [
            serde_json::json!({ "name": "x" }),
            serde_json::json!({ "source": {} }),
            serde_json::json!({ "source": { "kind": "" } }),
        ] {
            assert_eq!(source_kind_of(&def), None, "{def}");
        }
    }

    /// `to_file` itself carries no kind — the FS listing supplies one by
    /// reading the YAML, which is what keeps a source-gated surface visible in
    /// local mode.
    ///
    /// Local NEVER promotes a revision, so a `source_kind` that only came from
    /// the compile boundary was absent for every pipeline forever, and the
    /// Reports tab could not appear locally at all. "Hidden until compiled"
    /// and "hidden" are the same thing when nothing ever compiles.
    #[test]
    fn to_file_alone_carries_no_kind() {
        assert_eq!(to_file("pipelines/x.airway.yml".into()).source_kind, None);
    }

    /// The kind the FS fallback reads must match what the boundary would
    /// report for the same file, or a surface appears in one mode and not the
    /// other.
    #[test]
    fn the_filesystem_fallback_reads_the_same_kind_as_the_boundary() {
        let yaml = "name: ue\nsource:\n  kind: ubereats\n  config:\n    base_path: s3://z\n";
        let from_fs: serde_json::Value = serde_yaml::from_str(yaml).expect("valid yaml");
        assert_eq!(source_kind_of(&from_fs).as_deref(), Some("ubereats"));

        // A pipeline that will not parse stays listed, without a kind — a
        // broken file should still be visible in the sidebar so it can be
        // fixed, rather than vanishing.
        assert!(serde_yaml::from_str::<serde_json::Value>("name: [unclosed").is_err());
    }
}
