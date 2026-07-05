//! Cross-tenant workspace-health sweep — the periodic eval pass driven by the
//! `health_eval` schedule. Gathers signals, evaluates each workspace, diffs the
//! result against the last-known state, pushes Slack on transitions, and
//! upserts `workspace_health_state`.

use sea_orm::sea_query::OnConflict;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityTrait, Set, Statement};
use serde_json::json;
use std::collections::HashMap;

use super::SignalsRow;
use super::alert::{AlertDecision, decide_transition, push_slack};
use super::evaluator::{
    HealthStatus, HealthThresholds, WorkspaceHealth, WorkspaceSignals, evaluate,
};
use super::queries::{WorkspaceLabel, gather_signals, gather_workspace_labels};
use super::reconcile::{DriftVerdict, LiveReconcileRunner, ReconcileRunner};
use crate::integrations::slack::client::SlackClient;

/// Fold reconciliation verdicts into the signals the evaluator consumes.
fn apply_reconciliation(signals: &mut WorkspaceSignals, verdicts: Vec<DriftVerdict>) {
    signals.reconciliation = verdicts;
}

/// Evaluate one workspace's signals, push Slack on a status transition, and
/// upsert state. Returns `true` if an alert/recovery was pushed. Shared by the
/// fleet sweep and the single-workspace path so both behave identically.
async fn eval_and_persist(
    db: &DatabaseConnection,
    runner: &LiveReconcileRunner,
    client: &SlackClient,
    slack: &Option<(String, String)>,
    labels: &HashMap<uuid::Uuid, WorkspaceLabel>,
    thresholds: &HealthThresholds,
    signals: &mut WorkspaceSignals,
) -> bool {
    let verdicts = runner.run_checks(signals.workspace_id).await;
    apply_reconciliation(signals, verdicts);
    let health = evaluate(signals, thresholds);
    let prev = load_prev_status(db, signals.workspace_id).await;
    let decision = decide_transition(prev, health.status);

    let mut alerted = false;
    if decision != AlertDecision::Silent
        && let Some((token, channel)) = slack
    {
        match push_slack(
            client,
            token,
            channel,
            signals.workspace_id,
            labels.get(&signals.workspace_id),
            health.status,
            &health.reasons,
            decision,
        )
        .await
        {
            Ok(()) => alerted = true,
            Err(e) => tracing::warn!(target: "health_eval", error = %e, "slack push failed"),
        }
    }
    upsert_state(db, &health, signals, prev).await;
    alerted
}

/// Single-workspace eval: gather this workspace's signals (synthesizing an empty
/// set when it has no activity), evaluate, push Slack on a transition, and upsert
/// `workspace_health_state`. Same logic as the fleet sweep, scoped to one id.
pub(crate) async fn run_eval_pass_single(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
) -> Result<String, String> {
    let thresholds = HealthThresholds::from_env();
    let mut signals = gather_signals(db, &thresholds, Some(workspace_id))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .unwrap_or_else(|| WorkspaceSignals::empty(workspace_id));

    let runner = LiveReconcileRunner::from_env(chrono::Utc::now()).with_db(db.clone());
    let slack = ops_slack_target();
    let labels = match gather_workspace_labels(db).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(target: "health_eval", error = %e, "workspace label fetch failed");
            Default::default()
        }
    };
    let client = SlackClient::new();

    let alerted = eval_and_persist(
        db,
        &runner,
        &client,
        &slack,
        &labels,
        &thresholds,
        &mut signals,
    )
    .await;
    Ok(format!("evaluated=1 alerted={}", alerted as usize))
}

/// Load the last-known status for `ws`, mapping the stored string back into the
/// enum. `None` when there's no prior row (first eval) or an unrecognized value.
async fn load_prev_status(db: &DatabaseConnection, ws: uuid::Uuid) -> Option<HealthStatus> {
    let row = entity::workspace_health_state::Entity::find_by_id(ws)
        .one(db)
        .await
        .ok()??;
    match row.status.as_str() {
        "unhealthy" => Some(HealthStatus::Unhealthy),
        "degraded" => Some(HealthStatus::Degraded),
        "healthy" => Some(HealthStatus::Healthy),
        _ => None,
    }
}

/// Upsert the state row. `status` / `reasons` / `updated_at` are always
/// refreshed; `changed_at` is only bumped to now() when the status actually
/// changed vs the prior value (so it records "since when" the workspace has
/// held this status, not when the row was last touched).
async fn upsert_state(
    db: &DatabaseConnection,
    health: &WorkspaceHealth,
    signals: &WorkspaceSignals,
    prev: Option<HealthStatus>,
) {
    let ws = health.workspace_id;
    let status = health.status;
    let now = chrono::Utc::now().fixed_offset();
    // Full rollup the read endpoint returns verbatim (labels + timestamps are
    // joined/read separately, so they are intentionally not in the payload).
    let payload = json!({
        "workspace_id": ws,
        "status": status.as_str(),
        "reasons": health.reasons,
        "dimensions": health.dimensions,
        "signals": SignalsRow::from(signals),
        "reconciliation": signals.reconciliation,
    });
    let model = entity::workspace_health_state::ActiveModel {
        workspace_id: Set(ws),
        status: Set(status.as_str().to_string()),
        reasons: Set(json!(health.reasons)),
        changed_at: Set(now),
        updated_at: Set(now),
        payload: Set(Some(payload)),
    };
    // On conflict, refresh status/reasons/payload/updated_at but NOT changed_at
    // — a re-eval with the same status must preserve the original transition time.
    let res = entity::workspace_health_state::Entity::insert(model)
        .on_conflict(
            OnConflict::column(entity::workspace_health_state::Column::WorkspaceId)
                .update_columns([
                    entity::workspace_health_state::Column::Status,
                    entity::workspace_health_state::Column::Reasons,
                    entity::workspace_health_state::Column::Payload,
                    entity::workspace_health_state::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(db)
        .await;
    if let Err(e) = res {
        tracing::warn!(target: "health_eval", error = %e, "state upsert failed");
        return;
    }
    // Only when the status transitioned: stamp changed_at = now(). On a brand
    // new row the INSERT above already set it correctly, so this is a no-op
    // there (prev == None and a fresh status is still a change → harmless reset
    // to the same now()). The targeted update keeps the steady-state path from
    // clobbering the transition time.
    if prev != Some(status)
        && let Err(e) = db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "UPDATE workspace_health_state SET changed_at = $1 WHERE workspace_id = $2",
                [now.into(), ws.into()],
            ))
            .await
    {
        tracing::warn!(target: "health_eval", error = %e, "changed_at update failed");
    }
}

/// Ops Slack bot token + channel from env. `None` disables Slack (the dashboard
/// read endpoint still works). No established ops-alert Slack mechanism exists
/// in the codebase today — the per-org `slack_installations` tokens are
/// customer-scoped, so this internal alert path uses dedicated ops env vars.
fn ops_slack_target() -> Option<(String, String)> {
    let token = std::env::var("OXY_OPS_SLACK_BOT_TOKEN").ok()?;
    let channel = std::env::var("OXY_OPS_SLACK_CHANNEL").ok()?;
    Some((token, channel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::admin::workspace_health::reconcile::unreachable_verdict;
    use migration::MigratorTrait;
    use sea_orm::{Database, EntityTrait};

    async fn test_db() -> Option<DatabaseConnection> {
        let url = std::env::var("OXY_TEST_DATABASE_URL").ok()?;
        let db = Database::connect(&url).await.ok()?;
        migration::Migrator::up(&db, None).await.ok()?;
        agentic_runtime::migration::RuntimeMigrator::up(&db, None)
            .await
            .ok()?;
        Some(db)
    }

    #[tokio::test]
    async fn single_eval_persists_healthy_row_for_idle_workspace() {
        let Some(db) = test_db().await else {
            eprintln!("skipping: OXY_TEST_DATABASE_URL not set");
            return;
        };
        let ws = uuid::Uuid::new_v4();
        let summary = run_eval_pass_single(&db, ws).await.unwrap();
        assert_eq!(summary, "evaluated=1 alerted=0");
        let row = entity::workspace_health_state::Entity::find_by_id(ws)
            .one(&db)
            .await
            .unwrap()
            .expect("a state row should be persisted for the idle workspace");
        assert_eq!(row.status, "healthy");
    }

    fn empty_signals() -> WorkspaceSignals {
        WorkspaceSignals {
            workspace_id: uuid::Uuid::nil(),
            failed_runs: 0,
            timed_out_runs: 0,
            total_runs: 0,
            airway_last_run_failed: false,
            airway_completed_with_errors: false,
            open_high_anomalies: 0,
            open_medium_anomalies: 0,
            dead_letter_count: 0,
            reconciliation: Vec::new(),
        }
    }

    #[test]
    fn apply_reconciliation_sets_verdicts() {
        let mut s = empty_signals();
        apply_reconciliation(&mut s, vec![unreachable_verdict("c", "toast")]);
        assert_eq!(s.reconciliation.len(), 1);
        assert_eq!(s.reconciliation[0].status, HealthStatus::Degraded);
    }
}
