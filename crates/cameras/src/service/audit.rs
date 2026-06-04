//! Operator-action audit log (Tier B #7).
//!
//! Every mutating operator endpoint records a row here so the UI
//! can answer "who set what target, who claimed which device,
//! when." Failures to record are **logged but never propagated**
//! — a transient audit write failure must not break the user-
//! facing action it accompanies. The trade-off is that the audit
//! log is best-effort, not authoritative; in exchange we don't
//! couple Postgres availability of the audit table to the
//! availability of every PATCH/POST on the camera surface.
//!
//! ## Action key convention
//!
//! `<aggregate>.<verb>` in snake_case. Examples:
//!
//!   - `edge_box.register`
//!   - `edge_box.set_target`
//!   - `edge_box.bulk_set_target`
//!   - `device.claim`
//!   - `device.bind_existing`
//!   - `pack.update`
//!
//! Adding a new action is free — the column is TEXT, no enum to
//! migrate. Keep the keys grep-friendly: operators search the
//! Audit tab for "who touched box X" and we don't want fuzzy
//! matching to be the first line of defense.

use crate::entities::audit_events;
use crate::service::{ServiceError, ServiceResult};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

// ── Canonical action keys ──────────────────────────────────────────────────
//
// Keep this list in sync with what the route handlers actually
// log. Centralizing as constants avoids typos like
// `edge_box.set_target` vs `edgebox.set_target` becoming two
// different audit timelines for the same action.

pub const ACTION_SET_TARGET: &str = "edge_box.set_target";
pub const ACTION_BULK_SET_TARGET: &str = "edge_box.bulk_set_target";
pub const ACTION_SET_COHORT: &str = "edge_box.set_cohort";
pub const ACTION_BIND_EXISTING: &str = "device.bind_existing";
pub const ACTION_PACK_UPDATE: &str = "pack.update";
/// Camera health transition detected by `service::alerts::alerter_tick`.
/// Written for every transition that would have generated a Slack
/// message, regardless of whether Slack delivery actually happened —
/// the Dashboard's "Recent alerts" feed reads these so it works even
/// when no Slack install is configured.
pub const ACTION_CAMERA_ALERT: &str = "camera.alert";

/// Record one audit event. Logs + swallows any DB error — the
/// caller's response is unaffected by a failed audit write.
///
/// `actor_user_id` is `None` when the route layer wasn't able
/// to resolve a user (e.g. local-mode deployments where every
/// request is unauthenticated). NULL on the column then.
pub async fn record(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    actor_user_id: Option<Uuid>,
    action: &str,
    target_id: Option<Uuid>,
    details: Value,
) {
    let row = audit_events::ActiveModel {
        id: Set(Uuid::new_v4()),
        workspace_id: Set(workspace_id),
        actor_user_id: Set(actor_user_id),
        action: Set(action.to_string()),
        target_id: Set(target_id),
        details_json: Set(details),
        created_at: Set(chrono::Utc::now().fixed_offset()),
    };
    if let Err(e) = row.insert(db).await {
        tracing::warn!(
            workspace_id = %workspace_id,
            actor_user_id = ?actor_user_id,
            action,
            target_id = ?target_id,
            error = %e,
            "audit: failed to record event"
        );
    }
}

/// Filters for the operator-facing list endpoint.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// Only events with this action prefix. Substring or equal
    /// match (we use `LIKE 'prefix%'`). Useful for "show me all
    /// device.* events."
    pub action_prefix: Option<String>,
    /// Only events touching this target. Optional — when None,
    /// returns everything in the workspace.
    pub target_id: Option<Uuid>,
    /// Page size. Server clamps to a max so the UI can't request
    /// 100k rows by accident.
    pub limit: Option<u32>,
    /// Offset into the most-recent-first order. We deliberately
    /// don't paginate by `created_at` cursor here — the column
    /// has a millisecond clock, which is plenty of resolution
    /// for V1 fleet sizes, but if two events ever collide on the
    /// same ms the operator gets stable order via the row id
    /// fallback.
    pub offset: Option<u32>,
}

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 500;

/// One row as the UI sees it. Shape mirrors the entity model
/// because the audit tab renders columns 1:1 — adding a UI-only
/// field here means a new column in the table; resist unless
/// it's load-bearing.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEventRow {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub target_id: Option<Uuid>,
    pub details_json: Value,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<audit_events::Model> for AuditEventRow {
    fn from(m: audit_events::Model) -> Self {
        Self {
            id: m.id,
            actor_user_id: m.actor_user_id,
            action: m.action,
            target_id: m.target_id,
            details_json: m.details_json,
            created_at: m.created_at,
        }
    }
}

/// List recent audit events for a workspace, newest first.
pub async fn list_for_workspace(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    filter: ListFilter,
) -> ServiceResult<Vec<AuditEventRow>> {
    let limit = filter.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as u64;
    let offset = filter.offset.unwrap_or(0) as u64;

    let mut q =
        audit_events::Entity::find().filter(audit_events::Column::WorkspaceId.eq(workspace_id));
    if let Some(prefix) = filter.action_prefix.as_deref().filter(|s| !s.is_empty()) {
        // Trailing wildcard only — operators search by aggregate
        // (`device.`) or by exact action; a leading wildcard
        // forces a full table scan and isn't a UX we need.
        let pattern = format!("{}%", prefix.replace('%', "\\%").replace('_', "\\_"));
        q = q.filter(audit_events::Column::Action.like(&pattern));
    }
    if let Some(tid) = filter.target_id {
        q = q.filter(audit_events::Column::TargetId.eq(tid));
    }

    let rows = q
        .order_by_desc(audit_events::Column::CreatedAt)
        .order_by_desc(audit_events::Column::Id)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
        .map_err(ServiceError::from)?;
    Ok(rows.into_iter().map(AuditEventRow::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_keys_are_dot_separated_snake() {
        for key in [
            ACTION_SET_TARGET,
            ACTION_BULK_SET_TARGET,
            ACTION_SET_COHORT,
            ACTION_BIND_EXISTING,
            ACTION_PACK_UPDATE,
            ACTION_CAMERA_ALERT,
        ] {
            assert!(key.contains('.'), "{key} missing dot separator");
            assert!(
                key.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'),
                "{key} not snake_case"
            );
        }
    }
}
