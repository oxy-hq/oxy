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

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use lru::LruCache;
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

/// Which semantic entity a single-read resolves.
#[derive(Clone, Copy)]
pub enum SemanticEntity {
    View,
    Topic,
}

/// Materialise the promoted semantic scan and return it together with the
/// on-disk path of ONE requested view/topic, so a Path-based parser (airlayer)
/// can parse that file WITH full scan context for cross-entity reference
/// resolution (a topic hydrates its referenced views from the same scan dir).
///
/// Returns `Ok(None)` when the workspace isn't promoted or the requested file
/// isn't a compiled row — the caller then falls through to the FS, exactly like
/// every other reader. The returned [`MaterialisedScan`] guard MUST be held
/// until parsing finishes; its tempdir is deleted on drop.
///
/// TRACKING: this materialises the WHOLE promoted scan (S3 + disk writes) per
/// call, uncached — a topic detail click pays for the full scan on every open.
/// Acceptable while view/topic detail reads are low-frequency; if they grow,
/// cache by `(workspace_id, revision_id)` the way [`materialise_agent_context`]
/// already caches the agent context, instead of re-materialising per read.
pub async fn materialise_semantic_entity(
    workspace_id: Uuid,
    entity: SemanticEntity,
    file_path: &str,
) -> Result<Option<(MaterialisedScan, PathBuf)>, std::io::Error> {
    // Resolve the requested file to its compiled row first (cheap): we need its
    // `name` for `derive_target_path`, and a miss lets us bail to FS before
    // paying for a full-scan materialise.
    let row = match entity {
        SemanticEntity::View => {
            compiled_reader::resolve_semantic_view(workspace_id, None, file_path).await
        }
        SemanticEntity::Topic => {
            compiled_reader::resolve_semantic_topic(workspace_id, None, file_path).await
        }
    };
    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = ?e,
                "semantic single-read: boundary lookup failed; falling through to FS"
            );
            return Ok(None);
        }
    };
    let Some(scan) = materialise_semantic_scan(workspace_id).await? else {
        return Ok(None);
    };
    let (subdir, suffix) = match entity {
        SemanticEntity::View => ("views", "view.yml"),
        SemanticEntity::Topic => ("topics", "topic.yml"),
    };
    // Same mapping `write_artifacts` used, so the file is exactly where the
    // materialise put it.
    let target = derive_target_path(
        &scan.scan_path.join(subdir),
        &row.file_path,
        &row.name,
        suffix,
    );
    if !target.exists() {
        tracing::warn!(
            workspace_id = %workspace_id,
            file_path,
            "semantic single-read: row resolved but materialised file missing; falling through to FS"
        );
        return Ok(None);
    }
    Ok(Some((scan, target)))
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

/// A workspace's compiled context entities materialised at their
/// workspace-relative paths so an analytics agent's `context:` globs resolve.
/// The tempdir is `Arc`-held and cached per (workspace, revision): cloning a
/// `MaterialisedContext` shares the same dir, which survives until both the
/// cache entry and every in-flight clone have dropped (Arc refcount).
#[derive(Clone)]
pub struct MaterialisedContext {
    _dir: Arc<TempDir>,
    pub root: PathBuf,
}

/// Max distinct `(workspace, revision)` contexts a process keeps materialised.
/// Each entry pins a tempdir (the full semantic layer + automations + verified
/// SQL) on local disk for as long as it's cached, so an UNBOUNDED map would
/// grow one tempdir per distinct workspace the process ever served — monotonic
/// `/tmp` + inode pressure until restart. That bites hardest on a long-lived
/// `oxy worker`, which drains a global, affinity-free queue across every tenant
/// (a serve replica is ring-hash-sharded to a workspace subset). The LRU caps
/// it: the least-recently-served workspace's tempdir drops (Arc refcount →
/// `TempDir` cleanup) once evicted. 256 covers a large hot set while bounding
/// worst-case resident disk to a few hundred small text trees.
const CONTEXT_CACHE_CAP: usize = 256;

/// Process-global, size-bounded LRU of materialised contexts, keyed by
/// `(workspace_id, revision_id)`. Lets the per-request materialise — writing
/// every view / topic / automation / verified-SQL to a fresh `/tmp` dir — run
/// ONCE per promoted revision instead of on every request. Revisions are
/// immutable, so a hit is always current; a promote yields a new `revision_id`
/// → miss → re-materialise (and the prior revision of that workspace is dropped
/// eagerly on insert, so a busy workspace never pins two tempdirs).
///
/// `std::sync::Mutex` (not DashMap): `LruCache::get` mutates recency so it needs
/// `&mut`, and the guard is only ever held for sync map ops — never across an
/// `.await` — so it can't block the runtime.
fn context_cache() -> &'static Mutex<LruCache<(Uuid, Uuid), MaterialisedContext>> {
    static CACHE: OnceLock<Mutex<LruCache<(Uuid, Uuid), MaterialisedContext>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(LruCache::new(
            NonZeroUsize::new(CONTEXT_CACHE_CAP).expect("CONTEXT_CACHE_CAP is non-zero"),
        ))
    })
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
/// Scope: semantic views + topics + automations + verified `.sql` — the complete
/// `context:` set an analytics run globs. The result is cached per
/// (workspace, revision), so the materialise runs once per promoted revision.
///
/// `Ok(None)` when the workspace isn't promoted or has no semantic rows — the
/// caller then falls through to the FS workspace path.
pub async fn materialise_agent_context(
    workspace_id: Uuid,
) -> Result<Option<MaterialisedContext>, std::io::Error> {
    // Cache key: the revision actually being served (immutable). A hit returns
    // the warm tempdir without re-reading the rows or re-writing the dir.
    let Some(revision_id) = compiled_reader::resolve_request_revision(workspace_id, None).await
    else {
        return Ok(None); // not promoted / local mode → caller falls through to FS
    };
    if let Some(hit) = context_cache()
        .lock()
        .ok()
        .and_then(|mut c| c.get(&(workspace_id, revision_id)).cloned())
    {
        return Ok(Some(hit));
    }

    // Read every context entity at the SAME revision the cache key uses. The key
    // came from `resolve_request_revision` (which walks back to the last
    // config-deserialisable revision when `current_revision_id` is broken), but
    // the `list_*` readers resolve their own revision via `open_compiled_revision`
    // — and OUTSIDE an HTTP request (a worker/TaskSpec run, with no ambient pin)
    // that reads `current_revision_id` directly, with NO walk-back. Pinning the
    // readers to `revision_id` here makes key and content the one resolved
    // revision, so a config-broken current revision can't get cached under the
    // last-known-good key. On the HTTP path the ambient pin already equals
    // `revision_id`, so this is a harmless same-value re-scope.
    let Some((views, topics, automations, verified)) =
        compiled_reader::with_pinned_revision(Some(revision_id), async {
            let views = match compiled_reader::list_semantic_views(workspace_id, None).await {
                Ok(Some(rows)) => rows,
                Ok(None) => return None,
                Err(e) => {
                    tracing::warn!(workspace_id = %workspace_id, error = ?e,
                        "context materialise: views lookup failed; falling through to FS");
                    return None;
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
            // Automations + verified `.sql` complete the agent's `context:` set: the
            // solver globs them from `base_dir`, and a matched verified query is read
            // back by path at run time (`solver/specifying`). Without these in the
            // tempdir the run still hits the real FS for them — the leak this closes.
            // Both are additive (a miss yields an empty set), so they never gate the
            // not-promoted fall-through, which stays keyed on the semantic-view lookup.
            let automations =
                match compiled_reader::list_automation_artifacts(workspace_id, None).await {
                    Ok(Some(rows)) => rows,
                    Ok(None) => Vec::new(),
                    Err(e) => {
                        tracing::warn!(workspace_id = %workspace_id, error = ?e,
                            "context materialise: automations lookup failed; continuing without them");
                        Vec::new()
                    }
                };
            let verified = match compiled_reader::list_verified_queries(workspace_id, None).await {
                Ok(Some(rows)) => rows,
                Ok(None) => Vec::new(),
                Err(e) => {
                    tracing::warn!(workspace_id = %workspace_id, error = ?e,
                        "context materialise: verified-query lookup failed; continuing without them");
                    Vec::new()
                }
            };
            if views.is_empty()
                && topics.is_empty()
                && automations.is_empty()
                && verified.is_empty()
            {
                return None;
            }
            Some((views, topics, automations, verified))
        })
        .await
    else {
        return Ok(None);
    };

    let dir = Arc::new(TempDir::new()?);
    let root = dir.path().to_path_buf();
    write_context_artifacts(&views, &root).await?;
    write_context_artifacts(&topics, &root).await?;
    write_context_artifacts(&automations, &root).await?;
    write_verified_queries(&verified, &root).await?;

    let ctx = MaterialisedContext {
        _dir: dir,
        root: root.clone(),
    };
    // Insert under the bounded LRU. First drop any PRIOR revision of this
    // workspace so a promote frees the stale tempdir eagerly (Arc refcount →
    // cleanup once in-flight clones release) instead of waiting for LRU
    // eviction; then `put`, which itself evicts the least-recently-served
    // workspace when the cache is at CONTEXT_CACHE_CAP. (Lock held for sync map
    // ops only — never across an await.)
    if let Ok(mut cache) = context_cache().lock() {
        let stale: Vec<(Uuid, Uuid)> = cache
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| k.0 == workspace_id && k.1 != revision_id)
            .collect();
        for k in stale {
            cache.pop(&k);
        }
        cache.put((workspace_id, revision_id), ctx.clone());
    }

    // info (not debug like the per-request boundary reads): the single
    // confirmation that the boundary context path — not an absent FS — served
    // this revision. Logged on the materialise (a miss), not on warm hits.
    tracing::info!(
        workspace_id = %workspace_id,
        revision_id = %revision_id,
        views = views.len(),
        topics = topics.len(),
        automations = automations.len(),
        verified = verified.len(),
        root = %root.display(),
        "context materialise: materialised + cached agent context from compile boundary"
    );
    Ok(Some(ctx))
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

/// Write each verified query's raw SQL to `<root>/<file_path>`. Unlike the YAML
/// artifacts these carry no `definition` — the body IS the `.sql` text (with its
/// `/* oxy: ... */` header) — so they bypass `resolve_body` and write verbatim.
/// Same path-safety + dedup guard as `write_context_artifacts`.
async fn write_verified_queries(
    rows: &[compiled_reader::CompiledVerifiedQuery],
    root: &std::path::Path,
) -> Result<(), std::io::Error> {
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let rel = std::path::Path::new(&row.file_path);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            tracing::warn!(file_path = %row.file_path,
                "context materialise: skipping unsafe verified-query path");
            continue;
        }
        let target = root.join(rel);
        if !seen.insert(target.clone()) {
            tracing::warn!(file_path = %row.file_path,
                "context materialise: two verified queries materialise to the same path — dropping duplicate");
            continue;
        }
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&target, &row.content).await?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::compiled_reader::CompiledVerifiedQuery;

    fn vq(file_path: &str, content: &str) -> CompiledVerifiedQuery {
        CompiledVerifiedQuery {
            file_path: file_path.to_string(),
            content_sha256: String::new(),
            content: content.to_string(),
        }
    }

    // The verified-query writer must reproduce the file at its workspace-relative
    // path (so the agent's `context:` globs match) and must NEVER let a
    // compiler-produced path escape the request tempdir.
    #[tokio::test]
    async fn write_verified_queries_writes_relative_and_blocks_escape() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        let rows = vec![
            vq("example_sql/answer.sql", "SELECT 1 /* oxy: */"),
            vq("../escape.sql", "evil"),
            vq("/abs/escape.sql", "evil"),
        ];

        write_verified_queries(&rows, &root).await.unwrap();

        // Safe path: written verbatim, nested parent created.
        let body = tokio::fs::read_to_string(root.join("example_sql/answer.sql"))
            .await
            .unwrap();
        assert_eq!(body, "SELECT 1 /* oxy: */");
        // `..` traversal skipped — nothing lands beside the tempdir.
        assert!(!root.parent().unwrap().join("escape.sql").exists());
        // Absolute path skipped — not silently honoured.
        assert!(!std::path::Path::new("/abs/escape.sql").exists());
    }

    // Two rows mapping to the same target dedupe (first wins) instead of racing
    // or erroring — mirrors `write_context_artifacts`.
    #[tokio::test]
    async fn write_verified_queries_dedupes_same_path() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        let rows = vec![vq("q.sql", "first"), vq("q.sql", "second")];
        write_verified_queries(&rows, &root).await.unwrap();
        let body = tokio::fs::read_to_string(root.join("q.sql")).await.unwrap();
        assert_eq!(body, "first");
    }
}
