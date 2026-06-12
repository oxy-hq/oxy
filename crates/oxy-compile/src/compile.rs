//! The compile orchestrator.
//!
//! `compile_workspace` is the single entry point used by the CLI, by
//! the future TaskSpec wrapper, and by tests. It:
//!
//!   1. Discovers every interesting file (delegated to `walker`).
//!   2. Parses each into a generic JSON value (no strict typing in
//!      Phase 1.6a — that lands when runtime starts *reading* the
//!      rows in Phase 1.6c).
//!   3. Builds an in-memory list of `CompiledRow` values and a
//!      parallel list of per-file failures.
//!   4. Hands them to `writer` which materialises them into Postgres
//!      under a single revision_id.
//!
//! Phase 1.6a deliberately does NOT promote the new revision to
//! `workspaces.current_revision_id` (observation mode). The runtime
//! still reads YAML from disk; we're only verifying that compile
//! produces sane rows in production.

use crate::errors::CompileError;
use crate::outcome::{CompileOutcome, FailureKind, FileFailure, RevisionStatus};
use crate::walker::{DiscoveredFile, FileKind, ProcedureKind, discover};
use crate::writer::{
    FinaliseInput, FinaliseOutcome, RevisionContext, finalise_revision, insert_compiling_revision,
    mark_failed,
};
use sea_orm::DatabaseConnection;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

pub const CURRENT_SCHEMA_VERSION: i32 = 1;

/// How recent a successful revision has to be for an idempotent
/// re-compile to reuse it. Hardcoded to one hour; a same-SHA re-compile
/// inside the window short-circuits and returns the existing revision.
/// The window is intentionally not env-tunable — the partial unique
/// index in Postgres is the multi-worker correctness primitive; this
/// window is just a single-worker fast path that does not need a knob.
const IDEMPOTENCY_WINDOW_SECS: i64 = 60 * 60;

/// One row queued for insertion. Mirrors the per-entity tables.
#[derive(Debug, Clone)]
pub enum CompiledRow {
    Config(CompiledConfig),
    Agent(CompiledAgent),
    View(CompiledView),
    Topic(CompiledTopic),
    App(CompiledApp),
    Procedure(CompiledProcedure),
    VerifiedQuery(CompiledVerifiedQuery),
    Pipeline(CompiledPipeline),
    Reference(CompiledReference),
    MonitorConfig(CompiledMonitorConfig),
}

#[derive(Debug, Clone)]
pub struct CompiledMonitorConfig {
    pub definition: Value,
}

#[derive(Debug, Clone, Default)]
pub struct CompiledConfig {
    pub databases: Value,
    pub models: Option<Value>,
    pub integrations: Option<Value>,
    pub repositories: Option<Value>,
    pub builder_agent: Option<Value>,
    pub mcp: Option<Value>,
    pub other: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct CompiledAgent {
    pub name: String,
    pub file_path: String,
    pub definition: Value,
}

#[derive(Debug, Clone)]
pub struct CompiledView {
    pub name: String,
    pub file_path: String,
    pub definition: Value,
}

#[derive(Debug, Clone)]
pub struct CompiledTopic {
    pub name: String,
    pub file_path: String,
    pub definition: Value,
}

#[derive(Debug, Clone)]
pub struct CompiledApp {
    pub file_path: String,
    pub name: String,
    pub definition: Value,
    pub published: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledProcedure {
    pub file_path: String,
    pub name: String,
    pub extension: String,
    pub definition: Value,
}

#[derive(Debug, Clone)]
pub struct CompiledVerifiedQuery {
    pub file_path: String,
    pub content_sha256: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct CompiledPipeline {
    pub name: String,
    pub file_path: String,
    pub definition: Value,
}

#[derive(Debug, Clone)]
pub struct CompiledReference {
    pub from_kind: String,
    pub from_name: String,
    pub to_kind: String,
    pub to_name: String,
}

/// Public inputs to a compile run.
pub struct CompileRequest<'a> {
    pub db: &'a DatabaseConnection,
    pub workspace_id: Uuid,
    pub workspace_path: &'a Path,
    /// The SHA the operator should see on this revision. If None,
    /// recorded as the literal "local" — useful for `oxy compile`
    /// runs against a working copy with uncommitted edits.
    pub git_sha: Option<String>,
    pub branch: Option<String>,
    /// Identifies the binary version that produced this revision.
    pub compiler_version: String,
    /// When true AND the compile succeeds AND `kind == "main"`,
    /// atomically updates `workspaces.current_revision_id` to the
    /// new revision inside the finalise transaction. Defaults to
    /// false so observation-mode behaviour is the no-op (the
    /// foundation PR's contract).
    pub promote: bool,
    /// `main` (default) or `draft`. Drafts are scoped to a single
    /// `owner_user_id` and are never promoted to current_revision_id
    /// even when `promote` is true.
    pub kind: RevisionKind,
    /// Required when `kind == Draft`; ignored otherwise.
    pub owner_user_id: Option<Uuid>,
}

/// `main` vs `draft` revision kinds. Strictly typed so a caller can't
/// accidentally pass an unrecognised kind string into the writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionKind {
    Main,
    Draft,
}

impl RevisionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RevisionKind::Main => "main",
            RevisionKind::Draft => "draft",
        }
    }
}

impl Default for RevisionKind {
    fn default() -> Self {
        RevisionKind::Main
    }
}

/// The end-to-end compile entry point.
#[instrument(
    name = "oxy_compile",
    skip_all,
    fields(workspace_id = %request.workspace_id, git_sha = %request.git_sha.as_deref().unwrap_or("local")),
)]
pub async fn compile_workspace(
    request: CompileRequest<'_>,
) -> Result<CompileOutcome, CompileError> {
    let git_sha = request.git_sha.unwrap_or_else(|| "local".to_string());
    let kind_str = request.kind.as_str();
    info!(
        workspace_path = %request.workspace_path.display(),
        promote = request.promote,
        kind = kind_str,
        "compile started"
    );

    // Drafts MUST carry an owner_user_id so the future user-scoped
    // read path knows whom to serve them to. Catch the misconfigured
    // call here rather than letting it land as an integrity check
    // surprise downstream.
    if matches!(request.kind, RevisionKind::Draft) && request.owner_user_id.is_none() {
        return Err(CompileError::Internal(
            "draft revision requires owner_user_id".to_string(),
        ));
    }

    // Idempotency short-circuit. Skip the heavy work when a recent
    // successful revision already exists for this (workspace_id,
    // git_sha) pair and the request isn't trying to overwrite via
    // a different kind/owner_user_id. Two callers want this:
    //
    //   - Operator-triggered re-runs against an unchanged SHA from
    //     the admin "Run compile now" form, or repeated IDE Compile
    //     button clicks on the same HEAD.
    //   - Multiple workspaces inheriting the same commit (e.g. forks)
    //     that get triggered close together.
    //
    // Local-CLI compiles (`git_sha = "local"`) opt out — local edits
    // are not addressable by SHA, so identity is the working tree
    // itself which can change between invocations.
    //
    // When `promote` is requested AND the existing revision isn't
    // already current, we still execute the lightweight promotion
    // path so `workspaces.current_revision_id` ends up pointing at
    // the matching revision. That keeps "run compile now with
    // promote" semantically correct even when the SHA is unchanged.
    if git_sha != "local"
        && !matches!(request.kind, RevisionKind::Draft)
        && let Some(existing) =
            lookup_idempotent_revision(request.db, request.workspace_id, &git_sha).await?
    {
        info!(
            workspace_id = %request.workspace_id,
            git_sha = %git_sha,
            revision_id = %existing.revision_id,
            "compile idempotent — reusing existing successful revision"
        );
        if request.promote {
            if let Err(e) = crate::writer::promote_existing(
                request.db,
                request.workspace_id,
                existing.revision_id,
            )
            .await
            {
                warn!(
                    ?e,
                    "idempotent promote failed; falling through to full compile"
                );
            } else {
                return Ok(existing.into_outcome(git_sha, request.branch));
            }
        } else {
            return Ok(existing.into_outcome(git_sha, request.branch));
        }
    }

    let ctx = insert_compiling_revision(
        request.db,
        request.workspace_id,
        &git_sha,
        request.branch.as_deref(),
        CURRENT_SCHEMA_VERSION,
        &request.compiler_version,
        kind_str,
        request.owner_user_id,
    )
    .await?;

    // From here on, every error path should mark the revision row as
    // failed so the operator can see it. Use a guard struct so panics
    // / early returns still record.
    let outcome = drive_compile(
        request.db,
        &ctx,
        request.workspace_path,
        &git_sha,
        request.branch,
        request.promote,
        kind_str,
    )
    .await;

    match outcome {
        Ok(o) => Ok(o),
        Err(e) => {
            warn!(
                ?e,
                "compile failed before finalise; marking revision failed"
            );
            mark_failed(request.db, ctx.revision_id, &e.to_string()).await;
            Err(e)
        }
    }
}

async fn drive_compile(
    db: &DatabaseConnection,
    ctx: &RevisionContext,
    workspace_path: &Path,
    git_sha: &str,
    branch: Option<String>,
    promote: bool,
    kind: &str,
) -> Result<CompileOutcome, CompileError> {
    let files = discover(workspace_path)?;
    debug!(file_count = files.len(), "files discovered");

    let mut rows: Vec<CompiledRow> = Vec::new();
    let mut failures: Vec<FileFailure> = Vec::new();
    let mut compiled = 0u32;

    // Track (kind, identifier) so we can flag duplicates within one
    // revision. Two .agentic.yml files with the same `name` would
    // collide on the (revision_id, name) PK and crash the txn;
    // catching it here turns it into a structured per-file failure.
    let mut seen_keys: HashSet<(FileKind, String)> = HashSet::new();

    for file in &files {
        match compile_one(file).await {
            Ok(produced) => {
                for row in produced {
                    if let Some(key) = row_dedupe_key(&row, &file.kind) {
                        if !seen_keys.insert((file.kind, key.clone())) {
                            failures.push(FileFailure {
                                path: file.rel_path.clone(),
                                kind: FailureKind::Duplicate,
                                message: format!(
                                    "duplicate identifier '{}' for this entity kind",
                                    key
                                ),
                            });
                            continue;
                        }
                    }
                    rows.push(row);
                }
                compiled += 1;
            }
            Err(f) => failures.push(f),
        }
    }

    // Mirror local-file DuckDB warehouses to S3 so the stateless fleet can
    // query them (no-op without OXY_COMPILE_BLOB_S3_BUCKET). Only on an
    // otherwise-clean revision — a Failed revision won't be promoted, so
    // there's nothing to serve and no point uploading its data.
    if failures.is_empty()
        && let Some(CompiledRow::Config(cfg)) = rows
            .iter_mut()
            .find(|r| matches!(r, CompiledRow::Config(_)))
    {
        crate::duckdb_mirror::mirror_duckdb_databases(
            workspace_path,
            ctx.workspace_id,
            &mut cfg.databases,
        )
        .await;
    }

    let status = if failures.is_empty() {
        RevisionStatus::Ready
    } else {
        RevisionStatus::Failed
    };

    let outcome = finalise_revision(
        db,
        ctx,
        FinaliseInput {
            status: status.clone(),
            file_count_seen: files.len() as u32,
            file_count_compiled: compiled,
            file_count_failed: failures.len() as u32,
            failures: &failures,
            rows: &rows,
            promote,
            kind: kind.to_string(),
        },
    )
    .await?;

    // Multi-worker idempotency: when the partial unique index
    // rejected our finalise, return the winner's outcome instead.
    let finished_at = match outcome {
        FinaliseOutcome::Committed { finished_at } => finished_at,
        FinaliseOutcome::SupersededBy { revision_id } => {
            info!(
                losing = %ctx.revision_id,
                winning = %revision_id,
                "compile superseded by concurrent worker — returning winner's outcome"
            );
            // Fetch by PK, NOT through `lookup_idempotent_revision` —
            // that helper is window-gated (won't see a winner that
            // finished outside the idempotency window). The partial
            // unique index that produced this `SupersededBy` outcome
            // is window-independent. The winner's revision_id is
            // already known; load it directly.
            return lookup_revision_by_id(db, revision_id)
                .await?
                .map(|r| r.into_outcome(git_sha.to_string(), branch.clone()))
                .ok_or_else(|| {
                    CompileError::Internal(format!(
                        "superseded by revision {revision_id} but the row could not be loaded"
                    ))
                });
        }
    };

    info!(
        revision_id = %ctx.revision_id,
        files_seen = files.len(),
        files_compiled = compiled,
        files_failed = failures.len(),
        status = ?status,
        "compile finished"
    );

    Ok(CompileOutcome {
        revision_id: ctx.revision_id,
        status,
        git_sha: git_sha.to_string(),
        branch,
        started_at: ctx.started_at,
        finished_at,
        file_count_seen: files.len() as u32,
        file_count_compiled: compiled,
        file_count_failed: failures.len() as u32,
        failures,
    })
}

/// Per-file compile. Returns one or more rows on success, a structured
/// failure on parse / shape errors.
async fn compile_one(file: &DiscoveredFile) -> Result<Vec<CompiledRow>, FileFailure> {
    let bytes = tokio::fs::read(&file.abs_path)
        .await
        .map_err(|e| FileFailure {
            path: file.rel_path.clone(),
            kind: FailureKind::Io,
            message: e.to_string(),
        })?;
    let content = String::from_utf8_lossy(&bytes).into_owned();

    match file.kind {
        FileKind::Config => compile_config(file, &content),
        FileKind::AgenticAgent => compile_named_yaml(file, &content, |name, file_path, def| {
            CompiledRow::Agent(CompiledAgent {
                name,
                file_path,
                definition: def,
            })
        }),
        FileKind::SemanticView => compile_named_yaml(file, &content, |name, file_path, def| {
            CompiledRow::View(CompiledView {
                name,
                file_path,
                definition: def,
            })
        }),
        FileKind::SemanticTopic => compile_named_yaml(file, &content, |name, file_path, def| {
            CompiledRow::Topic(CompiledTopic {
                name,
                file_path,
                definition: def,
            })
        }),
        FileKind::App => compile_app(file, &content),
        FileKind::Procedure(p) => compile_procedure(file, &content, p),
        FileKind::AirwayPipeline => compile_named_yaml(file, &content, |name, file_path, def| {
            CompiledRow::Pipeline(CompiledPipeline {
                name,
                file_path,
                definition: def,
            })
        }),
        FileKind::VerifiedQuery => compile_verified_query(file, &content),
        FileKind::MonitorConfig => compile_monitor_config(file, &content),
    }
}

fn compile_monitor_config(
    file: &DiscoveredFile,
    content: &str,
) -> Result<Vec<CompiledRow>, FileFailure> {
    let value = parse_yaml(file, content)?;
    Ok(vec![CompiledRow::MonitorConfig(CompiledMonitorConfig {
        definition: value,
    })])
}

fn compile_config(file: &DiscoveredFile, content: &str) -> Result<Vec<CompiledRow>, FileFailure> {
    let value = parse_yaml(file, content)?;
    let mut cfg = CompiledConfig::default();
    let mut remaining = value.clone();
    match remaining {
        Value::Object(ref mut map) => {
            // Databases is the only field we require to exist; the
            // rest are optional. Empty array is valid because a
            // workspace with no databases yet is a real state during
            // onboarding.
            cfg.databases = map.remove("databases").unwrap_or(Value::Array(vec![]));
            cfg.models = map.remove("models");
            cfg.integrations = map.remove("integrations");
            cfg.repositories = map.remove("repositories");
            cfg.builder_agent = map.remove("builder_agent");
            cfg.mcp = map.remove("mcp");
            cfg.other = if map.is_empty() {
                None
            } else {
                Some(Value::Object(std::mem::take(map)))
            };

            // Strip inline secret literals before they hit Postgres. The
            // compiled config lands in `workspace_compiled_configs` — a
            // central, queryable, multi-tenant, per-revision-retained table —
            // so a plaintext `password:` in config.yml would be a far worse
            // resting place than the old per-worker FS read. `*_var` references
            // are preserved; the runtime resolves those from the encrypted
            // secret store at query time. Workspaces that used inline literals
            // must migrate to `*_var` (the documented pattern). Local/IDE reads
            // come from the filesystem, so they're unaffected — only the
            // stateless cloud fleet reads this compiled copy.
            let mut redacted = redact_inline_secrets(&mut cfg.databases);
            for field in [
                &mut cfg.integrations,
                &mut cfg.mcp,
                &mut cfg.builder_agent,
                &mut cfg.other,
            ]
            .into_iter()
            .flatten()
            {
                redacted += redact_inline_secrets(field);
            }
            if redacted > 0 {
                tracing::warn!(
                    path = ?file.rel_path,
                    redacted,
                    "compile: redacted inline secret literal(s) from config.yml — move them to \
                     the encrypted secret store via `*_var` references; inline secrets are not \
                     carried into the compiled config served to the runtime"
                );
            }
            Ok(vec![CompiledRow::Config(cfg)])
        }
        _ => Err(FileFailure {
            path: file.rel_path.clone(),
            kind: FailureKind::Shape,
            message: "config.yml must be a YAML mapping at the top level".into(),
        }),
    }
}

/// Recursively replace inline secret *literals* with `null` in a compiled
/// config sub-tree. Returns the number of fields redacted. `*_var` references
/// are preserved (they're env-var names, not secrets); only non-empty string
/// values under a sensitive key are stripped.
fn redact_inline_secrets(value: &mut Value) -> usize {
    match value {
        Value::Object(map) => {
            let mut n = 0;
            for (k, v) in map.iter_mut() {
                if is_sensitive_key(k) && matches!(v, Value::String(s) if !s.is_empty()) {
                    *v = Value::Null;
                    n += 1;
                } else {
                    n += redact_inline_secrets(v);
                }
            }
            n
        }
        Value::Array(arr) => arr.iter_mut().map(redact_inline_secrets).sum(),
        _ => 0,
    }
}

/// Whether a config key names a secret. Conservative substring match over the
/// credential field names used across the warehouse/integration configs, minus
/// the `*_var` reference suffix (those are kept so the runtime can resolve them
/// from the encrypted secret store).
fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    if k.ends_with("_var") {
        return false;
    }
    const NEEDLES: &[&str] = &[
        "password",
        "passwd",
        "passphrase",
        "secret",
        "token",
        "credential",
        "private_key",
        "api_key",
        "apikey",
        "access_key",
        "secret_key",
        "account_key",
        "client_secret",
        "connection_string",
        "sas_token",
        "key_path",
        "service_account",
    ];
    NEEDLES.iter().any(|needle| k.contains(needle))
}

#[cfg(test)]
mod redact_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_inline_literals_but_keeps_var_refs() {
        let mut v = json!({
            "databases": [{
                "name": "wh",
                "password": "hunter2",
                "password_var": "WH_PASSWORD",
                "host": "db.example.com"
            }]
        });
        let n = redact_inline_secrets(&mut v);
        assert_eq!(n, 1, "only the inline password literal should be redacted");
        let db = &v["databases"][0];
        assert!(db["password"].is_null(), "inline password stripped");
        assert_eq!(db["password_var"], "WH_PASSWORD", "var reference kept");
        assert_eq!(db["host"], "db.example.com", "non-secret kept");
        assert_eq!(db["name"], "wh");
    }

    #[test]
    fn redacts_nested_and_varied_secret_keys() {
        let mut v = json!({
            "service_account_json": "{...}",
            "nested": { "client_secret": "abc", "api_key": "k", "label": "ok" },
            "empty": "",
        });
        let n = redact_inline_secrets(&mut v);
        assert_eq!(n, 3);
        assert!(v["service_account_json"].is_null());
        assert!(v["nested"]["client_secret"].is_null());
        assert!(v["nested"]["api_key"].is_null());
        assert_eq!(v["nested"]["label"], "ok");
    }
}

fn compile_app(file: &DiscoveredFile, content: &str) -> Result<Vec<CompiledRow>, FileFailure> {
    let value = parse_yaml(file, content)?;
    let map = match &value {
        Value::Object(m) => m,
        _ => {
            return Err(shape_err(
                file,
                "app YAML must be a mapping at the top level",
            ));
        }
    };
    let name = map
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| derive_name_from_path(&file.rel_path));
    let published = map
        .get("published")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(vec![CompiledRow::App(CompiledApp {
        file_path: file.rel_path.clone(),
        name,
        definition: value,
        published,
    })])
}

fn compile_procedure(
    file: &DiscoveredFile,
    content: &str,
    proc_kind: ProcedureKind,
) -> Result<Vec<CompiledRow>, FileFailure> {
    let value = parse_yaml(file, content)?;
    let name = value
        .as_object()
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| derive_name_from_path(&file.rel_path));
    Ok(vec![CompiledRow::Procedure(CompiledProcedure {
        file_path: file.rel_path.clone(),
        name,
        extension: proc_kind.extension().to_string(),
        definition: value,
    })])
}

fn compile_verified_query(
    file: &DiscoveredFile,
    content: &str,
) -> Result<Vec<CompiledRow>, FileFailure> {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    Ok(vec![CompiledRow::VerifiedQuery(CompiledVerifiedQuery {
        file_path: file.rel_path.clone(),
        content_sha256: hash,
        content: content.to_string(),
    })])
}

/// Helper for YAML kinds whose row shape is essentially
/// `(name, file_path, definition jsonb)`.
fn compile_named_yaml<F>(
    file: &DiscoveredFile,
    content: &str,
    make_row: F,
) -> Result<Vec<CompiledRow>, FileFailure>
where
    F: FnOnce(String, String, Value) -> CompiledRow,
{
    let value = parse_yaml(file, content)?;
    let map = match &value {
        Value::Object(m) => m,
        _ => return Err(shape_err(file, "YAML must be a mapping at the top level")),
    };
    let name = map
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| derive_name_from_path(&file.rel_path));
    Ok(vec![make_row(name, file.rel_path.clone(), value)])
}

fn parse_yaml(file: &DiscoveredFile, content: &str) -> Result<Value, FileFailure> {
    serde_yaml::from_str::<Value>(content).map_err(|e| FileFailure {
        path: file.rel_path.clone(),
        kind: FailureKind::Yaml,
        message: e.to_string(),
    })
}

fn shape_err(file: &DiscoveredFile, message: &str) -> FileFailure {
    FileFailure {
        path: file.rel_path.clone(),
        kind: FailureKind::Shape,
        message: message.to_string(),
    }
}

/// Strip the kind-specific suffix(es) and the trailing extension so a
/// nameless YAML row still gets a sensible identifier — e.g.
/// `agents/clarify.agentic.yml` → `agents/clarify`. Used as a fallback
/// when the `name` field is missing.
fn derive_name_from_path(rel_path: &str) -> String {
    const STRIPS: &[&str] = &[
        ".agentic.yml",
        ".view.yml",
        ".topic.yml",
        ".app.yml",
        ".procedure.yml",
        ".workflow.yml",
        ".automation.yml",
        ".airway.yml",
        ".yml",
    ];
    for s in STRIPS {
        if let Some(stem) = rel_path.strip_suffix(s) {
            return stem.to_string();
        }
    }
    rel_path.to_string()
}

/// Returns the identifier that would collide on the per-entity PK if
/// duplicated. Some kinds key by name; some by file_path. Config has
/// no per-row dedup (revision_id is the PK).
fn row_dedupe_key(row: &CompiledRow, _kind: &FileKind) -> Option<String> {
    match row {
        CompiledRow::Config(_) => None,
        CompiledRow::Agent(a) => Some(format!("agent:{}", a.name)),
        CompiledRow::View(v) => Some(format!("view:{}", v.name)),
        CompiledRow::Topic(t) => Some(format!("topic:{}", t.name)),
        CompiledRow::App(a) => Some(format!("app:{}", a.file_path)),
        CompiledRow::Procedure(p) => Some(format!("proc:{}", p.file_path)),
        CompiledRow::VerifiedQuery(q) => Some(format!("sql:{}", q.file_path)),
        CompiledRow::Pipeline(p) => Some(format!("pipe:{}", p.name)),
        CompiledRow::Reference(_) => None,
        CompiledRow::MonitorConfig(_) => None,
    }
}

/// Snapshot of a successful revisions row we can reuse for an
/// idempotent compile. Carries just the fields the outcome needs;
/// the per-entity rows already exist tagged with `revision_id`.
#[derive(Debug, Clone)]
pub(crate) struct IdempotentRevision {
    pub(crate) revision_id: Uuid,
    pub(crate) started_at: chrono::DateTime<chrono::Utc>,
    pub(crate) finished_at: chrono::DateTime<chrono::Utc>,
    pub(crate) file_count_seen: u32,
    pub(crate) file_count_compiled: u32,
    pub(crate) file_count_failed: u32,
}

impl IdempotentRevision {
    pub(crate) fn into_outcome(self, git_sha: String, branch: Option<String>) -> CompileOutcome {
        CompileOutcome {
            revision_id: self.revision_id,
            status: crate::outcome::RevisionStatus::Ready,
            git_sha,
            branch,
            started_at: self.started_at,
            finished_at: self.finished_at,
            file_count_seen: self.file_count_seen,
            file_count_compiled: self.file_count_compiled,
            file_count_failed: self.file_count_failed,
            failures: Vec::new(),
        }
    }
}

/// Load a revision by primary key. Window-independent — used by the
/// `SupersededBy` recovery path where the winner's `revision_id` is
/// already known from the writer's outcome. The conversion to
/// `IdempotentRevision` (the carrier type used by `into_outcome`) only
/// succeeds when the row is `ready` and has a `finished_at`; otherwise
/// returns `Ok(None)` and the caller surfaces an internal error.
async fn lookup_revision_by_id(
    db: &DatabaseConnection,
    revision_id: Uuid,
) -> Result<Option<IdempotentRevision>, CompileError> {
    use sea_orm::EntityTrait;

    let row = entity::revisions::Entity::find_by_id(revision_id)
        .one(db)
        .await?;
    Ok(row.and_then(|r| {
        if r.status != "ready" {
            return None;
        }
        let finished_at = r.finished_at.map(|f| f.with_timezone(&chrono::Utc))?;
        Some(IdempotentRevision {
            revision_id: r.revision_id,
            started_at: r.started_at.with_timezone(&chrono::Utc),
            finished_at,
            file_count_seen: r.file_count_seen.max(0) as u32,
            file_count_compiled: r.file_count_compiled.max(0) as u32,
            file_count_failed: r.file_count_failed.max(0) as u32,
        })
    }))
}

/// Look up the most recent ready `main` revision for this
/// `(workspace_id, git_sha)` within the idempotency window. Returns
/// `Ok(None)` when there's no eligible row, when the window is zero
/// (operator turned idempotency off), or when the DB hiccups (the
/// caller falls through to a full compile).
async fn lookup_idempotent_revision(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    git_sha: &str,
) -> Result<Option<IdempotentRevision>, CompileError> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(IDEMPOTENCY_WINDOW_SECS);
    let cutoff_offset = cutoff.fixed_offset();

    let row = entity::revisions::Entity::find()
        .filter(entity::revisions::Column::WorkspaceId.eq(workspace_id))
        .filter(entity::revisions::Column::GitSha.eq(git_sha))
        .filter(entity::revisions::Column::Status.eq("ready"))
        .filter(entity::revisions::Column::Kind.eq("main"))
        .filter(entity::revisions::Column::FinishedAt.gte(cutoff_offset))
        .order_by_desc(entity::revisions::Column::FinishedAt)
        .one(db)
        .await?;

    Ok(row.and_then(|r| {
        let finished_at = r.finished_at.map(|f| f.with_timezone(&chrono::Utc))?;
        Some(IdempotentRevision {
            revision_id: r.revision_id,
            started_at: r.started_at.with_timezone(&chrono::Utc),
            finished_at,
            file_count_seen: r.file_count_seen.max(0) as u32,
            file_count_compiled: r.file_count_compiled.max(0) as u32,
            file_count_failed: r.file_count_failed.max(0) as u32,
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walker::{FileKind, discover};
    use std::fs;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[tokio::test]
    async fn compile_one_app_extracts_name_and_published() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write(
            root,
            "apps/sales.app.yml",
            "name: Sales overview\npublished: true\ntasks: []\n",
        );
        let files = discover(root).unwrap();
        let app_file = files
            .iter()
            .find(|f| matches!(f.kind, FileKind::App))
            .unwrap();
        let rows = compile_one(app_file).await.unwrap();
        let row = &rows[0];
        match row {
            CompiledRow::App(a) => {
                assert_eq!(a.name, "Sales overview");
                assert!(a.published);
                assert_eq!(a.file_path, "apps/sales.app.yml");
            }
            other => panic!("expected App, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn compile_one_app_falls_back_to_path_name_when_unnamed() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write(root, "apps/nameless.app.yml", "tasks: []\n");
        let files = discover(root).unwrap();
        let app_file = files
            .iter()
            .find(|f| matches!(f.kind, FileKind::App))
            .unwrap();
        let rows = compile_one(app_file).await.unwrap();
        let row = &rows[0];
        match row {
            CompiledRow::App(a) => {
                assert_eq!(a.name, "apps/nameless");
                assert!(!a.published);
            }
            other => panic!("expected App, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn compile_one_yaml_failure_returns_structured_error() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write(
            root,
            "agents/broken.agentic.yml",
            "this: is: invalid: yaml:\n",
        );
        let files = discover(root).unwrap();
        let file = files
            .iter()
            .find(|f| matches!(f.kind, FileKind::AgenticAgent))
            .unwrap();
        let err = compile_one(file).await.unwrap_err();
        assert!(matches!(err.kind, FailureKind::Yaml));
        assert_eq!(err.path, "agents/broken.agentic.yml");
    }

    #[tokio::test]
    async fn compile_one_verified_query_records_sha256() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        write(root, "queries/top.sql", "SELECT 1\n");
        let files = discover(root).unwrap();
        let file = files
            .iter()
            .find(|f| matches!(f.kind, FileKind::VerifiedQuery))
            .unwrap();
        let rows = compile_one(file).await.unwrap();
        match &rows[0] {
            CompiledRow::VerifiedQuery(q) => {
                assert_eq!(q.file_path, "queries/top.sql");
                assert_eq!(q.content, "SELECT 1\n");
                // sha256("SELECT 1\n") — sanity check, not pinned.
                assert_eq!(q.content_sha256.len(), 64);
                assert!(q.content_sha256.chars().all(|c| c.is_ascii_hexdigit()));
            }
            other => panic!("expected VerifiedQuery, got {:?}", other),
        }
    }

    #[test]
    fn derive_name_from_path_strips_known_suffixes() {
        assert_eq!(
            derive_name_from_path("agents/foo.agentic.yml"),
            "agents/foo"
        );
        assert_eq!(derive_name_from_path("views/v.view.yml"), "views/v");
        assert_eq!(derive_name_from_path("topics/t.topic.yml"), "topics/t");
        assert_eq!(derive_name_from_path("apps/a.app.yml"), "apps/a");
        assert_eq!(derive_name_from_path("p/x.procedure.yml"), "p/x");
        assert_eq!(derive_name_from_path("p/y.workflow.yml"), "p/y");
        assert_eq!(derive_name_from_path("p/z.automation.yml"), "p/z");
        assert_eq!(derive_name_from_path("pipe/a.airway.yml"), "pipe/a");
        assert_eq!(derive_name_from_path("noext"), "noext");
    }
}
