//! Hybrid Postgres-first reader for endpoints on the compile boundary.
//! Every public fn returns:
//!
//! * `Ok(Some(...))` — workspace is promoted on its default branch; here
//!   are the rows.
//! * `Ok(None)` — caller should fall through to the legacy FS path.
//!   Reasons: branch is non-default (IDE draft mode), no promoted revision
//!   exists yet, or a transient DB error.
//! * `Err(...)` — programmer-error DB failure surfaced from a successful
//!   `current_revision_id` lookup (rare). Connect failures are
//!   intentionally downgraded to `Ok(None)` so a Postgres hiccup can never
//!   blank the IDE sidebar.
//!
//! Resolution order on every call:
//!
//! 1. If the workspace is the nil-UUID local/single-instance workspace →
//!    `Ok(None)`. Local mode has the live working copy on disk and no
//!    compile-on-save, so it always reads FS.
//! 2. **IDE / single-process only:** if `branch_hint` is `Some(name)` and that
//!    name is not the workspace's default branch → `Ok(None)`. Non-default
//!    branches are drafts; the FS working copy (with uncommitted edits) is
//!    freshest. A stateless `serve` replica SKIPS this gate — it has no working
//!    copy, so it serves the latest promoted revision regardless of branch.
//! 3. Read `workspaces.current_revision_id`. If null → `Ok(None)`.
//! 4. Query the per-entity table keyed by that revision_id.

use crate::server::role_manifest::{Role, current_process_role};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

tokio::task_local! {
    /// Request-scoped revision pin. Set once by `workspace_middleware` for the
    /// duration of an HTTP request (via [`with_pinned_revision`]); read at the
    /// single resolver choke-point [`open_compiled_revision`]. With it set,
    /// every reader in one request shares ONE revision — so a promotion landing
    /// mid-request can never produce a torn read (config from revision N, app
    /// from N+1). It is **read-consistency request context**, not pipeline
    /// state: absent outside an HTTP request (background tasks, spawned
    /// sub-tasks), the readers fall back to per-call resolution unchanged.
    ///
    /// The inner `Option<Uuid>` distinguishes "pinned to revision X" (`Some`)
    /// from "pinned to no revision — read FS" (`None`); the task-local being
    /// *present at all* is what makes a reader trust the pin instead of
    /// re-resolving.
    static PINNED_REVISION: Option<Uuid>;
}

/// Run `fut` with the request's revision pinned. `pinned` is the result of a
/// single [`resolve_request_revision`] call at request entry.
pub async fn with_pinned_revision<F, T>(pinned: Option<Uuid>, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    PINNED_REVISION.scope(pinned, fut).await
}

/// Resolve the one revision this request should read, applying the full
/// branch / promotion / local-mode / DB-health logic exactly once. The caller
/// (`workspace_middleware`) stashes the result via [`with_pinned_revision`] so
/// every downstream reader is consistent. `None` → the request reads the
/// filesystem everywhere (non-default branch, unpromoted, local mode, or a DB
/// hiccup), matching the per-call fallback contract.
pub async fn resolve_request_revision(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Option<Uuid> {
    let (db, candidate) = match open_compiled_revision(workspace_id, branch_hint).await {
        Ok(Some(pair)) => pair,
        _ => return None,
    };

    // Fail-safe: never pin a revision whose compiled config won't deserialise
    // into the runtime `Config` — that revision 503s every request for this
    // workspace. Prefer the most recent prior revision that DOES deserialise
    // (last-known-good), degrading one bad promote to slightly-stale-but-working
    // data instead of an outage. The compile-time round-trip gate keeps
    // `current_revision_id` good going forward; this covers revisions promoted
    // before the gate existed (the #2520 transition) and any gate bypass.
    if revision_config_loads(&db, candidate).await {
        return Some(candidate);
    }
    tracing::warn!(
        workspace_id = %workspace_id,
        broken_revision = %candidate,
        "pinned revision config does not deserialise; searching for last-known-good"
    );
    last_known_good_revision(&db, workspace_id, candidate).await
}

/// Process-local memo of "revision R's compiled config deserialises into
/// `Config`". Revision IDs are immutable, so the answer is stable for a
/// revision's lifetime; this keeps the happy path O(1) after the first check.
fn config_validity_cache() -> &'static Mutex<HashMap<Uuid, bool>> {
    static CACHE: OnceLock<Mutex<HashMap<Uuid, bool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// True when this revision's compiled config deserialises into the runtime
/// `Config` (i.e. the request hot path can serve it). A revision with NO config
/// row is treated as valid — there's no config to 503 on, and the existing
/// FS/NeedsRecompile fallthrough handles it; we only walk back on an actual
/// deserialise failure. DB errors are treated as valid (fail open to the
/// existing behaviour rather than blanking a workspace on a transient hiccup).
async fn revision_config_loads(db: &DatabaseConnection, revision_id: Uuid) -> bool {
    if let Some(v) = config_validity_cache()
        .lock()
        .ok()
        .and_then(|c| c.get(&revision_id).copied())
    {
        return v;
    }
    let valid = match load_config_value(db, revision_id).await {
        Ok(Some(value)) => serde_json::from_value::<oxy::config::model::Config>(value).is_ok(),
        Ok(None) => true,
        Err(e) => {
            // Fail OPEN (don't blank a workspace on a transient hiccup) but do
            // NOT cache it: memoising `true` here would permanently mark a
            // genuinely-broken revision valid for this process, defeating the
            // last-known-good fallback whose whole job is preventing 503s.
            // Re-evaluate on the next request instead. (#2524 review)
            tracing::warn!(
                ?e, %revision_id,
                "config validity check: DB error; treating as valid for this request only (not cached)"
            );
            return true;
        }
    };
    if let Ok(mut c) = config_validity_cache().lock() {
        c.insert(revision_id, valid);
    }
    valid
}

/// Walk recent `ready` `main` revisions newest-first and return the first whose
/// compiled config deserialises. Bounded scan so a long broken history can't
/// turn one request into a long sweep. `None` → no good revision found (caller
/// falls through to FS / NeedsRecompile, matching today's behaviour).
async fn last_known_good_revision(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    exclude: Uuid,
) -> Option<Uuid> {
    const SCAN_LIMIT: u64 = 10;
    let rows = entity::revisions::Entity::find()
        .filter(entity::revisions::Column::WorkspaceId.eq(workspace_id))
        .filter(entity::revisions::Column::Kind.eq("main"))
        .filter(entity::revisions::Column::Status.eq("ready"))
        .order_by_desc(entity::revisions::Column::FinishedAt)
        .limit(SCAN_LIMIT)
        .all(db)
        .await
        .unwrap_or_default();
    for r in rows {
        if r.revision_id == exclude {
            continue;
        }
        if revision_config_loads(db, r.revision_id).await {
            tracing::info!(
                workspace_id = %workspace_id,
                good_revision = %r.revision_id,
                "serving last-known-good compiled revision (current revision config is unreadable)"
            );
            return Some(r.revision_id);
        }
    }
    None
}

/// Lightweight row shape carrying the fields the apps endpoint needs.
#[derive(Debug, Clone)]
pub struct CompiledApp {
    pub file_path: String,
    pub name: String,
    pub published: bool,
    /// Pulled out of the `definition` JSONB for the sidebar. Falls
    /// back to `name` when absent.
    pub title: Option<String>,
}

/// Row shape for the analytics-agent listing endpoint. Includes the
/// `llm.ref` value pulled from the JSONB so the home page can flag
/// readiness gaps against the agent the chat will actually use, plus the
/// `timezone` so the UI clock can render the workspace's local time.
#[derive(Debug, Clone)]
pub struct CompiledAgent {
    pub file_path: String,
    pub name: String,
    pub model_ref: Option<String>,
    pub timezone: Option<String>,
}

/// Row shape for the automation listing. The legacy extensions
/// (`.procedure.yml`, `.automation.yml`) are
/// preserved on the row so the file-tree grouping can show them.
#[derive(Debug, Clone)]
pub struct CompiledAutomation {
    pub file_path: String,
    pub name: String,
    pub extension: String,
}

/// Single-entity resolver result. Carries the full `definition` JSONB
/// so the runtime can deserialize into its strict type.
#[derive(Debug, Clone)]
pub struct CompiledArtifact {
    pub file_path: String,
    pub name: String,
    pub definition: Value,
    /// When set, an S3 object holds the canonical body for this
    /// artifact (semantic views + topics only). Materialisers should
    /// prefer the blob over `definition` to keep tablespace cost
    /// bounded for large semantic layers. Falls back to `definition`
    /// when the blob is missing / transport errors.
    pub compiled_sql_blob_key: Option<String>,
}

/// Verified-query (`.sql`) resolver result. Unlike the YAML entities a
/// verified query has no parsed `definition` — its body IS the raw SQL
/// text, carried verbatim alongside the content hash the compile worker
/// recorded (`content_sha256`). The hash lets a caller verify integrity
/// after a Postgres/S3 round-trip (the content-addressing invariant).
#[derive(Debug, Clone)]
pub struct CompiledVerifiedQuery {
    pub file_path: String,
    pub content_sha256: String,
    pub content: String,
}

impl CompiledVerifiedQuery {
    /// Re-hash the carried body and compare to the `content_sha256` the
    /// compile worker recorded. A mismatch means the body was corrupted on
    /// the Postgres/S3 round-trip — the content-addressing integrity check
    /// from the compile-complete design (a verifier the loud serve-side
    /// `WorkspaceFs` impl can use to refuse a corrupt artifact rather than
    /// silently serve it). The hash format mirrors `oxy_compile`'s
    /// `compile_verified_query`: lowercase-hex SHA-256 of the raw bytes.
    pub fn integrity_ok(&self) -> bool {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.content.as_bytes());
        let actual: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        actual == self.content_sha256
    }
}

/// Return `Ok(Some(apps))` when the workspace has a promoted revision
/// on its default branch; `Ok(None)` otherwise (caller falls through to FS).
pub async fn list_apps(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
    published_only: bool,
) -> Result<Option<Vec<CompiledApp>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };

    let mut find = entity::app_definitions::Entity::find()
        .filter(entity::app_definitions::Column::RevisionId.eq(revision_id));
    if published_only {
        find = find.filter(entity::app_definitions::Column::Published.eq(true));
    }
    let rows = find.all(&db).await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledApp {
                file_path: m.file_path,
                title: extract_title(&m.definition),
                name: m.name,
                published: m.published,
            })
            .collect(),
    ))
}

/// Listing equivalent for `get_agents`. Pulls `llm.ref` out of the
/// JSONB to match the existing handler's response shape without re-
/// reading the YAML.
pub async fn list_analytics_agents(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Vec<CompiledAgent>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let rows = entity::agent_definitions::Entity::find()
        .filter(entity::agent_definitions::Column::RevisionId.eq(revision_id))
        .all(&db)
        .await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledAgent {
                file_path: m.file_path,
                model_ref: extract_model_ref(&m.definition),
                timezone: extract_timezone(&m.definition),
                name: m.name,
            })
            .collect(),
    ))
}

/// Listing equivalent for the automation file enumeration.
pub async fn list_automations(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Vec<CompiledAutomation>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let rows = entity::automation_definitions::Entity::find()
        .filter(entity::automation_definitions::Column::RevisionId.eq(revision_id))
        .all(&db)
        .await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledAutomation {
                file_path: m.file_path,
                name: m.name,
                extension: m.extension,
            })
            .collect(),
    ))
}

/// Single-app resolver. Lookup by `file_path` since the UI references
/// apps that way (`/apps/<pathb64>`) and a workspace can carry
/// duplicate `name`s in different folders.
pub async fn resolve_app(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
    file_path: &str,
) -> Result<Option<CompiledArtifact>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row = entity::app_definitions::Entity::find_by_id((revision_id, file_path.to_string()))
        .one(&db)
        .await?;
    Ok(row.map(|m| CompiledArtifact {
        file_path: m.file_path,
        name: m.name,
        definition: m.definition,
        compiled_sql_blob_key: None,
    }))
}

/// Single-agent resolver. Keyed by `name` because the analytics
/// pipeline references agents by name (`AgenticAgent.name`) rather
/// than path.
pub async fn resolve_analytics_agent(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
    name: &str,
) -> Result<Option<CompiledArtifact>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row = entity::agent_definitions::Entity::find_by_id((revision_id, name.to_string()))
        .one(&db)
        .await?;
    Ok(row.map(|m| CompiledArtifact {
        file_path: m.file_path,
        name: m.name,
        definition: m.definition,
        compiled_sql_blob_key: None,
    }))
}

/// Single-automation resolver, keyed by `file_path` (the PK column).
pub async fn resolve_automation(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
    file_path: &str,
) -> Result<Option<CompiledArtifact>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row =
        entity::automation_definitions::Entity::find_by_id((revision_id, file_path.to_string()))
            .one(&db)
            .await?;
    Ok(row.map(|m| CompiledArtifact {
        file_path: m.file_path,
        name: m.name,
        definition: m.definition,
        compiled_sql_blob_key: None,
    }))
}

/// Resolve the workspace's compiled `config.yml`. The
/// `workspace_compiled_configs` table mirrors the top-level shape of
/// the runtime `Config` struct in JSONB columns; callers merge them
/// back into one JSON object and deserialise with `serde_json::from_value`.
///
/// Caller is responsible for setting `Config.workspace_path` after
/// deserialisation — it's `#[serde(skip)]` so the round-trip leaves
/// it empty.
pub async fn resolve_workspace_config(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Value>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    load_config_value(&db, revision_id).await
}

/// Load and merge the compiled config for a specific revision into the single
/// top-level object `config.yml` deserialises from. Uses the SAME merge as the
/// compile-time gate (`oxy_compile::merge_compiled_config`) so the shape the
/// gate validated and the shape the reader serves can never drift.
async fn load_config_value(
    db: &DatabaseConnection,
    revision_id: Uuid,
) -> Result<Option<Value>, sea_orm::DbErr> {
    let Some(row) = entity::workspace_compiled_configs::Entity::find_by_id(revision_id)
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    let cfg = oxy_compile::CompiledConfig {
        databases: row.databases,
        models: row.models,
        integrations: row.integrations,
        repositories: row.repositories,
        builder_agent: row.builder_agent,
        mcp: row.mcp,
        other: row.other,
    };
    Ok(Some(oxy_compile::merge_compiled_config(&cfg)))
}

/// Resolve the workspace's compiled `.monitor.yml`. Singleton per
/// revision, so the row's `definition` JSONB is the full
/// `MonitorConfig` (schedule + monitors) ready to round-trip back
/// into the strict-typed struct.
pub async fn resolve_monitor_config(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Value>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row = entity::monitor_configs::Entity::find_by_id(revision_id)
        .one(&db)
        .await?;
    Ok(row.map(|m| m.definition))
}

/// Resolve the workspace's compiled `.world-model.yml`. Singleton per
/// revision, so the row's `definition` JSONB is the full
/// `WorldModelConfig` (top-level `entities`) ready to round-trip back
/// into the strict-typed struct.
pub async fn resolve_world_model_config(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Value>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row = entity::world_model_configs::Entity::find_by_id(revision_id)
        .one(&db)
        .await?;
    Ok(row.map(|m| m.definition))
}

/// List every `.view.yml` row for the workspace's current revision.
/// Used by callers that want to enumerate semantic views without
/// touching FS — typically to populate Postgres-only "scan" paths
/// that replace the airlayer FS walker.
pub async fn list_semantic_views(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Vec<CompiledArtifact>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let rows = entity::semantic_views::Entity::find()
        .filter(entity::semantic_views::Column::RevisionId.eq(revision_id))
        .all(&db)
        .await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledArtifact {
                file_path: m.file_path,
                name: m.name,
                definition: m.definition,
                compiled_sql_blob_key: m.compiled_sql_blob_key,
            })
            .collect(),
    ))
}

/// Single semantic-view resolver, keyed by `name`.
pub async fn resolve_semantic_view(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
    name: &str,
) -> Result<Option<CompiledArtifact>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row = entity::semantic_views::Entity::find_by_id((revision_id, name.to_string()))
        .one(&db)
        .await?;
    Ok(row.map(|m| CompiledArtifact {
        file_path: m.file_path,
        name: m.name,
        definition: m.definition,
        compiled_sql_blob_key: m.compiled_sql_blob_key,
    }))
}

/// List every `.topic.yml` row for the workspace's current revision.
pub async fn list_semantic_topics(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Vec<CompiledArtifact>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let rows = entity::semantic_topics::Entity::find()
        .filter(entity::semantic_topics::Column::RevisionId.eq(revision_id))
        .all(&db)
        .await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledArtifact {
                file_path: m.file_path,
                name: m.name,
                definition: m.definition,
                compiled_sql_blob_key: m.compiled_sql_blob_key,
            })
            .collect(),
    ))
}

/// Single semantic-topic resolver, keyed by `name`.
pub async fn resolve_semantic_topic(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
    name: &str,
) -> Result<Option<CompiledArtifact>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row = entity::semantic_topics::Entity::find_by_id((revision_id, name.to_string()))
        .one(&db)
        .await?;
    Ok(row.map(|m| CompiledArtifact {
        file_path: m.file_path,
        name: m.name,
        definition: m.definition,
        compiled_sql_blob_key: m.compiled_sql_blob_key,
    }))
}

/// List every verified-query (`.sql`) row for the workspace's current
/// revision. The compile worker already WRITES these (walker →
/// `compile_verified_query` → writer into `verified_queries`), but until
/// now nothing read them back — so a stateless `serve` replica running a
/// verified query (`agentic/analytics/.../solver/specifying`) had to fall
/// through to the workspace filesystem, which on a no-working-copy node is
/// the `FileReadError` leak the compile boundary exists to close. The
/// general agent-context materialiser consumes this list to write the
/// `.sql` bodies into the request tempdir so the solver's read resolves
/// without ever touching the real FS.
pub async fn list_verified_queries(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Vec<CompiledVerifiedQuery>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let rows = entity::verified_queries::Entity::find()
        .filter(entity::verified_queries::Column::RevisionId.eq(revision_id))
        .all(&db)
        .await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledVerifiedQuery {
                file_path: m.file_path,
                content_sha256: m.content_sha256,
                content: m.content,
            })
            .collect(),
    ))
}

/// Single verified-query resolver, keyed by `file_path`. Verified queries
/// are referenced by their workspace-relative path (the agent `context:`
/// glob discovers them on disk by path; on the boundary that same path is
/// the row's PK alongside `revision_id`), so unlike the named entities we
/// look up by `file_path` — mirroring `resolve_app`.
///
/// TRACKING (PR #2557): the §8 materialiser wires `list_verified_queries` into
/// the agent context, so the analytics agent already resolves verified `.sql`
/// from the boundary via the materialised tree. This single-row resolver is the
/// reader for a future *direct* per-request lookup (e.g. a `GET
/// /verified-queries/{path}` handler) and is intentionally caller-less until
/// that surface lands; remove it if that surface is dropped.
pub async fn resolve_verified_query(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
    file_path: &str,
) -> Result<Option<CompiledVerifiedQuery>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row = entity::verified_queries::Entity::find_by_id((revision_id, file_path.to_string()))
        .one(&db)
        .await?;
    Ok(row.map(|m| CompiledVerifiedQuery {
        file_path: m.file_path,
        content_sha256: m.content_sha256,
        content: m.content,
    }))
}

/// List every automation (`.procedure.yml` / `.automation.yml`)
/// row for the workspace's current revision, carrying the full `definition` so
/// it can be materialised back to a YAML file — unlike `list_automations`, which
/// returns only listing metadata (no body). Feeds the agent-context materialiser
/// so the analytics solver discovers and runs automations FS-free on the serve
/// fleet.
pub async fn list_automation_artifacts(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Vec<CompiledArtifact>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let rows = entity::automation_definitions::Entity::find()
        .filter(entity::automation_definitions::Column::RevisionId.eq(revision_id))
        .all(&db)
        .await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledArtifact {
                file_path: m.file_path,
                name: m.name,
                definition: m.definition,
                compiled_sql_blob_key: None,
            })
            .collect(),
    ))
}

/// List the workspace's compiled Airway pipelines (`airway_pipelines`) as
/// path-addressed artifacts, mirroring [`list_automation_artifacts`]. The
/// `.airway.yml` body is already compiled (walker → `CompiledRow::Pipeline` →
/// `airway_pipelines`); this is the missing reader.
pub async fn list_pipeline_artifacts(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Vec<CompiledArtifact>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let rows = entity::airway_pipelines::Entity::find()
        .filter(entity::airway_pipelines::Column::RevisionId.eq(revision_id))
        .all(&db)
        .await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledArtifact {
                file_path: m.file_path,
                name: m.name,
                definition: m.definition,
                compiled_sql_blob_key: None,
            })
            .collect(),
    ))
}

/// Shared gate consulted at the top of every public reader. Returns
/// `Some((db, revision_id))` when the request should be served from
/// Postgres, `None` when the caller should fall through to FS.
///
/// Returns `Ok(None)` (not `Err`) for non-default branch, DB connect
/// failure, or workspace-not-promoted. We never want the IDE sidebar to
/// blank because the compile boundary is unhappy — FS is always the safety net.
async fn open_compiled_revision(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<(DatabaseConnection, Uuid)>, sea_orm::DbErr> {
    // Local / single-instance mode (the nil-UUID workspace) always has the live
    // working copy on disk and there's no compile-on-save, so serve from the
    // filesystem — otherwise an on-disk edit to a promoted workspace wouldn't
    // show until the next manual compile. The compile boundary exists for the
    // stateless cloud fleet, which has no working copy.
    if workspace_id == crate::server::serve_mode::LOCAL_WORKSPACE_ID {
        return Ok(None);
    }
    // Request-scoped pin: `workspace_middleware` already ran the branch /
    // promotion / local checks once and stashed the result, so trust it and
    // skip the re-resolve. Every reader in the request thus shares one revision
    // (no torn reads). `Some(None)` means "this request reads FS everywhere".
    if let Ok(pinned) = PINNED_REVISION.try_with(|p| *p) {
        let Some(revision_id) = pinned else {
            return Ok(None);
        };
        return match oxy::database::client::establish_connection().await {
            Ok(db) => Ok(Some((db, revision_id))),
            Err(e) => {
                tracing::warn!(
                    ?e,
                    "compiled_reader: DB connect failed (pinned); falling to FS"
                );
                Ok(None)
            }
        };
    }
    let db = match oxy::database::client::establish_connection().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                ?e,
                "compiled_reader: DB connect failed; falling through to FS"
            );
            return Ok(None);
        }
    };
    // Branch gate — only on a node that HAS a working copy (IDE / single
    // process). There, a non-default branch is a draft and the on-disk working
    // copy (with uncommitted edits) is freshest, so fall through to FS. A
    // stateless `serve` replica has NO working copy: "fall back to FS" there
    // degrades to a useless `needs_recompile` 503, and pinning a per-workspace
    // default branch is fragile. So a serve replica skips the gate and serves
    // the latest promoted revision regardless of which branch the FE tagged —
    // the compiled revision is the only (and freshest) thing it can serve.
    // See oxygen-internal#2528.
    let effective_branch = normalize_branch_hint(branch_hint);
    if current_process_role() != Role::Serve
        && let Some(branch) = effective_branch
        && !is_default_branch(&db, workspace_id, branch).await
    {
        tracing::debug!(
            workspace_id = %workspace_id,
            branch,
            "compiled_reader: non-default branch on a working-copy node — using FS"
        );
        return Ok(None);
    }
    let row = entity::workspaces::Entity::find_by_id(workspace_id)
        .one(&db)
        .await?;
    let Some(revision_id) = row.and_then(|w| w.current_revision_id) else {
        return Ok(None);
    };
    Ok(Some((db, revision_id)))
}

/// Treat an empty `branch` query param the same as `None`. Most FE calls
/// omit the branch from the URL, so they arrive at the middleware as
/// `Some("")` after axum's query parser. Previously this fell through to
/// FS (which on a serve replica means a 500 — no working copy). Empty
/// means "default branch": let `open_compiled_revision` resolve it from
/// the workspace row instead. Regression context: oxy-hq/oxygen-internal#1619.
fn normalize_branch_hint(branch_hint: Option<&str>) -> Option<&str> {
    branch_hint.filter(|b| !b.is_empty())
}

/// Returns true when `branch` is the workspace's recorded default
/// branch. Detected lazily and cached per-process by
/// [`crate::server::default_branch::resolve_default_branch`].
///
/// Only called for branch-hinted (IDE) requests — non-IDE callers
/// pass `None` and bypass this entire check. On any resolver failure
/// (workspace not yet on disk, git lookup errored) we return `false`,
/// which routes the IDE request to FS. That matches the design
/// premise: a non-classifiable branch is closer to "user is editing
/// something we can't validate" than "the user is on main", and FS
/// reading the working copy is always safe. Returning `true` here
/// would serve the promoted main revision to a feature-branch user
/// whenever git resolution hiccups — the opposite of what
/// branch-aware reading is for.
async fn is_default_branch(db: &DatabaseConnection, workspace_id: Uuid, branch: &str) -> bool {
    match crate::server::default_branch::resolve_default_branch(db, workspace_id).await {
        Some(default) => branch == default,
        None => false,
    }
}

/// Pull `title` out of the compiled `definition` JSONB without a
/// full struct deserialize. The full strict parse happens at compile
/// time; runtime only wants the sidebar label.
fn extract_title(definition: &Value) -> Option<String> {
    definition
        .as_object()?
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Pull `llm.ref` out of the compiled agent `definition` JSONB —
/// matches the YAML-parsing helper the legacy `get_agents` handler
/// uses to surface "this agent needs a key for provider X" on the
/// home page.
fn extract_model_ref(definition: &Value) -> Option<String> {
    definition
        .as_object()?
        .get("llm")?
        .as_object()?
        .get("ref")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Pull the top-level `timezone` out of the compiled agent `definition`
/// JSONB — the compiler stores the full parsed YAML, so the agentic
/// config's `timezone:` field is a top-level key here. Surfaced so the
/// workspace clock can render local time instead of UTC.
fn extract_timezone(definition: &Value) -> Option<String> {
    definition
        .as_object()?
        .get("timezone")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_branch_hint_strips_empty() {
        // An empty `?branch=` from the FE must NOT trigger the non-default
        // branch fallthrough — empty means "use the workspace default".
        assert_eq!(normalize_branch_hint(None), None);
        assert_eq!(normalize_branch_hint(Some("")), None);
        assert_eq!(normalize_branch_hint(Some("main")), Some("main"));
        assert_eq!(normalize_branch_hint(Some("feat/x")), Some("feat/x"));
        // We don't trim whitespace; a literal space is technically a legal
        // git branch name, so we treat it as a real branch hint.
        assert_eq!(normalize_branch_hint(Some(" ")), Some(" "));
    }

    #[test]
    fn extract_timezone_reads_top_level_field() {
        use serde_json::json;
        // The compiler stores the full parsed YAML, so `timezone` is a
        // top-level key alongside `llm`.
        let def = json!({
            "name": "restaurant_analyst",
            "llm": { "ref": "claude-sonnet-4-6" },
            "timezone": "America/Los_Angeles"
        });
        assert_eq!(
            extract_timezone(&def),
            Some("America/Los_Angeles".to_string())
        );
        // Absent timezone → None (the frontend supplies the default).
        let no_tz = json!({ "name": "x", "llm": { "ref": "gpt-4o" } });
        assert_eq!(extract_timezone(&no_tz), None);
        // A non-string timezone is ignored rather than panicking.
        let bad = json!({ "timezone": 42 });
        assert_eq!(extract_timezone(&bad), None);
    }

    #[test]
    fn verified_query_integrity_detects_corruption() {
        let content = "SELECT 1 /* oxy: verified */";
        // Hash the body the same way `oxy_compile::compile_verified_query`
        // does, so a faithful round-trip verifies.
        let sha = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(content.as_bytes());
            h.finalize()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        };
        let good = CompiledVerifiedQuery {
            file_path: "example_sql/answer.sql".to_string(),
            content_sha256: sha,
            content: content.to_string(),
        };
        assert!(
            good.integrity_ok(),
            "a faithfully round-tripped body verifies"
        );

        // Same recorded hash, different bytes → corruption is caught. This
        // case is non-circular: the hash is fixed, only `content` changes.
        let corrupted = CompiledVerifiedQuery {
            content: "SELECT 2 /* tampered */".to_string(),
            ..good.clone()
        };
        assert!(
            !corrupted.integrity_ok(),
            "a body whose bytes no longer match the recorded hash is rejected"
        );
    }
}
