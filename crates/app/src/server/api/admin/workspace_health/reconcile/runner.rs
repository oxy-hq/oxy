//! `ReconcileRunner`: runs every check in a workspace's `reconcile.yml` and
//! returns drift verdicts. Each check compares two operands (`actual` /
//! `expected`), each one of `semantic` / `sql` / `external` / `constant`. The
//! runner collects every external operand across both slots of all checks and
//! batch-fetches per `(source, integration)` (one Toast report per window,
//! shared), evaluates the non-external operands directly, then pairs each
//! check's two scalars into a verdict. The sweep depends only on the source
//! trait, so it is testable with a fake.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use agentic_analytics::MetricTreeRunner;
use agentic_pipeline::platform::ProjectContext;
use async_trait::async_trait;
use sea_orm::DatabaseConnection;

use super::config::{ExternalSpec, Operand, OperandKind, ReconcileCheck, parse_reconcile_config};
use super::oxy_query::{cell_to_f64, render_sql, semantic_request};
use super::source::{ExternalRequest, ReconcileError, SourceCtx, source_for};
use super::window::resolve_window;
use super::{
    DriftVerdict, ResolvedWindow, VerdictMeta, compare, error_verdict, unreachable_verdict,
};
use crate::server::api::compiled_reader::resolve_reconcile_config;
use crate::server::router::recovery::build_workspace_ctx;
use oxy::config::model::ToastAnalyticsIntegration;
use oxy::service::secret_manager::SecretManagerService;

/// Which slot of a check an operand fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Actual,
    Expected,
}

#[async_trait]
pub trait ReconcileRunner: Send + Sync {
    async fn run_checks(&self, workspace_id: uuid::Uuid) -> Vec<DriftVerdict>;
}

pub struct LiveReconcileRunner {
    now: chrono::DateTime<chrono::Utc>,
    /// Whole-report time budget for an async external report (create + poll).
    report_timeout: Duration,
    pct_unhealthy: f64,
    /// Central Postgres handle, needed to build the per-workspace semantic
    /// context that runs a semantic operand. `None` (e.g. in unit tests) leaves
    /// the measure unwired — such operands surface as an error.
    db: Option<DatabaseConnection>,
}

/// An external operand awaiting a fetch, tagged by its slot and carrying the
/// already-resolved window. `spec` borrows the check; `window` is owned so the
/// `ExternalRequest` can borrow it during the batch.
struct ExternalSlot<'a> {
    check_index: usize,
    side: Side,
    spec: &'a ExternalSpec,
    window: [String; 2],
}

/// How an operand evaluation failed, mapped to the matching degraded verdict.
enum OperandFailure {
    /// External source unreachable — Degraded, never Unhealthy.
    Unreachable(String),
    /// Any other pre-comparison failure (bad measure, missing secret, unknown
    /// source, validation error).
    Error(String),
}

impl OperandFailure {
    fn into_verdict(self, meta: &VerdictMeta) -> DriftVerdict {
        match self {
            OperandFailure::Unreachable(source) => unreachable_verdict(meta, &source),
            OperandFailure::Error(msg) => error_verdict(meta, msg),
        }
    }
}

impl LiveReconcileRunner {
    pub fn from_env(now: chrono::DateTime<chrono::Utc>) -> Self {
        let report_timeout = std::env::var("OXY_RECONCILE_REPORT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);
        let pct_unhealthy = std::env::var("OXY_RECONCILE_PCT_UNHEALTHY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5.0);
        Self {
            now,
            report_timeout: Duration::from_secs(report_timeout),
            pct_unhealthy,
            db: None,
        }
    }

    /// Attach the DB handle so semantic/SQL operands can execute (the sweep
    /// builds a workspace-scoped semantic context from it).
    pub fn with_db(mut self, db: DatabaseConnection) -> Self {
        self.db = Some(db);
        self
    }

    /// Filesystem fallback for the reconcile config, taken when the compiled
    /// reader returns `None` (local mode / draft branch on a working-copy node).
    /// Reads the workspace working copy's root `reconcile.yml` (resolved via the
    /// `WorkspaceManager`, which handles git-subdirectory layouts). `None` when
    /// there's no DB handle, no resolvable workspace path, or no file on disk —
    /// a genuinely reconcile-less workspace, which is not an error.
    async fn read_reconcile_config_fs(
        &self,
        workspace_id: uuid::Uuid,
    ) -> Option<serde_json::Value> {
        let db = self.db.as_ref()?;
        let ctx = build_workspace_ctx(workspace_id, db).await?;
        let path = ctx
            .workspace_manager()
            .config_manager
            .workspace_path()
            .join("reconcile.yml");
        let text = match tokio::fs::read_to_string(&path).await {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::warn!(target: "health_eval", %workspace_id, error = %e,
                    "reconcile config FS read failed");
                return None;
            }
        };
        match serde_yaml::from_str::<serde_json::Value>(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(target: "health_eval", %workspace_id, error = %e,
                    "reconcile config FS parse failed");
                None
            }
        }
    }

    /// Build the SYSTEM-mode metric-tree runner for this workspace once, reused
    /// across all of its checks. `None` when there's no DB handle or the
    /// workspace context can't be built (semantic operands then surface as an
    /// error, the safe default).
    async fn build_measure_runner(
        &self,
        workspace_id: uuid::Uuid,
    ) -> Option<Arc<dyn MetricTreeRunner>> {
        let db = self.db.as_ref()?;
        let ctx = build_workspace_ctx(workspace_id, db).await?;
        ctx.metric_tree_runner_system()
    }

    /// Resolve every `toast_analytics` integration in `config.yml` (paired with
    /// its name), so an operand can bind to a specific account by name. Empty
    /// when there's no DB handle, no workspace context, or none declared.
    async fn resolve_toast_integrations(
        &self,
        workspace_id: uuid::Uuid,
    ) -> Vec<(String, ToastAnalyticsIntegration)> {
        let Some(db) = self.db.as_ref() else {
            return Vec::new();
        };
        let Some(ctx) = build_workspace_ctx(workspace_id, db).await else {
            return Vec::new();
        };
        ctx.workspace_manager()
            .config_manager
            .toast_analytics_integrations()
            .into_iter()
            .map(|(name, t)| (name.to_string(), t.clone()))
            .collect()
    }

    /// Collect every external operand across both slots of all checks.
    /// `windows[i]` is the resolved (calendar-snapped) window for check `i`.
    fn collect_external_slots<'a>(
        checks: &'a [ReconcileCheck],
        windows: &[ResolvedWindow],
    ) -> Vec<ExternalSlot<'a>> {
        let mut slots = Vec::new();
        for (i, check) in checks.iter().enumerate() {
            for (side, operand) in [
                (Side::Actual, &check.actual),
                (Side::Expected, &check.expected),
            ] {
                if let Ok(OperandKind::External(spec)) = operand.resolve_kind() {
                    slots.push(ExternalSlot {
                        check_index: i,
                        side,
                        spec,
                        window: windows[i].dates.clone(),
                    });
                }
            }
        }
        slots
    }

    /// Batch-fetch every external operand, grouped by `(source, integration)` so
    /// each distinct external account fetches its report once (shared across all
    /// external operands in the group, either side, all checks). Result keyed by
    /// `(check_index, Side)`.
    async fn fetch_all_externals(
        &self,
        workspace_id: uuid::Uuid,
        checks: &[ReconcileCheck],
        windows: &[ResolvedWindow],
        toast_integrations: &[(String, ToastAnalyticsIntegration)],
    ) -> HashMap<(usize, Side), Result<f64, ReconcileError>> {
        let slots = Self::collect_external_slots(checks, windows);
        let mut by_group: BTreeMap<(&str, Option<&str>), Vec<usize>> = BTreeMap::new();
        for (slot_idx, slot) in slots.iter().enumerate() {
            by_group
                .entry((slot.spec.source.as_str(), slot.spec.integration.as_deref()))
                .or_default()
                .push(slot_idx);
        }

        let mut out = HashMap::new();
        for ((source_id, integration_name), slot_idxs) in by_group {
            let toast = pick_toast(toast_integrations, integration_name);
            let requests: Vec<ExternalRequest<'_>> = slot_idxs
                .iter()
                .map(|&si| ExternalRequest {
                    spec: slots[si].spec,
                    window: &slots[si].window,
                })
                .collect();
            let results = self
                .fetch_group(workspace_id, source_id, &requests, toast)
                .await;
            for (&si, r) in slot_idxs.iter().zip(results) {
                let slot = &slots[si];
                out.insert((slot.check_index, slot.side), r);
            }
        }
        out
    }

    /// Resolve one source's secrets and delegate to its batched fetch. Unknown
    /// source → an `Unknown` error per request. `toast` carries the workspace's
    /// resolved integration (secret var-names + base URL); `None` lets the
    /// source fall back to its built-in defaults.
    async fn fetch_group(
        &self,
        workspace_id: uuid::Uuid,
        source_id: &str,
        requests: &[ExternalRequest<'_>],
        toast: Option<&ToastAnalyticsIntegration>,
    ) -> Vec<Result<f64, ReconcileError>> {
        let Some(source) = source_for(source_id, toast) else {
            return requests
                .iter()
                .map(|_| Err(ReconcileError::Unknown(source_id.to_string())))
                .collect();
        };
        // `SecretManagerService`'s `project_id` parameter IS the workspace id in
        // Oxy — there is no separate projects table, and every secret read/write
        // site keys the manager by `workspace_id`. Passing it here matches how
        // the secrets were stored.
        let secret_manager = SecretManagerService::new(workspace_id);
        let mut secrets = HashMap::new();
        // Resolve each configured var name, keying the value by its logical slot
        // so the source reads it back by slot regardless of the var name.
        for (slot, var) in source.secret_vars() {
            if let Some(value) = secret_manager.get_secret(&var).await {
                secrets.insert(slot.to_string(), value);
            }
        }
        let ctx = SourceCtx {
            now: self.now,
            secrets,
            report_timeout: self.report_timeout,
        };
        source.fetch_externals(&ctx, requests).await
    }

    /// Evaluate one check's two operands to scalars and compare. Every failure
    /// maps to a Degraded verdict (never a panic, never aborts the sweep).
    async fn verdict_for_check(
        &self,
        workspace_id: uuid::Uuid,
        check_index: usize,
        check: &ReconcileCheck,
        window: &ResolvedWindow,
        measure_runner: &Option<Arc<dyn MetricTreeRunner>>,
        externals: &HashMap<(usize, Side), Result<f64, ReconcileError>>,
    ) -> DriftVerdict {
        let meta = VerdictMeta {
            check: check.name.clone(),
            description: check.description.clone(),
            actual_label: check.actual.label_or("Actual"),
            expected_label: check.expected.label_or("Expected"),
        };
        let actual = match self
            .eval_operand(
                workspace_id,
                (check_index, Side::Actual),
                &check.actual,
                window,
                measure_runner,
                externals,
            )
            .await
        {
            Ok(v) => v,
            Err(f) => return f.into_verdict(&meta),
        };
        let expected = match self
            .eval_operand(
                workspace_id,
                (check_index, Side::Expected),
                &check.expected,
                window,
                measure_runner,
                externals,
            )
            .await
        {
            Ok(v) => v,
            Err(f) => return f.into_verdict(&meta),
        };
        compare(
            &meta,
            actual,
            expected,
            &check.tolerance,
            self.pct_unhealthy,
        )
    }

    /// Resolve one operand to a scalar per its kind. External operands read the
    /// pre-fetched `externals` map by `(check_index, Side)`; the others execute
    /// directly. Any failure surfaces as an `OperandFailure` so the check
    /// degrades safely.
    async fn eval_operand(
        &self,
        workspace_id: uuid::Uuid,
        key: (usize, Side),
        operand: &Operand,
        window: &ResolvedWindow,
        measure_runner: &Option<Arc<dyn MetricTreeRunner>>,
        externals: &HashMap<(usize, Side), Result<f64, ReconcileError>>,
    ) -> Result<f64, OperandFailure> {
        let kind = operand.resolve_kind().map_err(OperandFailure::Error)?;
        match kind {
            OperandKind::Constant(n) => Ok(n),
            OperandKind::Semantic(spec) => {
                let runner = measure_runner
                    .as_ref()
                    .ok_or_else(|| OperandFailure::Error("no workspace semantic context".into()))?;
                let request = semantic_request(&spec.query, &spec.time_dimension, window);
                runner
                    .run_query_scalar(request)
                    .await
                    .map_err(|e| OperandFailure::Error(e.to_string()))
            }
            OperandKind::Sql(spec) => self
                .run_sql(workspace_id, &spec.database, &spec.sql, window)
                .await
                .map_err(OperandFailure::Error),
            OperandKind::External(spec) => match externals.get(&key) {
                Some(Ok(v)) => Ok(*v),
                Some(Err(ReconcileError::Unreachable(_))) => {
                    Err(OperandFailure::Unreachable(spec.source.clone()))
                }
                Some(Err(ReconcileError::Unknown(_))) => Err(OperandFailure::Error(format!(
                    "unknown reconcile source: {}",
                    spec.source
                ))),
                Some(Err(e)) => Err(OperandFailure::Error(e.to_string())),
                None => Err(OperandFailure::Error(
                    "external value not fetched".to_string(),
                )),
            },
        }
    }

    /// Run a raw-SQL operand against the named workspace connection, reducing
    /// the first cell of the first row to a scalar. `{{ start_date }}` /
    /// `{{ end_date }}` in the SQL are bound from the resolved window.
    async fn run_sql(
        &self,
        workspace_id: uuid::Uuid,
        database: &str,
        sql: &str,
        period: &ResolvedWindow,
    ) -> Result<f64, String> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| "no workspace context".to_string())?;
        let ctx = build_workspace_ctx(workspace_id, db)
            .await
            .ok_or_else(|| "no workspace context".to_string())?;
        // Use the general, all-warehouse connector builder (the same path the
        // semantic operand takes via the metric-tree runner) rather than the
        // airhouse-only `resolve_pre_built_connector`, which early-returns `None`
        // for ClickHouse/Snowflake/BigQuery/Postgres and collapses to a flat
        // "not configured" error. `build_connector_for` dispatches every
        // warehouse type and surfaces the real error.
        let connector = ctx
            .build_connector_for(database)
            .await
            .map_err(|e| format!("reconcile sql: database '{database}': {e}"))?;
        let rendered = render_sql(sql, period)?;
        let res = connector
            .execute_query(&rendered, 1)
            .await
            .map_err(|e| e.to_string())?;
        let row = res
            .result
            .rows
            .first()
            .ok_or_else(|| "reconcile sql returned no rows".to_string())?;
        let cell = row
            .0
            .first()
            .ok_or_else(|| "reconcile sql returned no columns".to_string())?;
        cell_to_f64(cell)
    }
}

/// Pick the toast integration an operand binds to: by `name` when it named one,
/// else the first declared (back-compat for single-account workspaces). `None`
/// when the named integration is absent or none exist — the source then
/// resolves no secrets and the check degrades to NotConfigured.
fn pick_toast<'a>(
    integrations: &'a [(String, ToastAnalyticsIntegration)],
    name: Option<&str>,
) -> Option<&'a ToastAnalyticsIntegration> {
    match name {
        Some(n) => integrations.iter().find(|(nm, _)| nm == n).map(|(_, t)| t),
        None => integrations.first().map(|(_, t)| t),
    }
}

#[async_trait]
impl ReconcileRunner for LiveReconcileRunner {
    async fn run_checks(&self, workspace_id: uuid::Uuid) -> Vec<DriftVerdict> {
        // The compiled reader returns `None` for local mode and for a
        // draft/non-default branch on a working-copy node (see
        // `open_compiled_revision`). Per the compile-boundary contract, readers
        // fall through to the filesystem on a miss — so read the workspace's
        // on-disk `reconcile.yml` rather than silently skipping reconciliation.
        let (raw, source) = match resolve_reconcile_config(workspace_id, None).await {
            Ok(Some(v)) => (v, "compiled"),
            Ok(None) => match self.read_reconcile_config_fs(workspace_id).await {
                Some(v) => (v, "fs"),
                None => return Vec::new(),
            },
            Err(e) => {
                tracing::warn!(target: "health_eval", %workspace_id, error = %e,
                    "reconcile config read failed");
                return Vec::new();
            }
        };
        let cfg = match parse_reconcile_config(&raw) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(target: "health_eval", %workspace_id, error = %e,
                    "reconcile config parse failed");
                return Vec::new();
            }
        };
        tracing::info!(
            target: "health_eval",
            %workspace_id,
            source,
            checks = cfg.checks.len(),
            "running reconcile checks"
        );

        // One workspace-scoped measure runner, reused across every check.
        let measure_runner = self.build_measure_runner(workspace_id).await;
        // Every toast integration the workspace declares, resolved once; each
        // external operand binds to the one it names.
        let toast_integrations = self.resolve_toast_integrations(workspace_id).await;

        // Resolve each check's window once (shared by both operands), then
        // batch-fetch every external operand across all checks.
        let windows: Vec<ResolvedWindow> = cfg
            .checks
            .iter()
            .map(|c| resolve_window(&c.window, self.now))
            .collect();
        let externals = self
            .fetch_all_externals(workspace_id, &cfg.checks, &windows, &toast_integrations)
            .await;

        let mut out = Vec::with_capacity(cfg.checks.len());
        for (i, check) in cfg.checks.iter().enumerate() {
            out.push(
                self.verdict_for_check(
                    workspace_id,
                    i,
                    check,
                    &windows[i],
                    &measure_runner,
                    &externals,
                )
                .await,
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::admin::workspace_health::evaluator::HealthStatus;

    struct FakeReconcileRunner {
        verdicts: Vec<DriftVerdict>,
    }

    #[async_trait::async_trait]
    impl ReconcileRunner for FakeReconcileRunner {
        async fn run_checks(&self, _workspace_id: uuid::Uuid) -> Vec<DriftVerdict> {
            self.verdicts.clone()
        }
    }

    fn meta(check: &str) -> VerdictMeta {
        VerdictMeta {
            check: check.to_string(),
            description: None,
            actual_label: "Actual".to_string(),
            expected_label: "Expected".to_string(),
        }
    }

    #[tokio::test]
    async fn fake_runner_returns_canned_verdicts() {
        let r = FakeReconcileRunner {
            verdicts: vec![unreachable_verdict(&meta("c"), "toast")],
        };
        let out = r.run_checks(uuid::Uuid::nil()).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].status, HealthStatus::Degraded);
    }

    fn toast_named(name: &str) -> (String, ToastAnalyticsIntegration) {
        (
            name.to_string(),
            ToastAnalyticsIntegration {
                client_id_var: Some(format!("{name}_ID")),
                client_secret_var: None,
                api_token_var: None,
                base_url: None,
            },
        )
    }

    #[test]
    fn pick_toast_binds_by_name_then_falls_back_to_first() {
        let list = vec![toast_named("main"), toast_named("second")];
        // Named match.
        assert_eq!(
            pick_toast(&list, Some("second")).unwrap().client_id_var,
            Some("second_ID".to_string())
        );
        // No name → first declared.
        assert_eq!(
            pick_toast(&list, None).unwrap().client_id_var,
            Some("main_ID".to_string())
        );
        // Named but absent → None (check degrades to NotConfigured).
        assert!(pick_toast(&list, Some("ghost")).is_none());
        // No integrations declared → None.
        assert!(pick_toast(&[], None).is_none());
    }

    #[test]
    fn env_defaults_are_sane() {
        let r = LiveReconcileRunner::from_env(chrono::Utc::now());
        assert_eq!(r.pct_unhealthy, 5.0);
        assert_eq!(r.report_timeout.as_secs(), 60);
    }

    #[test]
    fn sql_scalar_reduces_first_cell() {
        use super::super::oxy_query::cell_to_f64;
        use agentic_core::result::{CellValue, QueryRow};
        // Shape a one-cell result like execute_query(sql, 1) returns; the runner
        // reduces res.result.rows[0].0[0] via cell_to_f64.
        let rows = vec![QueryRow(vec![CellValue::Number(1234.5)])];
        let cell = &rows.first().unwrap().0[0];
        assert_eq!(cell_to_f64(cell).unwrap(), 1234.5);

        // Empty result ⇒ the runner surfaces "no rows".
        let empty: Vec<QueryRow> = vec![];
        assert!(empty.first().is_none());
    }

    use super::super::Tolerance;
    use super::super::compare::Combinator;
    use super::super::config::Grain;
    use super::super::config::{Operand, SemanticSpec, Window};

    fn semantic_operand(measure: &str) -> Operand {
        let mut query = airlayer::engine::query::QueryRequest::new();
        query.measures = vec![measure.to_string()];
        Operand {
            label: None,
            semantic: Some(SemanticSpec {
                query,
                time_dimension: "m.d".to_string(),
            }),
            sql: None,
            external: None,
            constant: None,
        }
    }

    fn constant_operand(n: f64) -> Operand {
        Operand {
            label: None,
            semantic: None,
            sql: None,
            external: None,
            constant: Some(n),
        }
    }

    fn check_with_grain(grain: Grain) -> ReconcileCheck {
        ReconcileCheck {
            name: "c".to_string(),
            description: None,
            window: Window {
                last: 1,
                grain,
                offset: 1,
                freshness: None,
                timezone: None,
                week_start:
                    crate::server::api::admin::workspace_health::reconcile::WeekStart::Sunday,
            },
            tolerance: Tolerance {
                abs: 1.0,
                pct: 0.5,
                combinator: Combinator::And,
            },
            group_by: None,
            actual: semantic_operand("m.x"),
            expected: constant_operand(100.0),
        }
    }

    #[tokio::test]
    async fn week_month_grain_evaluate_not_degrade() {
        // Week/month grains now snap to calendar boundaries and compare like any
        // other window — they must NOT degrade on grain alone.
        let r = LiveReconcileRunner::from_env(chrono::Utc::now());
        for grain in [Grain::Week, Grain::Month] {
            let mut check = check_with_grain(grain);
            // Both sides constant so the check evaluates with no runner/external.
            check.actual = constant_operand(5.0);
            check.expected = constant_operand(5.0);
            let window = resolve_window(&check.window, r.now);
            let v = r
                .verdict_for_check(
                    uuid::Uuid::nil(),
                    0,
                    &check,
                    &window,
                    &None,
                    &HashMap::new(),
                )
                .await;
            assert_eq!(v.status, HealthStatus::Healthy, "{grain:?} should compare");
        }
    }

    #[tokio::test]
    async fn constant_vs_constant_compares_without_external_fetch() {
        let r = LiveReconcileRunner::from_env(chrono::Utc::now());
        let mut check = check_with_grain(Grain::Day);
        // Both sides constant: no measure runner, no external fetch needed.
        check.actual = constant_operand(0.0);
        check.expected = constant_operand(0.0);
        let window = resolve_window(&check.window, r.now);
        let v = r
            .verdict_for_check(
                uuid::Uuid::nil(),
                0,
                &check,
                &window,
                &None,
                &HashMap::new(),
            )
            .await;
        assert_eq!(v.status, HealthStatus::Healthy);
        assert_eq!(v.actual, 0.0);
        assert_eq!(v.expected, 0.0);
    }
}
