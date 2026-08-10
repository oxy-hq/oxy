//! Resolve airway admission config for one run.
//!
//! Two rows may apply: the **global** row for a source kind
//! (`workspace_id IS NULL`) and a **sparse override** for one workspace. They
//! merge field by field, narrowest non-null winning — see [`resolve_admission`].
//!
//! Lives here rather than in `agentic-airway` because that crate must not
//! depend on `entity`; this one already does.

use entity::airway_source_config;
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use uuid::Uuid;

/// The two admission strings for one run, as stored. `None` means "unset",
/// which `agentic_airway::AirwayAdmission::from_strings` reads as airway's own
/// default — `permissive` / `production`.
///
/// Deliberately **not** parsed here: the strings ride the durable queue payload
/// and are parsed at the worker, so a value that was valid at enqueue and
/// invalid after a downgrade fails the run loudly rather than at submit time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedAdmission {
    pub contract_policy: Option<String>,
    pub environment: Option<String>,
}

/// Merge the global and per-workspace rows for `source_kind`.
///
/// **Field by field, not row by row.** A workspace row setting only
/// `environment` inherits `contract_policy` from the global row; taking the
/// workspace row wholesale would reset the omitted field to airway's default,
/// which is a policy downgrade nobody requested.
///
/// An absent table, an absent kind, and an all-null row are the same answer:
/// `None` for both, i.e. today's behaviour.
pub async fn resolve_admission(
    db: &DatabaseConnection,
    source_kind: &str,
    workspace_id: Uuid,
) -> Result<ResolvedAdmission, DbErr> {
    // One query, both candidate rows: the partial unique indexes guarantee at
    // most one of each, so this cannot return more than two.
    let rows = airway_source_config::Entity::find()
        .filter(airway_source_config::Column::SourceKind.eq(source_kind))
        .filter(
            airway_source_config::Column::WorkspaceId
                .is_null()
                .or(airway_source_config::Column::WorkspaceId.eq(workspace_id)),
        )
        .all(db)
        .await?;

    // Defence in depth, and it should stay silent forever: the two partial
    // unique indexes make a duplicate unrepresentable. If one ever does appear
    // — an index dropped by hand, a migration that recreated the table without
    // them — `find` below picks whichever row the planner returned first, and
    // the effective policy becomes non-deterministic across queries. That is
    // the failure the indexes exist to prevent, so it must not resolve
    // quietly.
    warn_on_duplicates(&rows, source_kind, workspace_id);

    let global = rows.iter().find(|r| r.workspace_id.is_none());
    let scoped = rows.iter().find(|r| r.workspace_id == Some(workspace_id));

    let pick = |f: fn(&airway_source_config::Model) -> Option<String>| {
        scoped.and_then(f).or_else(|| global.and_then(f))
    };

    Ok(ResolvedAdmission {
        contract_policy: pick(|r| r.contract_policy.clone()),
        environment: pick(|r| r.environment.clone()),
    })
}

/// Warns, loudly and by name, if more than one row of either scope came back.
///
/// Split out so the resolver's happy path stays a straight read. Warn rather
/// than error: a duplicate means the resolved policy is arbitrary, not that it
/// is unsafe — refusing the run would convert a latent schema problem into an
/// outage, and the airway defaults the resolver falls back to are the
/// permissive ones the run would have had anyway.
fn warn_on_duplicates(rows: &[airway_source_config::Model], source_kind: &str, workspace_id: Uuid) {
    let globals = rows.iter().filter(|r| r.workspace_id.is_none()).count();
    if globals > 1 {
        tracing::warn!(
            source_kind,
            count = globals,
            "airway_source_config has {globals} global rows for source_kind `{source_kind}`; \
             airway_source_config_global_uniq should make this impossible. Resolving against an \
             arbitrary one — the effective admission policy for this kind is non-deterministic \
             until the duplicates are removed."
        );
    }

    let scoped = rows
        .iter()
        .filter(|r| r.workspace_id == Some(workspace_id))
        .count();
    if scoped > 1 {
        tracing::warn!(
            source_kind,
            %workspace_id,
            count = scoped,
            "airway_source_config has {scoped} rows for source_kind `{source_kind}` in workspace \
             {workspace_id}; airway_source_config_workspace_uniq should make this impossible. \
             Resolving against an arbitrary one — the effective admission policy for this \
             workspace is non-deterministic until the duplicates are removed."
        );
    }
}
