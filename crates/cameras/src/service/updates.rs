//! IoT Phase 2 — software-update channel service layer. This module owns:
//!
//! - `get_target_for_edge` — what the device's update agent reads
//!   on each poll. Returns target tag + current tag + held_until.
//! - `set_target` — operator sets a per-box target image via UI.
//! - `record_health_update` — extended health endpoint records
//!   `current_image_tag` + `last_update_result` so the operator UI
//!   can show "applied / reverted / in_progress" badges.
//!
//! Workspace-wide release channels (stable / beta / pinned) are
//! deliberately deferred — V1 ships per-box target only. Operators
//! who want a "stable channel" today do it by setting every box's
//! target to the same tag.

use crate::entities::{edge_boxes, sites};
use crate::service::{ServiceError, ServiceResult};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
    sea_query::Expr,
};
use std::collections::HashSet;
use uuid::Uuid;

/// Payload the device's update agent receives from
/// `GET /control/boxes/{id}/target`. The agent compares
/// `target_image_tag` against the container's running image and
/// decides whether to pull + restart.
#[derive(Debug, Clone)]
pub struct TargetView {
    /// Operator-set target. `None` means "no automated update — stay
    /// on whatever you're running."
    pub target_image_tag: Option<String>,
    /// Last self-reported running tag, for cross-check.
    pub current_image_tag: Option<String>,
    /// If set + in the future, the agent should *skip* this poll
    /// (the operator is holding the box during a busy hour).
    pub held_until: Option<DateTime<Utc>>,
}

/// Operator-facing setter. Touched by `PATCH
/// /{wid}/cameras/edge-boxes/{id}/target`. Empty-string target is
/// rejected to avoid accidentally pinning a box to "" — clear by
/// passing `None`.
pub async fn set_target(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    edge_box_id: Uuid,
    target_image_tag: Option<String>,
    held_until: Option<DateTime<Utc>>,
) -> ServiceResult<edge_boxes::Model> {
    if let Some(ref tag) = target_image_tag
        && tag.trim().is_empty()
    {
        return Err(ServiceError::InvalidInput(
            "target_image_tag must be non-empty (or null to clear)".into(),
        ));
    }

    let row = edge_box_owned_by_workspace(db, edge_box_id, workspace_id).await?;
    let mut am: edge_boxes::ActiveModel = row.into();
    am.target_image_tag = Set(target_image_tag);
    am.held_until = Set(held_until.map(|t| t.fixed_offset()));
    am.updated_at = Set(Utc::now().fixed_offset());
    Ok(am.update(db).await?)
}

/// Operator-set cohort label, used to scope canary rollouts. Empty
/// string is rejected — `"default"` is the canonical "no special
/// grouping" value. Cohort labels are free-form text, capped at 64
/// chars so they fit on the EdgeBoxes table without truncation.
pub async fn set_cohort(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    edge_box_id: Uuid,
    cohort: &str,
) -> ServiceResult<edge_boxes::Model> {
    let trimmed = cohort.trim();
    if trimmed.is_empty() {
        return Err(ServiceError::InvalidInput(
            "cohort must be non-empty; use 'default' for the catch-all group".into(),
        ));
    }
    if trimmed.len() > 64 {
        return Err(ServiceError::InvalidInput(
            "cohort must be at most 64 characters".into(),
        ));
    }
    let row = edge_box_owned_by_workspace(db, edge_box_id, workspace_id).await?;
    let mut am: edge_boxes::ActiveModel = row.into();
    am.cohort = Set(trimmed.to_string());
    am.updated_at = Set(Utc::now().fixed_offset());
    Ok(am.update(db).await?)
}

/// Read for the device polling `/control/boxes/{id}/target`. The
/// device extractor enforces "the bearer matches the URL box_id"
/// before this is called, so we trust the lookup here.
pub async fn get_target_for_edge(
    db: &DatabaseConnection,
    edge_box_id: Uuid,
) -> ServiceResult<TargetView> {
    let row = edge_boxes::Entity::find_by_id(edge_box_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    Ok(TargetView {
        target_image_tag: row.target_image_tag,
        current_image_tag: row.current_image_tag,
        held_until: row.held_until.map(|t| t.with_timezone(&Utc)),
    })
}

/// Record what the device's update agent reported after attempting
/// an update. Hooked into the existing `/control/boxes/{id}/health`
/// endpoint — the device sends `current_image_tag` on every health
/// ping anyway, plus an optional `last_update_result` when the
/// state changed since the previous tick.
///
/// Idempotent on repeated identical updates: we always write,
/// because `last_update_at` is part of the staleness signal the
/// UI uses to show "last attempt 3 min ago."
/// Env var the server reads on every `record_health_update` to
/// decide whether the worker's reported `protocol_version` is
/// new enough. Unset / `0` means "no enforcement" — operators
/// who haven't yet bumped the server-side floor get the legacy
/// behavior. See `internal-docs/edge-image-releases.md` for the
/// versioning convention.
const MIN_EDGE_PROTOCOL_ENV: &str = "OXY_MIN_EDGE_PROTOCOL_VERSION";

/// Compute the `incompatible_reason` value to persist for the given
/// posted compatibility manifest. Pure function so the rule is unit-
/// testable without touching the DB.
fn compute_incompatible_reason(compatibility: Option<&serde_json::Value>) -> Option<String> {
    let min_required = std::env::var(MIN_EDGE_PROTOCOL_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    if min_required == 0 {
        return None;
    }
    let manifest = compatibility?;
    let worker_pv = manifest.get("protocol_version").and_then(|v| v.as_u64())?;
    if worker_pv < min_required {
        Some(format!(
            "worker protocol_version {worker_pv} is below server minimum {min_required}"
        ))
    } else {
        None
    }
}

pub async fn record_health_update(
    db: &DatabaseConnection,
    edge_box_id: Uuid,
    current_image_tag: Option<String>,
    last_update_result: Option<String>,
    compatibility: Option<serde_json::Value>,
) -> ServiceResult<()> {
    let row = edge_boxes::Entity::find_by_id(edge_box_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let mut am: edge_boxes::ActiveModel = row.into();
    let now = Utc::now().fixed_offset();
    if current_image_tag.is_some() {
        am.current_image_tag = Set(current_image_tag);
    }
    if compatibility.is_some() {
        // Always re-evaluate compat when a manifest is posted — the
        // operator may have bumped OXY_MIN_EDGE_PROTOCOL_VERSION
        // since the last health post, and this is when we find out.
        let reason = compute_incompatible_reason(compatibility.as_ref());
        am.incompatible_reason = Set(reason);
        am.edge_compatibility_json = Set(compatibility);
    }
    if let Some(result) = last_update_result {
        // Allowed values mirror the edge agent's _rollback outcomes
        // (`update_agent.py::_rollback`). `stuck_at_broken_target`
        // and `revert_failed` are the honest signals that the
        // operator needs to look — the older code coerced both into
        // `reverted` and hid the failure.
        const ALLOWED: &[&str] = &[
            "applied",
            "reverted",
            "in_progress",
            "revert_failed",
            "stuck_at_broken_target",
        ];
        if !ALLOWED.contains(&result.as_str()) {
            return Err(ServiceError::InvalidInput(format!(
                "last_update_result must be one of {}, got {result}",
                ALLOWED.join(" / ")
            )));
        }
        am.last_update_result = Set(Some(result));
        am.last_update_at = Set(Some(now));
    }
    am.updated_at = Set(now);
    am.update(db).await?;
    Ok(())
}

/// Bulk variant of [`set_target`]: applies one target_image_tag +
/// held_until to every box in `box_ids` in a single transaction.
///
/// Atomicity matters: if any box belongs to another workspace we
/// reject the whole call with 403 before touching the DB. Doing
/// it partially would leak that workspace A's `box_id` exists
/// (the operator sees "10 boxes updated, 8 succeeded") AND would
/// leave the operator with an inconsistent rollout — half their
/// chain on the new image, half on the old.
///
/// Returns the number of rows actually touched. For a well-formed
/// caller this equals `box_ids.len()`, but a duplicate-id input
/// (de-duped via HashSet) or an empty list both return 0 with no
/// error.
pub async fn bulk_set_target(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    box_ids: &[Uuid],
    target_image_tag: Option<String>,
    held_until: Option<DateTime<Utc>>,
) -> ServiceResult<u64> {
    if let Some(ref tag) = target_image_tag
        && tag.trim().is_empty()
    {
        return Err(ServiceError::InvalidInput(
            "target_image_tag must be non-empty (or null to clear)".into(),
        ));
    }
    if box_ids.is_empty() {
        return Ok(0);
    }
    // De-dup so the count returned isn't inflated by a caller
    // passing the same id twice.
    let unique: HashSet<Uuid> = box_ids.iter().copied().collect();
    let unique_ids: Vec<Uuid> = unique.into_iter().collect();

    // Pre-flight: every requested box must (a) exist and (b) sit
    // under a site this workspace owns. Two queries instead of
    // one fancy join — cheaper to read and easier to debug.
    let rows = edge_boxes::Entity::find()
        .filter(edge_boxes::Column::Id.is_in(unique_ids.clone()))
        .all(db)
        .await?;
    if rows.len() != unique_ids.len() {
        return Err(ServiceError::NotFound);
    }
    let site_ids: HashSet<Uuid> = rows.iter().map(|r| r.site_id).collect();
    let allowed_sites: HashSet<Uuid> = sites::Entity::find()
        .filter(sites::Column::Id.is_in(site_ids.iter().copied().collect::<Vec<_>>()))
        .filter(sites::Column::WorkspaceId.eq(workspace_id))
        .select_only()
        .column(sites::Column::Id)
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect();
    if allowed_sites.len() != site_ids.len() {
        return Err(ServiceError::Forbidden(
            "one or more edge boxes belong to another workspace",
        ));
    }

    // Single UPDATE for all rows. update_many returns the affected
    // count which we hand back to the caller for the response body.
    let now = Utc::now().fixed_offset();
    let res = edge_boxes::Entity::update_many()
        .col_expr(
            edge_boxes::Column::TargetImageTag,
            Expr::value(target_image_tag),
        )
        .col_expr(
            edge_boxes::Column::HeldUntil,
            Expr::value(held_until.map(|t| t.fixed_offset())),
        )
        .col_expr(edge_boxes::Column::UpdatedAt, Expr::value(now))
        .filter(edge_boxes::Column::Id.is_in(unique_ids))
        .exec(db)
        .await?;
    Ok(res.rows_affected)
}

/// Defense-in-depth lookup ensuring the edge_box belongs to the
/// claimed workspace — same shape as the camera-side ownership
/// check in `service::config`. Without this, an operator in
/// workspace A could PATCH an edge box bound to workspace B's
/// site via a URL-guessed box_id.
async fn edge_box_owned_by_workspace(
    db: &DatabaseConnection,
    edge_box_id: Uuid,
    workspace_id: Uuid,
) -> ServiceResult<edge_boxes::Model> {
    use crate::entities::sites;
    let row = edge_boxes::Entity::find_by_id(edge_box_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let site = sites::Entity::find_by_id(row.site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden(
            "edge box belongs to another workspace",
        ));
    }
    Ok(row)
}

#[cfg(test)]
mod compat_tests {
    use super::*;
    use serde_json::json;
    use serial_test::serial;
    use std::sync::Mutex;

    // Env-mutating tests must serialize — set_var is global.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn with_min(min: Option<&str>, f: impl FnOnce()) {
        let _g = ENV_MUTEX.lock().unwrap();
        // SAFETY: serialized by ENV_MUTEX above.
        unsafe {
            match min {
                Some(v) => std::env::set_var(MIN_EDGE_PROTOCOL_ENV, v),
                None => std::env::remove_var(MIN_EDGE_PROTOCOL_ENV),
            }
        }
        f();
        unsafe { std::env::remove_var(MIN_EDGE_PROTOCOL_ENV) };
    }

    #[serial]
    #[test]
    fn returns_none_when_enforcement_disabled() {
        with_min(None, || {
            let m = json!({ "protocol_version": 1 });
            assert!(compute_incompatible_reason(Some(&m)).is_none());
        });
    }

    #[serial]
    #[test]
    fn returns_none_when_worker_at_min() {
        with_min(Some("2"), || {
            let m = json!({ "protocol_version": 2 });
            assert!(compute_incompatible_reason(Some(&m)).is_none());
        });
    }

    #[serial]
    #[test]
    fn returns_none_when_worker_above_min() {
        with_min(Some("2"), || {
            let m = json!({ "protocol_version": 3 });
            assert!(compute_incompatible_reason(Some(&m)).is_none());
        });
    }

    #[serial]
    #[test]
    fn flags_too_old_worker() {
        with_min(Some("3"), || {
            let m = json!({ "protocol_version": 1 });
            let reason = compute_incompatible_reason(Some(&m)).expect("flagged");
            assert!(reason.contains("1"));
            assert!(reason.contains("3"));
        });
    }

    #[serial]
    #[test]
    fn returns_none_when_no_manifest_posted() {
        with_min(Some("2"), || {
            assert!(compute_incompatible_reason(None).is_none());
        });
    }

    #[serial]
    #[test]
    fn returns_none_when_manifest_missing_protocol_version() {
        with_min(Some("2"), || {
            let m = json!({ "edge_version": "1.2.3" });
            // No protocol_version field — we can't decide, so don't flag.
            assert!(compute_incompatible_reason(Some(&m)).is_none());
        });
    }
}
