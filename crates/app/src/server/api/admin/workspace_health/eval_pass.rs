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
use super::smoke::config::smoke_due;
use super::smoke::runner::resolve_smoke_settings;
use super::smoke::{LiveSmokeRunner, SmokeProbeStatus, SmokeRunner, SmokeVerdict, probe_statuses};
use crate::integrations::slack::client::SlackClient;
use oxy::config::health_check::SmokeTestConfig;

/// Everything an eval needs that doesn't vary per workspace. Grouped so the
/// per-workspace entry point stays a two-argument function as the pass grows.
struct EvalCtx<'a> {
    db: &'a DatabaseConnection,
    reconcile: &'a LiveReconcileRunner,
    smoke: &'a LiveSmokeRunner,
    slack_client: &'a SlackClient,
    slack: &'a Option<(String, String)>,
    labels: &'a HashMap<uuid::Uuid, WorkspaceLabel>,
    thresholds: &'a HealthThresholds,
    /// The pass's reference instant — also the `last_smoke_at` stamp when the
    /// smoke probes run, so the gate and the stamp can't disagree.
    now: chrono::DateTime<chrono::Utc>,
    /// Run the smoke probes on this pass regardless of the cadence — an operator
    /// pressing "Run smoke test". It overrides the *clock*, never the *config*:
    /// a workspace with `smoke_test: { enabled: false }` still runs nothing, so
    /// the button can't bill an opted-out workspace for warehouse queries and
    /// agent tokens.
    force_smoke: bool,
}

/// Fold reconciliation verdicts into the signals the evaluator consumes.
fn apply_reconciliation(signals: &mut WorkspaceSignals, verdicts: Vec<DriftVerdict>) {
    signals.reconciliation = verdicts;
}

/// The parts of the previous state row this pass needs: the status to diff
/// against, and the smoke verdicts + stamp + config to decide whether the probes
/// are due.
#[derive(Default)]
struct PrevState {
    status: Option<HealthStatus>,
    last_smoke_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    smoke: Vec<SmokeVerdict>,
    /// The smoke config the stored verdicts were produced under. `None` for a row
    /// written before this was persisted, which reads as "we don't know what ran"
    /// and makes the probes due — the safe direction: one extra smoke run, versus
    /// serving verdicts that may not match the config the UI is showing.
    smoke_config: Option<SmokeTestConfig>,
}

/// Evaluate one workspace's signals, push Slack on a status transition, and
/// upsert state. Returns `true` if an alert/recovery was pushed. Shared by the
/// fleet sweep and the single-workspace path so both behave identically.
async fn eval_and_persist(ctx: &EvalCtx<'_>, signals: &mut WorkspaceSignals) -> bool {
    let workspace_id = signals.workspace_id;
    let verdicts = ctx.reconcile.run_checks(workspace_id).await;
    apply_reconciliation(signals, verdicts);

    let prev = load_prev_state(ctx.db, workspace_id).await;
    let smoke = resolve_smoke(ctx, workspace_id, &prev).await;
    signals.smoke = smoke.verdicts.clone();

    let health = evaluate(signals, ctx.thresholds);
    let decision = decide_transition(prev.status, health.status);

    let mut alerted = false;
    if decision != AlertDecision::Silent
        && let Some((token, channel)) = ctx.slack
    {
        match push_slack(
            ctx.slack_client,
            token,
            channel,
            workspace_id,
            ctx.labels.get(&workspace_id),
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
    upsert_state(ctx.db, &health, signals, prev.status, &smoke).await;
    alerted
}

/// The smoke half of an eval pass: the verdicts to roll up, when they were
/// produced, which probe kinds are enabled, and the config that produced them.
/// `probes` is persisted so the UI can distinguish a disabled probe from an
/// enabled one with no results; `config` so the next pass can tell whether the
/// stored verdicts still describe the config the UI is showing.
struct SmokeOutcome {
    verdicts: Vec<SmokeVerdict>,
    last_smoke_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    probes: Vec<SmokeProbeStatus>,
    /// `None` only when smoke is switched off entirely — which also clears the
    /// stamp, so re-enabling probes immediately.
    config: Option<SmokeTestConfig>,
}

/// Decide whether this pass runs the smoke probes, and run them if so.
///
/// `probes` reflects the *current* config every pass, even the ones that don't
/// re-run the probes, so toggling a probe kind shows up on the next pass rather
/// than the next smoke run. That is also why the carry-forward arm is gated on
/// the config and not just the clock: the enabled flags and the verdicts are
/// rendered side by side, so verdicts from a run under a *different* config make
/// the tab contradict itself (a newly-enabled `agent` probe, which always
/// produces a verdict, would show none — "No targets found"). `smoke_due` treats
/// a changed config as due now, so the two can never drift apart by more than
/// one eval pass.
///
/// Most passes still fall through the carry-forward arm: the smoke cadence
/// (default 6h) is far slower than the eval cadence (default 10m), so the
/// previous run's verdicts are reused verbatim and the dimension holds its value
/// instead of flapping to Healthy between smoke runs.
async fn resolve_smoke(
    ctx: &EvalCtx<'_>,
    workspace_id: uuid::Uuid,
    prev: &PrevState,
) -> SmokeOutcome {
    // An absent `smoke_test:` block resolves to the cheap default (connections
    // only), so most workspaces land here with `explicit: false` and still get
    // probed. Only an explicit `enabled: false` switches it off — and that clears
    // the stamp and the stored config, so re-enabling probes immediately rather
    // than waiting out an interval measured from a run under a different config.
    let settings = resolve_smoke_settings(ctx.db, workspace_id).await;
    if !settings.config.enabled {
        return SmokeOutcome {
            verdicts: Vec::new(),
            last_smoke_at: None,
            probes: Vec::new(),
            config: None,
        };
    }
    let probes = probe_statuses(&settings.config);

    if !ctx.force_smoke
        && !smoke_due(
            prev.smoke_config.as_ref(),
            &settings.config,
            prev.last_smoke_at,
            ctx.now,
        )
    {
        return SmokeOutcome {
            verdicts: prev.smoke.clone(),
            last_smoke_at: prev.last_smoke_at,
            probes,
            config: prev.smoke_config.clone(),
        };
    }

    tracing::info!(target: "health_eval", %workspace_id, "running workspace smoke test");
    let verdicts = ctx.smoke.run_smoke(workspace_id, &settings).await;
    SmokeOutcome {
        verdicts,
        last_smoke_at: Some(ctx.now.fixed_offset()),
        probes,
        config: Some(settings.config),
    }
}

/// Single-workspace eval: gather this workspace's signals (synthesizing an empty
/// set when it has no activity), evaluate, push Slack on a transition, and upsert
/// `workspace_health_state`. Same logic as the fleet sweep, scoped to one id.
///
/// `force_smoke` runs the smoke probes on this pass even if their cadence has not
/// elapsed — the "Run smoke test" button. Scheduled fires pass `false` and keep
/// the 6h cadence, so the button is the only thing that can make the probes cost
/// more than the config asked for.
pub(crate) async fn run_eval_pass_single(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    force_smoke: bool,
) -> Result<String, String> {
    let thresholds = HealthThresholds::from_env();
    let mut signals = gather_signals(db, &thresholds, Some(workspace_id))
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .unwrap_or_else(|| WorkspaceSignals::empty(workspace_id));

    let now = chrono::Utc::now();
    let reconcile = LiveReconcileRunner::from_env(now).with_db(db.clone());
    let smoke = LiveSmokeRunner::from_env().with_db(db.clone());
    let slack = ops_slack_target();
    let labels = match gather_workspace_labels(db).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(target: "health_eval", error = %e, "workspace label fetch failed");
            Default::default()
        }
    };
    let client = SlackClient::new();

    let ctx = EvalCtx {
        db,
        reconcile: &reconcile,
        smoke: &smoke,
        slack_client: &client,
        slack: &slack,
        labels: &labels,
        thresholds: &thresholds,
        now,
        force_smoke,
    };
    let alerted = eval_and_persist(&ctx, &mut signals).await;
    Ok(format!("evaluated=1 alerted={}", alerted as usize))
}

/// Load the previous state row: the last-known status (mapped back into the
/// enum), when the smoke probes last ran, and the verdicts they produced. All
/// fields default when there's no prior row (first eval), the read fails, or the
/// stored value is unrecognized — a missing prior state is not an error.
async fn load_prev_state(db: &DatabaseConnection, ws: uuid::Uuid) -> PrevState {
    let Ok(Some(row)) = entity::workspace_health_state::Entity::find_by_id(ws)
        .one(db)
        .await
    else {
        return PrevState::default();
    };
    let status = match row.status.as_str() {
        "unhealthy" => Some(HealthStatus::Unhealthy),
        "degraded" => Some(HealthStatus::Degraded),
        "healthy" => Some(HealthStatus::Healthy),
        _ => None,
    };
    PrevState {
        status,
        last_smoke_at: row.last_smoke_at,
        smoke: cached_smoke_verdicts(row.payload.as_ref()),
        smoke_config: cached_smoke_config(row.payload.as_ref()),
    }
}

/// The smoke config the stored verdicts were produced under. Absent (a row from
/// before this key existed) or malformed both yield `None`, which reads as "we
/// can't vouch for what produced these verdicts" and re-runs the probes — one
/// unnecessary smoke run, once, rather than an indefinitely stale tab.
fn cached_smoke_config(payload: Option<&serde_json::Value>) -> Option<SmokeTestConfig> {
    payload
        .and_then(|p| p.get("smoke_config"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Read the previous run's smoke verdicts back out of the stored payload. A
/// payload written before this dimension existed has no `smoke` key, and a
/// malformed one must not wedge the pass — both yield no verdicts, which reads
/// as "smoke test hasn't run yet" and makes the next pass run it.
fn cached_smoke_verdicts(payload: Option<&serde_json::Value>) -> Vec<SmokeVerdict> {
    payload
        .and_then(|p| p.get("smoke"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
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
    smoke: &SmokeOutcome,
) {
    let ws = health.workspace_id;
    let status = health.status;
    let now = chrono::Utc::now().fixed_offset();
    // Full rollup the read endpoint returns verbatim (labels + timestamps are
    // joined/read separately, so they are intentionally not in the payload).
    // `smoke` is carried forward from the previous run on passes where the smoke
    // cadence hasn't elapsed, so it is always current with `last_smoke_at`;
    // `smoke_probes` reflects the current config so the UI can name a disabled
    // probe even between runs. `smoke_config` records what the verdicts were
    // produced under — the next pass compares it against the live config and
    // re-probes on a change, so the two never contradict each other for longer
    // than one eval pass. It is internal bookkeeping; the UI ignores it.
    let payload = json!({
        "workspace_id": ws,
        "status": status.as_str(),
        "reasons": health.reasons,
        "dimensions": health.dimensions,
        "signals": SignalsRow::from(signals),
        "reconciliation": signals.reconciliation,
        "smoke": signals.smoke,
        "smoke_probes": smoke.probes,
        "smoke_config": smoke.config,
    });
    let model = entity::workspace_health_state::ActiveModel {
        workspace_id: Set(ws),
        status: Set(status.as_str().to_string()),
        reasons: Set(json!(health.reasons)),
        changed_at: Set(now),
        updated_at: Set(now),
        payload: Set(Some(payload)),
        last_smoke_at: Set(smoke.last_smoke_at),
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
                    entity::workspace_health_state::Column::LastSmokeAt,
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
    use crate::server::api::admin::workspace_health::reconcile::{
        VerdictMeta, unreachable_verdict,
    };
    use crate::server::test_support::{SKIP_MSG, test_db};
    use sea_orm::EntityTrait;

    #[tokio::test]
    async fn single_eval_persists_healthy_row_for_idle_workspace() {
        let Some(db) = test_db().await else {
            eprintln!("{SKIP_MSG}");
            return;
        };
        let ws = uuid::Uuid::new_v4();
        let summary = run_eval_pass_single(&db, ws, false).await.unwrap();
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
            smoke: Vec::new(),
        }
    }

    #[test]
    fn apply_reconciliation_sets_verdicts() {
        let mut s = empty_signals();
        let meta = VerdictMeta {
            check: "c".to_string(),
            description: None,
            actual_label: "Actual".to_string(),
            expected_label: "Expected".to_string(),
        };
        apply_reconciliation(&mut s, vec![unreachable_verdict(&meta, "toast")]);
        assert_eq!(s.reconciliation.len(), 1);
        assert_eq!(s.reconciliation[0].status, HealthStatus::Degraded);
    }

    #[test]
    fn cached_smoke_verdicts_survive_a_payload_round_trip() {
        // This is the carry-forward path: on the ~35 eval passes between two 6h
        // smoke runs, the dimension's value comes from here.
        use crate::server::api::admin::workspace_health::smoke::{SmokeProbeKind, failed};
        let stored = json!({
            "smoke": [ failed(SmokeProbeKind::Semantic, "orders", "boom".into(), 7) ]
        });
        let back = cached_smoke_verdicts(Some(&stored));
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].check, "semantic:orders");
        assert_eq!(back[0].status, HealthStatus::Unhealthy);
    }

    #[test]
    fn missing_or_malformed_smoke_payload_yields_no_verdicts() {
        // A row written before the smoke dimension existed, and a corrupted one:
        // both must read as "hasn't run yet" rather than wedging the pass.
        assert!(cached_smoke_verdicts(None).is_empty());
        assert!(cached_smoke_verdicts(Some(&json!({ "reconciliation": [] }))).is_empty());
        assert!(cached_smoke_verdicts(Some(&json!({ "smoke": "not-an-array" }))).is_empty());
    }

    #[test]
    fn cached_smoke_config_round_trips_and_fails_safe() {
        let cfg = SmokeTestConfig {
            semantic: oxy::config::health_check::SemanticProbeConfig::Sweep(true),
            ..SmokeTestConfig::default()
        };
        let back = cached_smoke_config(Some(&json!({ "smoke_config": cfg })));
        assert_eq!(back.as_ref().map(|c| c.semantic.enabled()), Some(true));

        // A row from before this key existed, a null (smoke switched off), and a
        // corrupted value all read as "unknown" → the probes re-run. Never as an
        // empty config, which would silently match nothing and freeze the tab.
        assert!(cached_smoke_config(None).is_none());
        assert!(cached_smoke_config(Some(&json!({ "smoke": [] }))).is_none());
        assert!(cached_smoke_config(Some(&json!({ "smoke_config": null }))).is_none());
        assert!(cached_smoke_config(Some(&json!({ "smoke_config": "nonsense" }))).is_none());
    }
}
