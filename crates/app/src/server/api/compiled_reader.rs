//! Hybrid Postgres-first reader for endpoints migrating onto the
//! compile boundary. Every public fn returns:
//!
//!   * `Ok(Some(...))` — workspace is promoted on its default branch
//!                       and the kill switch is off; here are the rows.
//!   * `Ok(None)`       — caller should fall through to the legacy FS
//!                       path. Reasons: kill-switch feature flag is on,
//!                       branch is non-default (IDE draft mode), no
//!                       promoted revision exists yet, or a transient
//!                       DB error.
//!   * `Err(...)`       — programmer-error DB failure surfaced from a
//!                       successful `current_revision_id` lookup (rare).
//!                       Connect failures are intentionally downgraded
//!                       to `Ok(None)` so a Postgres hiccup can never
//!                       blank the IDE sidebar.
//!
//! Resolution order on every call:
//!
//!   1. If `compile_boundary_disabled` feature flag is on → `Ok(None)`.
//!   2. If `branch_hint` is `Some(name)` and that name is not the
//!      workspace's default branch → `Ok(None)`. Non-default branches
//!      are by definition drafts; the FS working copy is freshest.
//!   3. Read `workspaces.current_revision_id`. If null → `Ok(None)`.
//!   4. Query the per-entity table keyed by that revision_id.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::Value;
use uuid::Uuid;

use crate::server::feature_flags;

/// Feature-flag key whose presence (when enabled) disables the entire
/// hybrid reader. Mirror of the registry entry.
const KILL_SWITCH_FLAG: &str = "compile_boundary_disabled";

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
/// readiness gaps against the agent the chat will actually use.
#[derive(Debug, Clone)]
pub struct CompiledAgent {
    pub file_path: String,
    pub name: String,
    pub model_ref: Option<String>,
}

/// Row shape for the procedure listing. The legacy extensions
/// (`.procedure.yml`, `.workflow.yml`, `.automation.yml`) are
/// preserved on the row so the file-tree grouping can show them.
#[derive(Debug, Clone)]
pub struct CompiledProcedure {
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

/// Return `Ok(Some(apps))` when the workspace has a promoted revision
/// on its default branch and the kill switch is off; `Ok(None)`
/// otherwise (caller falls through to FS).
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
                name: m.name,
            })
            .collect(),
    ))
}

/// Listing equivalent for the workflow / procedure file enumeration.
pub async fn list_procedures(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<Vec<CompiledProcedure>>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let rows = entity::procedure_definitions::Entity::find()
        .filter(entity::procedure_definitions::Column::RevisionId.eq(revision_id))
        .all(&db)
        .await?;
    Ok(Some(
        rows.into_iter()
            .map(|m| CompiledProcedure {
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

/// Single-procedure resolver, keyed by `file_path` (the PK column).
pub async fn resolve_procedure(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
    file_path: &str,
) -> Result<Option<CompiledArtifact>, sea_orm::DbErr> {
    let Some((db, revision_id)) = open_compiled_revision(workspace_id, branch_hint).await? else {
        return Ok(None);
    };
    let row =
        entity::procedure_definitions::Entity::find_by_id((revision_id, file_path.to_string()))
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
    let row = entity::workspace_compiled_configs::Entity::find_by_id(revision_id)
        .one(&db)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };

    // Rebuild the original config.yml top-level object by merging the
    // split columns. Start from `other` (catch-all for unrecognised
    // keys) and layer the typed columns on top, preserving the
    // canonical ordering downstream readers might rely on.
    let mut merged = match row.other {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    merged.insert("databases".into(), row.databases);
    if let Some(v) = row.models {
        merged.insert("models".into(), v);
    }
    if let Some(v) = row.integrations {
        merged.insert("integrations".into(), v);
    }
    if let Some(v) = row.repositories {
        merged.insert("repositories".into(), v);
    }
    if let Some(v) = row.builder_agent {
        merged.insert("builder_agent".into(), v);
    }
    if let Some(v) = row.mcp {
        merged.insert("mcp".into(), v);
    }
    Ok(Some(Value::Object(merged)))
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

/// Shared gate consulted at the top of every public reader. Returns
/// `Some((db, revision_id))` when the request should be served from
/// Postgres, `None` when the caller should fall through to FS.
///
/// Returns `Ok(None)` (not `Err`) for kill switch on, non-default
/// branch, DB connect failure, or workspace-not-promoted. We never
/// want the IDE sidebar to blank because the compile boundary is
/// unhappy — FS is always the safety net.
async fn open_compiled_revision(
    workspace_id: Uuid,
    branch_hint: Option<&str>,
) -> Result<Option<(DatabaseConnection, Uuid)>, sea_orm::DbErr> {
    if feature_flags::is_enabled(KILL_SWITCH_FLAG) {
        tracing::debug!(
            workspace_id = %workspace_id,
            "compiled_reader: kill switch on — using FS"
        );
        return Ok(None);
    }
    let db = match oxy::database::client::establish_connection().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(?e, "compiled_reader: DB connect failed; falling through to FS");
            return Ok(None);
        }
    };
    if let Some(branch) = branch_hint
        && !is_default_branch(&db, workspace_id, branch).await
    {
        tracing::debug!(
            workspace_id = %workspace_id,
            branch,
            "compiled_reader: non-default branch — using FS"
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
