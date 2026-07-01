//! `ReconcileRunner`: runs every check in a workspace's `reconcile.yml` and
//! returns drift verdicts. The live impl batches the external fetch per source
//! (one Toast report per window, shared across checks) then pairs each with its
//! Oxy measure; the sweep depends only on the trait, so it is testable with a
//! fake.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use agentic_analytics::MetricTreeRunner;
use agentic_pipeline::platform::ProjectContext;
use airlayer::engine::query::{FilterOperator, QueryFilter};
use async_trait::async_trait;
use sea_orm::DatabaseConnection;

use super::config::{FilterOp, MeasureFilterSpec, ReconcileCheck, parse_reconcile_config};
use super::source::{ReconcileError, SourceCtx, source_for};
use super::window::resolve_window;
use super::{DriftVerdict, compare, error_verdict, unreachable_verdict};
use crate::server::api::compiled_reader::resolve_reconcile_config;
use crate::server::router::recovery::build_workspace_ctx;
use oxy::config::model::ToastAnalyticsIntegration;
use oxy::service::secret_manager::SecretManagerService;

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
    /// context that runs the Oxy-side measure. `None` (e.g. in unit tests)
    /// leaves the measure unwired — checks surface as "measure failed".
    db: Option<DatabaseConnection>,
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

    /// Attach the DB handle so the Oxy-side measure can execute (the sweep
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
    /// workspace context can't be built (measures then surface as "measure
    /// failed", the safe default).
    async fn build_measure_runner(
        &self,
        workspace_id: uuid::Uuid,
    ) -> Option<Arc<dyn MetricTreeRunner>> {
        let db = self.db.as_ref()?;
        let ctx = build_workspace_ctx(workspace_id, db).await?;
        ctx.metric_tree_runner_system()
    }

    /// Resolve every `toast_analytics` integration in `config.yml` (paired with
    /// its name), so a check can bind to a specific account by name. Empty when
    /// there's no DB handle, no workspace context, or none declared.
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

    /// Batch the external fetch for one source's checks, then pair each with its
    /// Oxy measure into a verdict. Verdicts are returned in `checks` order.
    async fn run_source_group(
        &self,
        workspace_id: uuid::Uuid,
        source_id: &str,
        checks: &[&ReconcileCheck],
        measure_runner: &Option<Arc<dyn MetricTreeRunner>>,
        toast: Option<&ToastAnalyticsIntegration>,
    ) -> Vec<DriftVerdict> {
        let externals = self
            .fetch_externals(workspace_id, source_id, checks, toast)
            .await;
        let mut out = Vec::with_capacity(checks.len());
        for (check, ext) in checks.iter().zip(externals) {
            out.push(self.verdict_for(check, ext, measure_runner).await);
        }
        out
    }

    /// Resolve the source's secrets and delegate to its batched fetch. Unknown
    /// source → an `Unknown` error per check. `toast` carries the workspace's
    /// resolved `toast` integration (secret var-names + base URL); `None` lets
    /// the source fall back to its built-in defaults.
    async fn fetch_externals(
        &self,
        workspace_id: uuid::Uuid,
        source_id: &str,
        checks: &[&ReconcileCheck],
        toast: Option<&ToastAnalyticsIntegration>,
    ) -> Vec<Result<f64, ReconcileError>> {
        let Some(source) = source_for(source_id, toast) else {
            return checks
                .iter()
                .map(|_| Err(ReconcileError::Unknown(source_id.to_string())))
                .collect();
        };
        // `SecretManagerService`'s `project_id` parameter IS the workspace id in
        // Oxy — there is no separate projects table, and every secret read/write
        // site (the secrets API, request middleware, recovery) keys the manager
        // by `workspace_id`. Passing it here matches how the secrets were stored.
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
        source.fetch_externals(&ctx, checks).await
    }

    /// Pair one check's external value with its Oxy measure. Every failure maps
    /// to a Degraded verdict (never a panic, never aborts the sweep).
    async fn verdict_for(
        &self,
        check: &ReconcileCheck,
        external: Result<f64, ReconcileError>,
        measure_runner: &Option<Arc<dyn MetricTreeRunner>>,
    ) -> DriftVerdict {
        // Only day-grained windows are calendar-correct. Week/Month resolve to
        // approximate 7-/30-day spans that don't line up with a calendar period,
        // so Oxy and the external source would compare different ranges and read
        // spurious drift. Degrade the check with a clear reason rather than emit a
        // silently-wrong comparison, until calendar-aware boundaries land.
        if !matches!(check.window.grain, super::Grain::Day) {
            return error_verdict(
                &check.name,
                format!(
                    "unsupported reconcile window grain {:?}: only `day` is supported \
                     (week/month would compare non-calendar spans)",
                    check.window.grain
                ),
            );
        }
        let ext = match external {
            Ok(v) => v,
            Err(ReconcileError::Unreachable(_)) => {
                return unreachable_verdict(&check.name, &check.source);
            }
            Err(ReconcileError::Unknown(_)) => {
                return error_verdict(
                    &check.name,
                    format!("unknown reconcile source: {}", check.source),
                );
            }
            Err(e) => return error_verdict(&check.name, e.to_string()),
        };

        let oxy = match self.run_measure(check, measure_runner).await {
            Ok(v) => v,
            Err(msg) => return error_verdict(&check.name, format!("measure failed: {msg}")),
        };
        compare(&check.name, oxy, ext, &check.tolerance, self.pct_unhealthy)
    }

    /// Execute the Oxy-side semantic measure for the check's window and filters,
    /// returning a single aggregated scalar via the SYSTEM-mode metric-tree
    /// runner (compiles the measure through airlayer and runs it on the
    /// workspace connector). `measure_runner` is `None` when no workspace
    /// context could be built (no DB handle, local sentinel, missing path) —
    /// surfaced as an error so the check degrades safely.
    async fn run_measure(
        &self,
        check: &ReconcileCheck,
        measure_runner: &Option<Arc<dyn MetricTreeRunner>>,
    ) -> Result<f64, String> {
        let runner = measure_runner
            .as_ref()
            .ok_or_else(|| "no workspace semantic context".to_string())?;
        let date_range = resolve_window(&check.window, self.now);
        let filters = check.filters.iter().map(to_query_filter).collect();
        runner
            .run_scalar(
                check.measure.clone(),
                check.time_dimension.clone(),
                (date_range[0].clone(), date_range[1].clone()),
                filters,
            )
            .await
            .map_err(|e| e.to_string())
    }
}

/// Pick the toast integration a check group binds to: by `name` when the check
/// named one, else the first declared (back-compat for single-account
/// workspaces). `None` when the named integration is absent or none exist — the
/// source then resolves no secrets and the check degrades to NotConfigured.
fn pick_toast<'a>(
    integrations: &'a [(String, ToastAnalyticsIntegration)],
    name: Option<&str>,
) -> Option<&'a ToastAnalyticsIntegration> {
    match name {
        Some(n) => integrations.iter().find(|(nm, _)| nm == n).map(|(_, t)| t),
        None => integrations.first().map(|(_, t)| t),
    }
}

/// Map a reconcile `MeasureFilterSpec` to an airlayer `QueryFilter`. airlayer
/// has no `in`/`not_in` operator — `equals`/`notEquals` with multiple `values`
/// carries IN/NOT-IN semantics — so the list ops fold onto those.
fn to_query_filter(f: &MeasureFilterSpec) -> QueryFilter {
    let operator = match f.op {
        FilterOp::Eq | FilterOp::In => FilterOperator::Equals,
        FilterOp::Neq | FilterOp::NotIn => FilterOperator::NotEquals,
        FilterOp::Gt => FilterOperator::Gt,
        FilterOp::Gte => FilterOperator::Gte,
        FilterOp::Lt => FilterOperator::Lt,
        FilterOp::Lte => FilterOperator::Lte,
    };
    QueryFilter {
        member: Some(f.field.clone()),
        operator: Some(operator),
        values: json_to_values(&f.value),
        and: None,
        or: None,
    }
}

/// A scalar value → one string; an array → one string per element (for IN).
/// Strings drop their JSON quotes; other scalars stringify as-is.
fn json_to_values(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items.iter().map(scalar_to_string).collect(),
        other => vec![scalar_to_string(other)],
    }
}

fn scalar_to_string(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

#[async_trait]
impl ReconcileRunner for LiveReconcileRunner {
    async fn run_checks(&self, workspace_id: uuid::Uuid) -> Vec<DriftVerdict> {
        // The compiled reader returns `None` for local mode and for a
        // draft/non-default branch on a working-copy node (see
        // `open_compiled_revision`). Per the compile-boundary contract, readers
        // fall through to the filesystem on a miss — so read the workspace's
        // on-disk `reconcile.yml` rather than silently skipping reconciliation
        // (the bug that made health checks complete instantly with no checks on
        // any branch other than the default).
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
        // group below binds to the one its checks name.
        let toast_integrations = self.resolve_toast_integrations(workspace_id).await;

        // Group check indices by (source, integration) so each distinct external
        // account fetches its report once — two checks naming different Toast
        // accounts must NOT share one batched report — then stitch verdicts back
        // into the original order.
        let mut by_group: BTreeMap<(&str, Option<&str>), Vec<usize>> = BTreeMap::new();
        for (i, c) in cfg.checks.iter().enumerate() {
            by_group
                .entry((c.source.as_str(), c.integration.as_deref()))
                .or_default()
                .push(i);
        }
        let mut slots: Vec<Option<DriftVerdict>> = (0..cfg.checks.len()).map(|_| None).collect();
        for ((source_id, integration_name), idxs) in by_group {
            let toast = pick_toast(&toast_integrations, integration_name);
            let group: Vec<&ReconcileCheck> = idxs.iter().map(|&i| &cfg.checks[i]).collect();
            let verdicts = self
                .run_source_group(workspace_id, source_id, &group, &measure_runner, toast)
                .await;
            for (&i, v) in idxs.iter().zip(verdicts) {
                slots[i] = Some(v);
            }
        }
        slots
            .into_iter()
            .map(|v| v.expect("every slot filled"))
            .collect()
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

    #[tokio::test]
    async fn fake_runner_returns_canned_verdicts() {
        let r = FakeReconcileRunner {
            verdicts: vec![unreachable_verdict("c", "toast")],
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

    use super::super::Tolerance;
    use super::super::compare::Combinator;
    use super::super::config::{ExternalSpec, Grain, Window};

    fn check_with_grain(grain: Grain) -> ReconcileCheck {
        ReconcileCheck {
            name: "c".to_string(),
            source: "toast".to_string(),
            integration: None,
            measure: "m.x".to_string(),
            time_dimension: "m.d".to_string(),
            filters: vec![],
            window: Window {
                last: 1,
                grain,
                offset: 1,
            },
            external: ExternalSpec {
                metric: "netSalesAmount".to_string(),
                restaurants: vec![],
            },
            tolerance: Tolerance {
                abs: 1.0,
                pct: 0.5,
                combinator: Combinator::And,
            },
        }
    }

    #[tokio::test]
    async fn non_day_grain_degrades_with_reason() {
        let r = LiveReconcileRunner::from_env(chrono::Utc::now());
        // Even with a valid external value, a week/month grain must degrade
        // (never compare) so a non-calendar span can't read as spurious drift.
        let v = r
            .verdict_for(&check_with_grain(Grain::Month), Ok(100.0), &None)
            .await;
        assert_eq!(v.status, HealthStatus::Degraded);
        assert!(v.reason.unwrap().contains("grain"));
    }
}
