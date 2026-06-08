//! Canary-first OTA rollout plans.
//!
//! Operators define a plan: "ship `oxy-edge:v2.3.1` to cohort
//! `staging` first; if no reverts after 24h, promote to every other
//! box in the workspace." The [`spawn`] supervisor task runs every
//! 60s, advances plans through the state machine, and auto-aborts
//! when too many boxes in the canary cohort report `revert_failed`,
//! `stuck_at_broken_target`, or `reverted`.
//!
//! State machine:
//!
//! ```text
//!   pending  ──start──▶  canary  ──promote──▶  promoting  ──converge──▶  complete
//!                          │                       │
//!                          ▼                       ▼
//!                       aborted                 aborted
//! ```
//!
//! - **pending** — created by operator, supervisor hasn't picked it
//!   up yet. Usually a sub-second window before `start_canary` runs.
//! - **canary** — `target_image_tag` set on every box matching
//!   `canary_cohort`. Watching for `applied` (success) vs the three
//!   failure flavors. Auto-aborts when failures exceed threshold,
//!   auto-promotes when all canary boxes converge AND the soak
//!   window has elapsed.
//! - **promoting** — same target_image_tag now set on every other
//!   non-canary box in the workspace. Watching for the same signals.
//! - **complete** — every targeted box converged on the target tag.
//! - **aborted** — either operator hit "stop" or the failure
//!   threshold tripped. We do NOT auto-rollback the boxes that
//!   already converged; the operator can launch a separate plan
//!   targeting the previous tag if they want that.

use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set, sea_query::Expr,
};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::entities::{edge_boxes, rollout_plans, sites};
use crate::service::{ServiceError, ServiceResult, updates};

pub const SUPERVISOR_TICK_INTERVAL: Duration = Duration::from_secs(60);

/// Failure result strings the worker can post via
/// `record_health_update`. Counting these against a canary cohort
/// is what trips the auto-abort.
const FAILURE_RESULTS: &[&str] = &["reverted", "revert_failed", "stuck_at_broken_target"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Pending,
    Canary,
    Promoting,
    Complete,
    Aborted,
}

impl PlanStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Canary => "canary",
            Self::Promoting => "promoting",
            Self::Complete => "complete",
            Self::Aborted => "aborted",
        }
    }

    fn parse(raw: &str) -> ServiceResult<Self> {
        match raw {
            "pending" => Ok(Self::Pending),
            "canary" => Ok(Self::Canary),
            "promoting" => Ok(Self::Promoting),
            "complete" => Ok(Self::Complete),
            "aborted" => Ok(Self::Aborted),
            other => Err(ServiceError::Internal(format!(
                "unknown rollout plan status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreatePlanInput {
    pub workspace_id: Uuid,
    pub target_image_tag: String,
    pub canary_cohort: String,
    /// Default 24h. Minimum soak before auto-promote.
    pub canary_min_minutes: i32,
    /// Default 5.0. Percent of canary boxes reporting failure that
    /// will trip auto-abort.
    pub failure_threshold_pct: f32,
    pub actor_user_id: Option<Uuid>,
}

pub async fn create_plan(
    db: &DatabaseConnection,
    input: CreatePlanInput,
) -> ServiceResult<rollout_plans::Model> {
    if input.target_image_tag.trim().is_empty() {
        return Err(ServiceError::InvalidInput(
            "target_image_tag must be non-empty".into(),
        ));
    }
    if input.canary_cohort.trim().is_empty() {
        return Err(ServiceError::InvalidInput(
            "canary_cohort must be non-empty".into(),
        ));
    }
    if input.canary_min_minutes < 0 {
        return Err(ServiceError::InvalidInput(
            "canary_min_minutes must be non-negative".into(),
        ));
    }
    if !(0.0..=100.0).contains(&input.failure_threshold_pct) {
        return Err(ServiceError::InvalidInput(
            "failure_threshold_pct must be between 0 and 100".into(),
        ));
    }

    let now = Utc::now().fixed_offset();
    let id = Uuid::new_v4();
    let am = rollout_plans::ActiveModel {
        id: Set(id),
        workspace_id: Set(input.workspace_id),
        target_image_tag: Set(input.target_image_tag),
        canary_cohort: Set(input.canary_cohort),
        canary_min_minutes: Set(input.canary_min_minutes),
        failure_threshold_pct: Set(input.failure_threshold_pct),
        status: Set(PlanStatus::Pending.as_str().into()),
        canary_started_at: Set(None),
        promotion_started_at: Set(None),
        completed_at: Set(None),
        aborted_at: Set(None),
        abort_reason: Set(None),
        actor_user_id: Set(input.actor_user_id),
        created_at: Set(now),
        updated_at: Set(now),
    };
    let model = am.insert(db).await?;
    info!(
        plan_id = %model.id,
        workspace_id = %model.workspace_id,
        target = %model.target_image_tag,
        cohort = %model.canary_cohort,
        "rollouts.created"
    );
    Ok(model)
}

pub async fn list_plans(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<rollout_plans::Model>> {
    Ok(rollout_plans::Entity::find()
        .filter(rollout_plans::Column::WorkspaceId.eq(workspace_id))
        .order_by_desc(rollout_plans::Column::CreatedAt)
        .all(db)
        .await?)
}

/// Per-box convergence row used by the rollout detail page. Lets
/// the operator answer "which boxes failed?" without manually
/// cross-referencing the EdgeBoxes list against timestamps.
#[derive(Debug, Clone, Serialize)]
pub struct BoxConvergenceRow {
    pub edge_box_id: Uuid,
    pub hardware_model: String,
    pub cohort: String,
    pub site_name: String,
    pub current_image_tag: Option<String>,
    pub last_update_result: Option<String>,
    pub last_update_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// True when this box matched the plan's canary cohort at apply
    /// time. Today derived from `cohort == plan.canary_cohort` — a
    /// box that operator-moved out of the cohort post-apply would
    /// still get `in_canary = false` here, which matches the
    /// operator's mental model ("the apply already went out").
    pub in_canary: bool,
    /// Coarse per-box state derived for the UI:
    ///   - "converged" — current_image_tag matches plan.target_image_tag
    ///   - "failed"    — last_update_result is one of the failure
    ///                   results AND last_update_at >= plan.canary_started_at
    ///   - "pending"   — apply went out but the worker hasn't reported
    ///                   convergence or a failure yet
    ///   - "untargeted" — the apply hasn't reached this box yet
    ///                   (promoting phase, non-canary box)
    pub convergence_status: String,
}

pub async fn plan_convergence(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    plan_id: Uuid,
) -> ServiceResult<Vec<BoxConvergenceRow>> {
    let plan = load_plan(db, workspace_id, plan_id).await?;
    let started_at = plan
        .canary_started_at
        .or(plan.promotion_started_at)
        .or(Some(plan.created_at));

    let site_ids = workspace_site_ids(db, workspace_id).await?;
    if site_ids.is_empty() {
        return Ok(Vec::new());
    }
    let site_rows = sites::Entity::find()
        .filter(sites::Column::Id.is_in(site_ids.clone()))
        .all(db)
        .await?;
    let site_name_by_id: std::collections::HashMap<Uuid, String> =
        site_rows.into_iter().map(|s| (s.id, s.name)).collect();

    let boxes = edge_boxes::Entity::find()
        .filter(edge_boxes::Column::SiteId.is_in(site_ids))
        .all(db)
        .await?;

    let promoting_or_later = matches!(
        PlanStatus::parse(&plan.status)?,
        PlanStatus::Promoting | PlanStatus::Complete
    );

    let mut out = Vec::with_capacity(boxes.len());
    for b in boxes {
        let in_canary = b.cohort == plan.canary_cohort;
        // A box was "targeted" by the apply if it's in the canary
        // cohort (canary phase already ran) OR the plan reached the
        // promoting phase (everyone's targeted at that point).
        let targeted = in_canary || promoting_or_later;
        let status = if !targeted {
            "untargeted".to_string()
        } else if b.current_image_tag.as_deref() == Some(plan.target_image_tag.as_str()) {
            "converged".to_string()
        } else if let (Some(result), Some(at), Some(since)) = (
            b.last_update_result.as_deref(),
            b.last_update_at,
            started_at,
        ) {
            if at >= since && FAILURE_RESULTS.contains(&result) {
                "failed".to_string()
            } else {
                "pending".to_string()
            }
        } else {
            "pending".to_string()
        };

        out.push(BoxConvergenceRow {
            edge_box_id: b.id,
            hardware_model: b.hardware_model,
            cohort: b.cohort,
            site_name: site_name_by_id.get(&b.site_id).cloned().unwrap_or_default(),
            current_image_tag: b.current_image_tag,
            last_update_result: b.last_update_result,
            last_update_at: b.last_update_at,
            in_canary,
            convergence_status: status,
        });
    }
    // Canary rows first (operator's primary attention), then by site
    // for stable ordering across refetches.
    out.sort_by(|a, b| {
        b.in_canary
            .cmp(&a.in_canary)
            .then_with(|| a.site_name.cmp(&b.site_name))
            .then_with(|| a.hardware_model.cmp(&b.hardware_model))
    });
    Ok(out)
}

pub async fn abort_plan(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    plan_id: Uuid,
    reason: &str,
) -> ServiceResult<rollout_plans::Model> {
    let plan = load_plan(db, workspace_id, plan_id).await?;
    let status = PlanStatus::parse(&plan.status)?;
    if matches!(status, PlanStatus::Complete | PlanStatus::Aborted) {
        return Err(ServiceError::Conflict(format!(
            "plan is already terminal ({})",
            status.as_str()
        )));
    }
    let now = Utc::now().fixed_offset();
    let mut am: rollout_plans::ActiveModel = plan.into();
    am.status = Set(PlanStatus::Aborted.as_str().into());
    am.aborted_at = Set(Some(now));
    am.abort_reason = Set(Some(reason.into()));
    am.updated_at = Set(now);
    let updated = am.update(db).await?;
    info!(
        plan_id = %updated.id,
        reason,
        "rollouts.aborted"
    );
    Ok(updated)
}

async fn load_plan(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    plan_id: Uuid,
) -> ServiceResult<rollout_plans::Model> {
    let plan = rollout_plans::Entity::find_by_id(plan_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if plan.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden(
            "rollout plan belongs to another workspace",
        ));
    }
    Ok(plan)
}

// ── Supervisor loop ─────────────────────────────────────────────────────────

/// Spawn the rollout supervisor on the current Tokio runtime.
/// The loop exits when `shutdown` is cancelled, matching the other
/// process-level background jobs (`stale::spawn`, `log_retention::spawn`).
pub fn spawn(db: DatabaseConnection, shutdown: CancellationToken) {
    tokio::spawn(async move {
        info!(
            tick_secs = SUPERVISOR_TICK_INTERVAL.as_secs(),
            "cameras.rollout_supervisor: started"
        );
        let mut ticker = tokio::time::interval(SUPERVISOR_TICK_INTERVAL);
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    info!("cameras.rollout_supervisor: shutdown");
                    return;
                }
                _ = ticker.tick() => {
                    if let Err(e) = supervisor_tick(&db).await {
                        warn!(error = %e, "cameras.rollout_supervisor: tick failed");
                    }
                }
            }
        }
    });
}

/// One pass of the supervisor. Public so tests can drive it without
/// the ticker.
pub async fn supervisor_tick(db: &DatabaseConnection) -> ServiceResult<()> {
    let active = rollout_plans::Entity::find()
        .filter(
            Condition::any()
                .add(rollout_plans::Column::Status.eq(PlanStatus::Pending.as_str()))
                .add(rollout_plans::Column::Status.eq(PlanStatus::Canary.as_str()))
                .add(rollout_plans::Column::Status.eq(PlanStatus::Promoting.as_str())),
        )
        .all(db)
        .await?;

    for plan in active {
        if let Err(e) = advance_plan(db, plan).await {
            warn!(error = %e, "rollouts.advance_failed");
        }
    }
    Ok(())
}

async fn advance_plan(db: &DatabaseConnection, plan: rollout_plans::Model) -> ServiceResult<()> {
    let status = PlanStatus::parse(&plan.status)?;
    match status {
        PlanStatus::Pending => start_canary(db, plan).await,
        PlanStatus::Canary => check_canary(db, plan).await,
        PlanStatus::Promoting => check_promotion(db, plan).await,
        PlanStatus::Complete | PlanStatus::Aborted => Ok(()),
    }
}

async fn start_canary(db: &DatabaseConnection, plan: rollout_plans::Model) -> ServiceResult<()> {
    let canary_boxes = fetch_canary_box_ids(db, &plan).await?;
    if canary_boxes.is_empty() {
        // No boxes in this cohort — operator likely typo'd. Abort
        // with a clear reason rather than silently advancing to
        // promoting (which would skip the canary entirely).
        let reason = format!("no edge boxes match cohort '{}'", plan.canary_cohort);
        finalize_aborted(db, plan, &reason).await?;
        return Ok(());
    }
    updates::bulk_set_target(
        db,
        plan.workspace_id,
        &canary_boxes,
        Some(plan.target_image_tag.clone()),
        None,
    )
    .await?;
    let now = Utc::now().fixed_offset();
    let mut am: rollout_plans::ActiveModel = plan.into();
    am.status = Set(PlanStatus::Canary.as_str().into());
    am.canary_started_at = Set(Some(now));
    am.updated_at = Set(now);
    let updated = am.update(db).await?;
    info!(
        plan_id = %updated.id,
        canary_box_count = canary_boxes.len(),
        "rollouts.canary_started"
    );
    Ok(())
}

async fn check_canary(db: &DatabaseConnection, plan: rollout_plans::Model) -> ServiceResult<()> {
    let cohort_boxes = fetch_cohort_boxes(db, plan.workspace_id, &plan.canary_cohort).await?;
    let started_at = plan
        .canary_started_at
        .ok_or_else(|| ServiceError::Internal("canary in flight without started_at".into()))?;

    let metrics = cohort_metrics(&cohort_boxes, &plan.target_image_tag, started_at);

    // Auto-abort if too many failures during the soak window.
    let failure_pct = metrics.failure_pct();
    if failure_pct > plan.failure_threshold_pct as f64 {
        let reason = format!(
            "canary failure rate {:.1}% exceeded threshold {:.1}%",
            failure_pct, plan.failure_threshold_pct
        );
        finalize_aborted(db, plan, &reason).await?;
        return Ok(());
    }

    // Auto-promote when: every canary box converged AND soak window elapsed.
    let soak_elapsed_minutes =
        (Utc::now().with_timezone(&started_at.timezone()) - started_at).num_minutes();
    let soak_done = soak_elapsed_minutes >= plan.canary_min_minutes as i64;
    if metrics.applied == metrics.total && soak_done {
        promote_to_full(db, plan).await?;
    }
    Ok(())
}

async fn promote_to_full(db: &DatabaseConnection, plan: rollout_plans::Model) -> ServiceResult<()> {
    let remaining = fetch_non_canary_box_ids(db, &plan).await?;
    if !remaining.is_empty() {
        updates::bulk_set_target(
            db,
            plan.workspace_id,
            &remaining,
            Some(plan.target_image_tag.clone()),
            None,
        )
        .await?;
    }
    let now = Utc::now().fixed_offset();
    let mut am: rollout_plans::ActiveModel = plan.into();
    am.status = Set(PlanStatus::Promoting.as_str().into());
    am.promotion_started_at = Set(Some(now));
    am.updated_at = Set(now);
    let updated = am.update(db).await?;
    info!(
        plan_id = %updated.id,
        remaining_box_count = remaining.len(),
        "rollouts.promoted_to_full"
    );
    // If there were no non-canary boxes, the plan is effectively complete
    // immediately; the next supervisor tick will detect convergence and finalize.
    Ok(())
}

async fn check_promotion(db: &DatabaseConnection, plan: rollout_plans::Model) -> ServiceResult<()> {
    let all_boxes = fetch_all_workspace_box_ids(db, plan.workspace_id).await?;
    let rows = edge_boxes::Entity::find()
        .filter(edge_boxes::Column::Id.is_in(all_boxes.clone()))
        .all(db)
        .await?;
    let started_at = plan
        .promotion_started_at
        .or(plan.canary_started_at)
        .ok_or_else(|| ServiceError::Internal("promoting plan with no start timestamps".into()))?;
    let metrics = cohort_metrics(&rows, &plan.target_image_tag, started_at);

    let failure_pct = metrics.failure_pct();
    if failure_pct > plan.failure_threshold_pct as f64 {
        let reason = format!(
            "promotion failure rate {:.1}% exceeded threshold {:.1}%",
            failure_pct, plan.failure_threshold_pct
        );
        finalize_aborted(db, plan, &reason).await?;
        return Ok(());
    }

    if metrics.applied == metrics.total {
        let now = Utc::now().fixed_offset();
        let mut am: rollout_plans::ActiveModel = plan.into();
        am.status = Set(PlanStatus::Complete.as_str().into());
        am.completed_at = Set(Some(now));
        am.updated_at = Set(now);
        let updated = am.update(db).await?;
        info!(plan_id = %updated.id, "rollouts.complete");
    }
    Ok(())
}

async fn finalize_aborted(
    db: &DatabaseConnection,
    plan: rollout_plans::Model,
    reason: &str,
) -> ServiceResult<rollout_plans::Model> {
    let now = Utc::now().fixed_offset();
    let mut am: rollout_plans::ActiveModel = plan.into();
    am.status = Set(PlanStatus::Aborted.as_str().into());
    am.aborted_at = Set(Some(now));
    am.abort_reason = Set(Some(reason.into()));
    am.updated_at = Set(now);
    let updated = am.update(db).await?;
    warn!(
        plan_id = %updated.id,
        reason,
        "rollouts.auto_aborted"
    );
    Ok(updated)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct CohortMetrics {
    total: usize,
    applied: usize,
    failed: usize,
}

impl CohortMetrics {
    fn failure_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.failed as f64 / self.total as f64) * 100.0
    }
}

fn cohort_metrics(
    rows: &[edge_boxes::Model],
    target: &str,
    started_at: DateTime<FixedOffset>,
) -> CohortMetrics {
    let mut applied = 0;
    let mut failed = 0;
    for row in rows {
        if row.current_image_tag.as_deref() == Some(target) {
            applied += 1;
            continue;
        }
        if let (Some(result), Some(at)) = (row.last_update_result.as_deref(), row.last_update_at)
            && at >= started_at
            && FAILURE_RESULTS.contains(&result)
        {
            failed += 1;
        }
    }
    CohortMetrics {
        total: rows.len(),
        applied,
        failed,
    }
}

async fn fetch_canary_box_ids(
    db: &DatabaseConnection,
    plan: &rollout_plans::Model,
) -> ServiceResult<Vec<Uuid>> {
    let rows = fetch_cohort_boxes(db, plan.workspace_id, &plan.canary_cohort).await?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

async fn fetch_non_canary_box_ids(
    db: &DatabaseConnection,
    plan: &rollout_plans::Model,
) -> ServiceResult<Vec<Uuid>> {
    let site_ids = workspace_site_ids(db, plan.workspace_id).await?;
    if site_ids.is_empty() {
        return Ok(Vec::new());
    }
    // Retired boxes are excluded everywhere in the rollout flow —
    // pushing a target tag to a soft-deleted box would never
    // converge, blocking the supervisor's auto-promote forever.
    let rows = edge_boxes::Entity::find()
        .filter(edge_boxes::Column::SiteId.is_in(site_ids))
        .filter(edge_boxes::Column::Cohort.ne(plan.canary_cohort.clone()))
        .filter(edge_boxes::Column::Status.ne("retired"))
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

async fn fetch_all_workspace_box_ids(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<Uuid>> {
    let site_ids = workspace_site_ids(db, workspace_id).await?;
    if site_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = edge_boxes::Entity::find()
        .filter(edge_boxes::Column::SiteId.is_in(site_ids))
        .filter(edge_boxes::Column::Status.ne("retired"))
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|r| r.id).collect())
}

async fn fetch_cohort_boxes(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    cohort: &str,
) -> ServiceResult<Vec<edge_boxes::Model>> {
    let site_ids = workspace_site_ids(db, workspace_id).await?;
    if site_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(edge_boxes::Entity::find()
        .filter(edge_boxes::Column::SiteId.is_in(site_ids))
        .filter(edge_boxes::Column::Cohort.eq(cohort))
        .filter(edge_boxes::Column::Status.ne("retired"))
        .all(db)
        .await?)
}

async fn workspace_site_ids(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<Uuid>> {
    Ok(sites::Entity::find()
        .filter(sites::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await?
        .into_iter()
        .map(|s| s.id)
        .collect())
}

fn plan_cohort_str(plan: &rollout_plans::Model) -> &str {
    &plan.canary_cohort
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_row(
        id: Uuid,
        current: Option<&str>,
        result: Option<&str>,
        at_ago_secs: i64,
    ) -> edge_boxes::Model {
        let now: DateTime<FixedOffset> = Utc::now().fixed_offset();
        let at = if at_ago_secs >= 0 {
            Some(now - chrono::Duration::seconds(at_ago_secs))
        } else {
            Some(now + chrono::Duration::seconds(-at_ago_secs))
        };
        edge_boxes::Model {
            id,
            site_id: Uuid::nil(),
            hardware_model: "test".into(),
            image_tag: "old".into(),
            cohort: "default".into(),
            tailscale_ip: None,
            funnel_hostname: None,
            status: "active".into(),
            unifi_console_id: None,
            unifi_public_ip: None,
            unifi_rtsp_reachable: false,
            registered_at: now,
            last_seen_at: Some(now),
            bandwidth_5min_bytes: None,
            bandwidth_reported_at: None,
            target_image_tag: None,
            current_image_tag: current.map(String::from),
            held_until: None,
            last_update_result: result.map(String::from),
            last_update_at: at,
            edge_compatibility_json: None,
            incompatible_reason: None,
            updated_at: now,
        }
    }

    #[test]
    fn cohort_metrics_counts_applied_failed() {
        let started = Utc::now().fixed_offset() - chrono::Duration::seconds(120);
        let rows = vec![
            fake_row(Uuid::new_v4(), Some("v2.3"), None, 0),
            fake_row(Uuid::new_v4(), Some("v2.3"), None, 0),
            fake_row(Uuid::new_v4(), Some("v2.2"), Some("revert_failed"), 30),
            fake_row(Uuid::new_v4(), Some("v2.2"), Some("reverted"), 30),
            fake_row(Uuid::new_v4(), Some("v2.2"), None, 0), // still pulling
        ];
        let m = cohort_metrics(&rows, "v2.3", started);
        assert_eq!(m.total, 5);
        assert_eq!(m.applied, 2);
        assert_eq!(m.failed, 2);
        assert!((m.failure_pct() - 40.0).abs() < 0.01);
    }

    #[test]
    fn cohort_metrics_ignores_pre_canary_failures() {
        // A box that reported `reverted` BEFORE the canary started
        // is stale signal — it's about a previous rollout, not this one.
        let started = Utc::now().fixed_offset();
        let rows = vec![
            // last_update_at is 3600s ago, before `started` — should NOT count.
            fake_row(Uuid::new_v4(), Some("v2.2"), Some("revert_failed"), 3600),
        ];
        let m = cohort_metrics(&rows, "v2.3", started);
        assert_eq!(m.failed, 0);
    }

    #[test]
    fn plan_status_round_trips() {
        for s in [
            PlanStatus::Pending,
            PlanStatus::Canary,
            PlanStatus::Promoting,
            PlanStatus::Complete,
            PlanStatus::Aborted,
        ] {
            assert_eq!(PlanStatus::parse(s.as_str()).unwrap(), s);
        }
        assert!(PlanStatus::parse("bogus").is_err());
    }
}
