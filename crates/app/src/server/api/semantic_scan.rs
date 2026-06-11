//! Postgres-backed FS materialisers for callers that walk a directory
//! / file path: airlayer (semantic scan) and the metric-monitoring
//! crate (`.monitor.yml`).
//!
//! `airlayer` (and the local semantic engine on top of it) consumes a
//! filesystem directory of `.view.yml` + `.topic.yml` files and parses
//! them itself. We don't want to fork that loader to take Postgres
//! rows directly — too much surface and the loader semantics are
//! intentionally FS-coupled (glob ordering, dialect-aware shims, etc.).
//!
//! Instead, when the compile boundary is wired on, we *materialise*
//! the compiled rows into a temporary directory laid out the way
//! airlayer expects and hand THAT directory back as the scan path.
//! The tempdir lives for the duration of the request, then the
//! `TempDir` guard drops and cleans up.
//!
//! Cost: O(view + topic) YAML writes per semantic query. View YAML
//! bodies are tens of KB at most, write to tmpfs, single-digit ms on
//! a healthy disk. Cache-warming the layer across requests is a
//! follow-up — the natural shape is a per-(workspace_id, revision_id)
//! cache holding the parsed `SemanticLayer` directly, skipping the
//! tempdir entirely.

use std::path::PathBuf;

use tempfile::TempDir;
use uuid::Uuid;

use crate::server::api::compiled_reader::{
    self, CompiledArtifact,
};

/// Wraps a tempdir + its semantics scan path. Drop the wrapper to
/// clean up.
pub struct MaterialisedScan {
    _dir: TempDir,
    pub scan_path: PathBuf,
}

/// Materialise the workspace's promoted-revision semantic views +
/// topics into a tempdir laid out as airlayer expects:
///
/// ```text
/// <tmp>/semantics/views/<file_path>
/// <tmp>/semantics/topics/<file_path>
/// ```
///
/// Returns `Ok(None)` when the workspace isn't promoted, the kill
/// switch is on, or no semantic rows exist. Caller then falls through
/// to the FS scan path.
pub async fn materialise_semantic_scan(
    workspace_id: Uuid,
) -> Result<Option<MaterialisedScan>, std::io::Error> {
    if !crate::server::feature_flags::is_enabled("compile_runtime_use_postgres") {
        return Ok(None);
    }

    let views = match compiled_reader::list_semantic_views(workspace_id, None).await {
        Ok(Some(rows)) => rows,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = ?e,
                "semantic scan: views lookup failed; falling through to FS"
            );
            return Ok(None);
        }
    };
    let topics = match compiled_reader::list_semantic_topics(workspace_id, None).await {
        Ok(Some(rows)) => rows,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = ?e,
                "semantic scan: topics lookup failed; falling through to FS"
            );
            return Ok(None);
        }
    };

    if views.is_empty() && topics.is_empty() {
        return Ok(None);
    }

    let dir = TempDir::new()?;
    let scan_path = dir.path().join("semantics");
    let views_dir = scan_path.join("views");
    let topics_dir = scan_path.join("topics");
    std::fs::create_dir_all(&views_dir)?;
    std::fs::create_dir_all(&topics_dir)?;

    write_artifacts(&views, &views_dir, "view.yml").await?;
    write_artifacts(&topics, &topics_dir, "topic.yml").await?;

    tracing::debug!(
        workspace_id = %workspace_id,
        views = views.len(),
        topics = topics.len(),
        path = %scan_path.display(),
        "semantic scan: materialised from compile boundary"
    );

    Ok(Some(MaterialisedScan {
        _dir: dir,
        scan_path,
    }))
}

/// Write each artifact's `definition` JSONB to a YAML file under
/// `target_dir`, recreating the subdirectory layout from the row's
/// `file_path` so two artifacts that share a basename across
/// directories (e.g. `a/orders.view.yml` and `b/orders.view.yml` with
/// distinct in-file `name`s) don't collide.
///
/// The compile-boundary PK on `semantic_views` is `(revision_id,
/// name)`, so distinct names with the same basename DO ship as two
/// rows. A flat tempdir would silently overwrite one with the other
/// and airlayer would see only the survivor at query time — a
/// silent-data-loss bug under the "two folders, same file basename"
/// pattern.
async fn write_artifacts(
    rows: &[CompiledArtifact],
    target_dir: &std::path::Path,
    fallback_suffix: &str,
) -> Result<(), std::io::Error> {
    use std::collections::HashSet;

    // Inside `materialise_semantic_scan` we already build
    // `<tmp>/semantics/views` and `<tmp>/semantics/topics`. The
    // compile rows' `file_path` is workspace-relative (e.g.
    // `semantics/views/orders.view.yml` or
    // `team-a/orders.view.yml`). We strip the leading
    // `semantics/<kind>/` prefix when present so the result lands as
    // a child of `target_dir`; otherwise we append the row's relative
    // path verbatim. The point is: preserve every directory segment
    // BELOW the kind, so distinct rows land at distinct paths.
    let mut written: HashSet<std::path::PathBuf> = HashSet::new();
    for row in rows {
        let target = derive_target_path(target_dir, &row.file_path, &row.name, fallback_suffix);
        if !written.insert(target.clone()) {
            tracing::warn!(
                file_path = %row.file_path,
                target = %target.display(),
                "semantic scan: two rows materialise to the same tempdir path — second write overwrites the first"
            );
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = resolve_body(row).await.map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("resolve body for semantic row {}: {e}", row.name),
            )
        })?;
        std::fs::write(target, body)?;
    }
    Ok(())
}

/// Resolve a row's canonical body. Prefer the S3 blob when
/// `compiled_sql_blob_key` is set AND the bucket is configured; on
/// any S3 miss / transport error fall back to serializing the in-row
/// `definition` JSONB. The fall-through is what keeps semantic
/// queries working when the bucket is misconfigured or the blob
/// genuinely disappeared — Postgres always has the canonical body.
async fn resolve_body(row: &CompiledArtifact) -> Result<Vec<u8>, String> {
    if let Some(key) = row.compiled_sql_blob_key.as_deref() {
        match oxy_compile::blob_store::get_blob(key).await {
            Ok(Some(bytes)) => return Ok(bytes),
            Ok(None) => {
                // Bucket not configured. Treat as a normal fall-through.
            }
            Err(e) => {
                tracing::warn!(
                    key,
                    error = %e,
                    "semantic scan: blob fetch failed; falling back to in-row definition"
                );
            }
        }
    }
    serde_yaml::to_string(&row.definition)
        .map(|s| s.into_bytes())
        .map_err(|e| format!("yaml: {e}"))
}

/// Strip the `semantics/<kind>/` prefix from a workspace-relative path
/// when present (the airlayer loader already gives us the kind-rooted
/// target_dir). Falls back to a sanitised version of the row name when
/// the path has no `file_name()` (e.g. trailing slash).
fn derive_target_path(
    target_dir: &std::path::Path,
    file_path: &str,
    row_name: &str,
    fallback_suffix: &str,
) -> std::path::PathBuf {
    let trimmed = file_path
        .strip_prefix("semantics/views/")
        .or_else(|| file_path.strip_prefix("semantics/topics/"))
        .unwrap_or(file_path);
    if trimmed.is_empty() {
        return target_dir.join(format!("{}.{}", row_name, fallback_suffix));
    }
    target_dir.join(trimmed)
}

/// Tempdir + path to a materialised `.monitor.yml`. Drop the wrapper
/// to clean up.
pub struct MaterialisedMonitorConfig {
    _dir: TempDir,
    pub config_path: PathBuf,
}

/// Materialise the workspace's `.monitor.yml` from
/// `monitor_configs.definition` into a tempdir. The metric-monitoring
/// crate (and its scan-workspace entry point) accepts a file path;
/// this hands it a Postgres-sourced file. Returns `Ok(None)` when the
/// flag is off, the workspace isn't promoted, or no monitor row
/// exists — caller falls through to the FS path.
pub async fn materialise_monitor_config(
    workspace_id: Uuid,
) -> Result<Option<MaterialisedMonitorConfig>, std::io::Error> {
    if !crate::server::feature_flags::is_enabled("compile_runtime_use_postgres") {
        return Ok(None);
    }
    let definition = match compiled_reader::resolve_monitor_config(workspace_id, None).await {
        Ok(Some(d)) => d,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = ?e,
                "materialise_monitor_config: lookup failed; falling through to FS"
            );
            return Ok(None);
        }
    };
    let yaml = serde_yaml::to_string(&definition).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("re-serialise monitor config: {e}"),
        )
    })?;
    let dir = TempDir::new()?;
    let config_path = dir.path().join(".monitor.yml");
    std::fs::write(&config_path, yaml)?;
    tracing::debug!(
        workspace_id = %workspace_id,
        path = %config_path.display(),
        "materialise_monitor_config: materialised from compile boundary"
    );
    Ok(Some(MaterialisedMonitorConfig {
        _dir: dir,
        config_path,
    }))
}
