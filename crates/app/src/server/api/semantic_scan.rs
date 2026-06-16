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

use crate::server::api::compiled_reader::{self, CompiledArtifact};

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
/// Returns `Ok(None)` when the workspace isn't promoted or no semantic rows
/// exist. Caller then falls through to the FS scan path.
pub async fn materialise_semantic_scan(
    workspace_id: Uuid,
) -> Result<Option<MaterialisedScan>, std::io::Error> {
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
    // tokio::fs so the per-request materialise never blocks the reactor thread
    // (this is on the semantic-query hot path).
    tokio::fs::create_dir_all(&views_dir).await?;
    tokio::fs::create_dir_all(&topics_dir).await?;

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
            tokio::fs::create_dir_all(parent).await?;
        }
        let body = resolve_body(row).await.map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("resolve body for semantic row {}: {e}", row.name),
            )
        })?;
        tokio::fs::write(target, body).await?;
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

/// Tempdir holding a workspace's compiled context entities, laid out at their
/// workspace-relative paths so an analytics agent's `context:` globs resolve.
/// Drop to clean up.
pub struct MaterialisedContext {
    _dir: TempDir,
    pub root: PathBuf,
}

/// Materialise the workspace's promoted-revision **context** entities into a
/// tempdir at their workspace-relative `file_path`, e.g.
/// `<tmp>/semantics/views/orders.view.yml`. The analytics run resolves an
/// agent's `context:` globs (`./semantics/**/*`) against this root, which lets a
/// stateless serve replica — with no working copy — build the semantic catalog
/// and discover the databases the views reference (`datasource:`), instead of
/// globbing an absent filesystem and silently ending up with "no databases
/// configured".
///
/// Scope: semantic views + topics (the entities that carry the agent's database
/// references). Procedures and verified `.sql` are a follow-up — `list_procedures`
/// returns no body and verified queries have no compiled reader yet, so on the
/// fleet they're simply absent from context (subruns degrade; the IDE, which
/// reads the FS, is unaffected).
///
/// `Ok(None)` when the workspace isn't promoted or has no semantic rows — the
/// caller then falls through to the FS workspace path.
pub async fn materialise_agent_context(
    workspace_id: Uuid,
) -> Result<Option<MaterialisedContext>, std::io::Error> {
    let views = match compiled_reader::list_semantic_views(workspace_id, None).await {
        Ok(Some(rows)) => rows,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!(workspace_id = %workspace_id, error = ?e,
                "context materialise: views lookup failed; falling through to FS");
            return Ok(None);
        }
    };
    // Topics are optional — a workspace can ship views without topics.
    let topics = match compiled_reader::list_semantic_topics(workspace_id, None).await {
        Ok(Some(rows)) => rows,
        Ok(None) => Vec::new(),
        Err(e) => {
            tracing::warn!(workspace_id = %workspace_id, error = ?e,
                "context materialise: topics lookup failed; continuing with views only");
            Vec::new()
        }
    };
    if views.is_empty() && topics.is_empty() {
        return Ok(None);
    }

    let dir = TempDir::new()?;
    let root = dir.path().to_path_buf();
    write_context_artifacts(&views, &root).await?;
    write_context_artifacts(&topics, &root).await?;

    // info (not debug like the per-request boundary reads): this is a per-run
    // operation on the stateless fleet and the single confirmation that the
    // boundary context path — not an absent FS — served the run.
    tracing::info!(
        workspace_id = %workspace_id,
        views = views.len(),
        topics = topics.len(),
        root = %root.display(),
        "context materialise: materialised agent context from compile boundary"
    );
    Ok(Some(MaterialisedContext { _dir: dir, root }))
}

/// Write each artifact's resolved body to `<root>/<file_path>`, recreating the
/// workspace-relative directory layout so the agent's globs match. Paths that
/// are absolute or escape the root (`..`) are skipped defensively — `file_path`
/// is compiler-produced and workspace-relative, but we never trust it into a
/// `join` blindly.
async fn write_context_artifacts(
    rows: &[CompiledArtifact],
    root: &std::path::Path,
) -> Result<(), std::io::Error> {
    use futures::stream::TryStreamExt;

    // Resolve target paths up front: drop unsafe paths (absolute / `..`) and
    // dedup by target. The compile PK is (revision_id, name), so two named
    // entities can share one physical `file_path`; without deduping, the
    // concurrent writes below would race on that path (truncated / interleaved
    // output) instead of the serial version's clean last-writer-wins. Mirrors
    // the sibling `write_artifacts` collision guard.
    let mut seen = std::collections::HashSet::new();
    let targets: Vec<(std::path::PathBuf, &CompiledArtifact)> = rows
        .iter()
        .filter_map(|row| {
            let rel = std::path::Path::new(&row.file_path);
            if rel.is_absolute()
                || rel
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                tracing::warn!(file_path = %row.file_path,
                    "context materialise: skipping unsafe file_path");
                return None;
            }
            let target = root.join(rel);
            if !seen.insert(target.clone()) {
                tracing::warn!(file_path = %row.file_path,
                    "context materialise: two rows materialise to the same path — dropping duplicate");
                return None;
            }
            Some((target, row))
        })
        .collect();

    // Resolve each body (possibly an S3 GET for blob-keyed rows) + write it
    // concurrently, bounded so a large semantic layer can't open unbounded S3
    // connections at once. Doing this serially would add every row's blob-fetch
    // latency to the run's startup; in-row JSONB rows resolve without any S3.
    const MAX_CONCURRENT: usize = 16;
    futures::stream::iter(targets.into_iter().map(Ok::<_, std::io::Error>))
        .try_for_each_concurrent(MAX_CONCURRENT, |(target, row)| async move {
            // create_dir_all is idempotent, so concurrent callers writing into
            // the same `semantics/views` dir don't race destructively.
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let body = resolve_body(row).await.map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("resolve body for context row {}: {e}", row.name),
                )
            })?;
            tokio::fs::write(target, body).await
        })
        .await
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
/// workspace isn't promoted or no monitor row exists — caller falls through
/// to the FS path.
pub async fn materialise_monitor_config(
    workspace_id: Uuid,
) -> Result<Option<MaterialisedMonitorConfig>, std::io::Error> {
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
    tokio::fs::write(&config_path, yaml).await?;
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
