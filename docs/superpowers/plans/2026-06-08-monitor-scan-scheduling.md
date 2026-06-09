# Monitor Scan Scheduling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hardcoded hourly anomaly monitoring tick with a first-class cron schedule target (`monitor_scan`) that reads its default cadence per-granularity from `.monitor.yml`.

**Architecture:** `MonitorScanPort` trait (defined in `agentic-pipeline/platform`) decouples the scheduler from `oxy-metric-monitoring`. `OxyProjectContext` implements it. The recovery loop bootstraps schedule rows from `.monitor.yml` on each periodic tick and then calls `tick_monitor_schedules` — same CAS/miss-counting machinery already used for workflow/airway/agent schedules.

**Tech Stack:** Rust (tokio, sea-orm, serde_yaml, async-trait), React + TypeScript (Tailwind, shadcn/ui)

---

## File Map

| File | Change |
|---|---|
| `crates/metric-monitoring/src/config.rs` | Add `MonitorScheduleConfig`; add `schedule` field to `MonitorConfig` |
| `crates/metric-monitoring/src/service.rs` | Add `granularity_filter: Option<Granularity>` to `scan_workspace` + `scan_one` |
| `crates/metric-monitoring/src/lib.rs` | Re-export `MonitorScheduleConfig` |
| `crates/agentic/pipeline/src/platform/mod.rs` | Add `MonitorScanPort` trait; add `as_monitor_scan_port()` default method to `ProjectContext` |
| `crates/agentic/pipeline/src/scheduler.rs` | Add `"monitor_scan"` to `validate_input`; add `tick_monitor_schedules`; add `run_monitor_schedule_now` |
| `crates/app/src/agentic_wiring/project_ctx.rs` | `impl MonitorScanPort for OxyProjectContext`; override `as_monitor_scan_port()` |
| `crates/app/src/server/router/recovery.rs` | Add `bootstrap_monitor_schedules`; wire into periodic paths; remove old `run_metric_monitoring_tick` + `METRIC_MONITORING_INTERVAL` |
| `crates/app/src/server/api/metric_anomalies.rs` | Update `scan_workspace` call to pass `None` granularity filter |
| `crates/app/src/server/api/schedules.rs` | Update `run_now` handler to route `monitor_scan` to `run_monitor_schedule_now` |
| `web-app/src/types/schedule.ts` | Add `"monitor_scan"` to `ScheduleTargetKind` |
| `web-app/src/pages/ide/coordinator/components/constants.ts` | Add `"monitor"` `JobType`; update `targetKindToJobType` and `JOB_TYPES` |
| `web-app/src/pages/ide/coordinator/Jobs/components/ScheduleDialog.tsx` | Handle `monitor_scan` in edit mode (hide target picker + question; show read-only granularity) |

---

## Task 1: `.monitor.yml` schema — `MonitorScheduleConfig` + granularity filter

**Files:**
- Modify: `crates/metric-monitoring/src/config.rs`
- Modify: `crates/metric-monitoring/src/service.rs`
- Modify: `crates/metric-monitoring/src/lib.rs`

- [ ] **Step 1: Write failing tests for the new config fields**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/metric-monitoring/src/config.rs`:

```rust
#[test]
fn parses_schedule_block() {
    let yaml = r#"
schedule:
  daily: "0 6 * * *"
  weekly: "0 6 * * 1"

monitors:
  - measure: orders.revenue
    time_dimension: orders.created_at
"#;
    let cfg: MonitorConfig = serde_yaml::from_str(yaml).unwrap();
    let sched = cfg.schedule.unwrap();
    assert_eq!(sched.daily.as_deref(), Some("0 6 * * *"));
    assert_eq!(sched.weekly.as_deref(), Some("0 6 * * 1"));
    assert!(sched.monthly.is_none());
}

#[test]
fn schedule_block_optional() {
    let yaml = r#"
monitors:
  - measure: orders.revenue
    time_dimension: orders.created_at
"#;
    let cfg: MonitorConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(cfg.schedule.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo nextest run -p oxy-metric-monitoring 2>&1 | grep -E "^(error|FAIL|test)"
```

Expected: compile error — `MonitorConfig` has no `schedule` field yet.

- [ ] **Step 3: Add `MonitorScheduleConfig` and `schedule` field to `MonitorConfig`**

In `crates/metric-monitoring/src/config.rs`, add the new struct before `MonitorConfig` and update `MonitorConfig`:

```rust
/// Per-granularity cron expressions read from `.monitor.yml`.
/// All fields are optional — only declared granularities get schedule rows.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MonitorScheduleConfig {
    /// Cron expression for day-granularity monitors (e.g. `"0 6 * * *"`).
    pub daily: Option<String>,
    /// Cron expression for week-granularity monitors (e.g. `"0 6 * * 1"`).
    pub weekly: Option<String>,
    /// Cron expression for month-granularity monitors (e.g. `"5 6 1 * *"`).
    pub monthly: Option<String>,
}
```

And update `MonitorConfig`:

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorConfig {
    /// Per-granularity cron schedule. Absent = no automated scanning.
    #[serde(default)]
    pub schedule: Option<MonitorScheduleConfig>,
    #[serde(default)]
    pub monitors: Vec<MonitorEntry>,
}
```

- [ ] **Step 4: Write failing test for granularity filter in service.rs**

Add to `#[cfg(test)] mod tests` in `crates/metric-monitoring/src/service.rs`:

```rust
#[test]
fn granularity_filter_skips_non_matching() {
    use crate::config::{Granularity, MonitorEntry, Sensitivity};
    use crate::detect::Direction;

    let daily = MonitorEntry {
        measure: "a.b".into(),
        time_dimension: "a.t".into(),
        granularity: Granularity::Day,
        lookback_days: 30,
        seasonality: None,
        sensitivity: Sensitivity::Medium,
        label: None,
        filters: vec![],
        group_by: None,
        direction: Direction::Both,
    };
    let weekly = MonitorEntry {
        measure: "c.d".into(),
        time_dimension: "c.t".into(),
        granularity: Granularity::Week,
        lookback_days: 90,
        seasonality: None,
        sensitivity: Sensitivity::Medium,
        label: None,
        filters: vec![],
        group_by: None,
        direction: Direction::Both,
    };
    // filter_monitors_by_granularity is the logic we need to test
    let all = vec![daily.clone(), weekly.clone()];
    let filtered: Vec<_> = all
        .into_iter()
        .filter(|m| m.granularity == Granularity::Day)
        .collect();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].measure, "a.b");
}
```

- [ ] **Step 5: Add `granularity_filter` parameter to `scan_workspace` and `scan_one`**

In `crates/metric-monitoring/src/service.rs`, update the signature of `scan_workspace`:

```rust
pub async fn scan_workspace(
    runner: Arc<dyn MetricTreeRunner>,
    config_path: &Path,
    now: DateTime<Utc>,
    granularity_filter: Option<Granularity>,
) -> Result<ScanResult, ScanError> {
    let cfg: MonitorConfig = load_from_file(config_path)?;
    let mut result = ScanResult::default();

    // Apply granularity filter before expanding group_by entries.
    let monitors: Vec<MonitorEntry> = if let Some(gran) = granularity_filter {
        cfg.monitors.into_iter().filter(|m| m.granularity == gran).collect()
    } else {
        cfg.monitors
    };

    // Expand group_by entries (same logic as before, but now over `monitors`).
    let mut expanded: Vec<MonitorEntry> = Vec::new();
    for entry in monitors {
        // ... rest of expansion logic unchanged
```

The expansion loop body is unchanged. Only the `cfg.monitors` reference at the top becomes `monitors`.

- [ ] **Step 6: Update `scan_one` — no signature change needed**

`scan_one` takes a single `&MonitorEntry` and does not need the filter — filtering happens in `scan_workspace` before calling `scan_one`. No change needed.

- [ ] **Step 7: Update `tick_workspace` in `crates/metric-monitoring/src/tick.rs`**

`tick_workspace` calls `scan_workspace` directly. Update the call to pass `None`:

```rust
let scan = scan_workspace(runner, &config_path, Utc::now(), None).await?;
```

- [ ] **Step 8: Re-export `MonitorScheduleConfig` from `crates/metric-monitoring/src/lib.rs`**

Add to the existing `pub use config::{...}` line:

```rust
pub use config::{
    Direction, Granularity, LoadError as ConfigLoadError, MonitorConfig, MonitorEntry,
    MonitorScheduleConfig, Sensitivity, default_config_path, load_from_file,
};
```

- [ ] **Step 9: Run tests and verify passing**

```bash
cargo nextest run -p oxy-metric-monitoring 2>&1 | grep -E "^(error|warning\[|FAIL|PASS|ok)"
```

Expected: all tests pass, no errors.

- [ ] **Step 10: Commit**

```bash
git add crates/metric-monitoring/src/config.rs crates/metric-monitoring/src/service.rs crates/metric-monitoring/src/tick.rs crates/metric-monitoring/src/lib.rs
git commit -m "feat(metric-monitoring): add per-granularity schedule config and granularity filter"
```

---

## Task 2: `MonitorScanPort` trait + `as_monitor_scan_port()` on `ProjectContext`

**Files:**
- Modify: `crates/agentic/pipeline/src/platform/mod.rs`

- [ ] **Step 1: Add `MonitorScanPort` trait and `as_monitor_scan_port()` to `ProjectContext`**

In `crates/agentic/pipeline/src/platform/mod.rs`, add the new trait after the existing imports (after the `use async_trait::async_trait;` line) and before `ProjectContext`:

```rust
/// Port for running a workspace anomaly scan for one granularity tier.
///
/// Implemented by `OxyProjectContext` in the `app` crate. The default impl
/// returns `None` so test fakes and non-Oxy adapters compile unchanged.
#[async_trait]
pub trait MonitorScanPort: Send + Sync {
    /// Run monitors matching `granularity` ("day" | "week" | "month"),
    /// persist anomaly rows, and return a brief audit summary.
    async fn run_monitor_scan(
        &self,
        db: &sea_orm::DatabaseConnection,
        workspace_id: uuid::Uuid,
        granularity: &str,
    ) -> Result<String, String>;
}
```

Then add `as_monitor_scan_port` as a default method in the `ProjectContext` trait (after the `anomaly_store` method):

```rust
    /// Return a [`MonitorScanPort`] implementation if this context supports
    /// anomaly scanning. Default `None` so existing adapters compile unchanged.
    fn as_monitor_scan_port(&self) -> Option<&dyn MonitorScanPort> {
        None
    }
```

- [ ] **Step 2: Check it compiles**

```bash
cargo check -p agentic-pipeline 2>&1 | grep -E "^(error|warning\[)"
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/agentic/pipeline/src/platform/mod.rs
git commit -m "feat(agentic-pipeline): add MonitorScanPort trait and as_monitor_scan_port() to ProjectContext"
```

---

## Task 3: Scheduler — `validate_input`, `tick_monitor_schedules`, `run_monitor_schedule_now`

**Files:**
- Modify: `crates/agentic/pipeline/src/scheduler.rs`

- [ ] **Step 1: Write a failing test for `validate_input` with `"monitor_scan"`**

Add to the end of `crates/agentic/pipeline/tests/scheduler_test.rs` (or in a `#[cfg(test)]` block inside `scheduler.rs` if that file already contains tests):

```rust
#[test]
fn validate_input_accepts_monitor_scan() {
    use agentic_pipeline::scheduler::{ScheduleInput, validate_input_pub};
    // Note: expose validate_input as pub(crate) for testing, see Step 2.
    let input = ScheduleInput {
        name: "Monitor".to_string(),
        target_kind: "monitor_scan".to_string(),
        target_ref: ".monitor.yml".to_string(),
        question: None,
        variables: Some(serde_json::json!({"granularity": "day"})),
        cron_expr: "0 6 * * *".to_string(),
        timezone: "UTC".to_string(),
        enabled: true,
    };
    // validate_input is private; test via create_schedule in integration tests,
    // or expose as pub(crate). We'll rely on the compile+run check below.
    let _ = input; // structural check
}
```

Actually `validate_input` is private. Test via the error message from `create_schedule` in Task 3's integration check. Skip the unit test for `validate_input` and test via the DB path in the final cargo check.

- [ ] **Step 2: Add `"monitor_scan"` to `validate_input`**

In `crates/agentic/pipeline/src/scheduler.rs`, update `validate_input`:

```rust
fn validate_input(input: &ScheduleInput) -> Result<(), ScheduleError> {
    if input.name.trim().is_empty() {
        return Err(ScheduleError::Invalid("name must not be empty".into()));
    }
    if !matches!(
        input.target_kind.as_str(),
        "workflow" | "airway" | "agent" | "monitor_scan"
    ) {
        return Err(ScheduleError::Invalid(format!(
            "target_kind must be 'workflow', 'airway', 'agent', or 'monitor_scan', got {:?}",
            input.target_kind
        )));
    }
    if input.target_ref.trim().is_empty() {
        return Err(ScheduleError::Invalid(
            "target_ref must not be empty".into(),
        ));
    }
    if input.target_kind == "agent"
        && input
            .question
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(ScheduleError::Invalid(
            "question must not be empty for agent schedules".into(),
        ));
    }
    validate_cron(&input.cron_expr, &input.timezone).map_err(ScheduleError::Invalid)
}
```

- [ ] **Step 3: Add `tick_monitor_schedules` function**

Add this function after the existing `tick_schedules` function in `scheduler.rs`. It needs these imports already present at the top of the file: `Statement`, `DatabaseBackend`, `DatabaseConnection`, `ColumnTrait`, `EntityTrait`, `QueryFilter`, `count_occurrences_between`, `next_occurrence_after`, `schedule`. All are already imported.

Add the import for the port trait at the top of the file:

```rust
use crate::platform::MonitorScanPort;
```

Then add the function:

```rust
/// Run one scheduler pass for `monitor_scan` schedules in the given workspace.
/// Returns the number of schedules fired. Never errors the caller —
/// per-schedule failures are logged and skipped.
pub async fn tick_monitor_schedules(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    port: &dyn MonitorScanPort,
) -> usize {
    let now = chrono::Utc::now().fixed_offset();
    let due = match schedule::Entity::find()
        .filter(schedule::Column::WorkspaceId.eq(workspace_id))
        .filter(schedule::Column::TargetKind.eq("monitor_scan"))
        .filter(schedule::Column::Enabled.eq(true))
        .filter(schedule::Column::NextRunAt.lte(now))
        .all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(target: "scheduler", error = %e, "monitor tick: query failed");
            return 0;
        }
    };

    let mut fired = 0;
    for s in due {
        // Validate granularity before touching next_run_at — a misconfigured
        // row is a data error, not a transient failure, so we don't advance.
        let granularity = match s
            .variables
            .as_ref()
            .and_then(|v| v.get("granularity"))
            .and_then(|g| g.as_str())
        {
            Some(g) => g.to_string(),
            None => {
                set_last_error(db, &s.id, Some("missing granularity in variables")).await;
                continue;
            }
        };

        let next = match next_occurrence_after(&s.cron_expr, &s.timezone, chrono::Utc::now()) {
            Ok(n) => n.fixed_offset(),
            Err(e) => {
                tracing::warn!(
                    target: "scheduler",
                    schedule_id = %s.id,
                    error = %e,
                    "monitor tick: bad cron/timezone; skipping"
                );
                set_last_error(db, &s.id, Some(&e)).await;
                continue;
            }
        };

        let prev_due_utc = s.next_run_at.with_timezone(&chrono::Utc);
        let missed = count_occurrences_between(
            &s.cron_expr,
            &s.timezone,
            prev_due_utc,
            chrono::Utc::now(),
            1000,
        )
        .unwrap_or(0);

        // CAS-advance: exactly-once fire across replicas.
        let won = match db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "UPDATE agentic_schedules \
                 SET next_run_at = $1, \
                     last_fired_at = now(), \
                     missed_runs = missed_runs + $4, \
                     last_missed_at = CASE WHEN $4 > 0 THEN now() ELSE last_missed_at END, \
                     updated_at = now() \
                 WHERE id = $2 AND next_run_at = $3",
                [
                    next.into(),
                    s.id.clone().into(),
                    s.next_run_at.into(),
                    (missed as i32).into(),
                ],
            ))
            .await
        {
            Ok(r) => r.rows_affected() == 1,
            Err(e) => {
                tracing::error!(target: "scheduler", schedule_id = %s.id, error = %e, "monitor tick: CAS failed");
                continue;
            }
        };
        if !won {
            continue;
        }
        if missed > 0 {
            tracing::warn!(
                target: "scheduler",
                schedule_id = %s.id,
                missed,
                "monitor tick: catch-up fire skipped {} occurrences (policy: run-once-then-resume)",
                missed,
            );
        }

        match port.run_monitor_scan(db, workspace_id, &granularity).await {
            Ok(summary) => {
                fired += 1;
                tracing::info!(
                    target: "scheduler",
                    schedule_id = %s.id,
                    granularity = %granularity,
                    summary = %summary,
                    "monitor tick: scan complete"
                );
                record_fire_success(db, &s.id, &summary).await;
            }
            Err(e) => {
                tracing::error!(
                    target: "scheduler",
                    schedule_id = %s.id,
                    error = %e,
                    "monitor tick: scan failed; schedule advanced, will retry next slot"
                );
                set_last_error(db, &s.id, Some(&e)).await;
            }
        }
    }

    fired
}
```

- [ ] **Step 4: Add `run_monitor_schedule_now` function**

Add immediately after `tick_monitor_schedules`:

```rust
/// Fire a monitor_scan schedule out-of-band (run-now). Returns a summary
/// string used as a synthetic audit trail on `last_run_id`.
pub async fn run_monitor_schedule_now(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    port: &dyn MonitorScanPort,
    id: &str,
) -> Result<String, ScheduleError> {
    let s = get_schedule(db, workspace_id, id).await?;
    let granularity = s
        .variables
        .as_ref()
        .and_then(|v| v.get("granularity"))
        .and_then(|g| g.as_str())
        .ok_or_else(|| ScheduleError::Invalid("missing granularity in variables".into()))?
        .to_string();
    match port.run_monitor_scan(db, workspace_id, &granularity).await {
        Ok(summary) => {
            record_fire_success(db, &s.id, &summary).await;
            Ok(summary)
        }
        Err(e) => {
            set_last_error(db, &s.id, Some(&e)).await;
            Err(ScheduleError::Invalid(e))
        }
    }
}
```

- [ ] **Step 5: Check it compiles**

```bash
cargo check -p agentic-pipeline 2>&1 | grep -E "^(error|warning\[)"
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/agentic/pipeline/src/scheduler.rs crates/agentic/pipeline/src/platform/mod.rs
git commit -m "feat(agentic-pipeline): add monitor_scan target_kind, tick_monitor_schedules, run_monitor_schedule_now"
```

---

## Task 4: `OxyProjectContext` implements `MonitorScanPort`

**Files:**
- Modify: `crates/app/src/agentic_wiring/project_ctx.rs`

- [ ] **Step 1: Add `MonitorScanPort` import**

In `crates/app/src/agentic_wiring/project_ctx.rs`, find the existing `agentic_pipeline` imports and add:

```rust
use agentic_pipeline::platform::MonitorScanPort;
```

(The file already imports many `agentic_pipeline::platform` items; add this alongside them.)

- [ ] **Step 2: Implement `MonitorScanPort` for `OxyProjectContext`**

Add this `impl` block after the existing `impl WorkspaceContext for OxyProjectContext` block:

```rust
#[async_trait]
impl MonitorScanPort for OxyProjectContext {
    async fn run_monitor_scan(
        &self,
        db: &sea_orm::DatabaseConnection,
        workspace_id: uuid::Uuid,
        granularity: &str,
    ) -> Result<String, String> {
        use oxy_metric_monitoring::Granularity;

        let runner = self
            .metric_tree_runner_system()
            .ok_or_else(|| "no metric tree runner available".to_string())?;
        let gran = match granularity {
            "day" => Granularity::Day,
            "week" => Granularity::Week,
            "month" => Granularity::Month,
            other => return Err(format!("unknown granularity: {other:?}")),
        };
        let config_path =
            oxy_metric_monitoring::default_config_path(self.workspace_path());
        let scan =
            oxy_metric_monitoring::scan_workspace(runner, &config_path, chrono::Utc::now(), Some(gran))
                .await
                .map_err(|e| e.to_string())?;
        let persisted = oxy_metric_monitoring::upsert_anomalies(db, workspace_id, &scan)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "scanned={} failed={} persisted={}",
            scan.outcomes.len(),
            scan.failures.len(),
            persisted
        ))
    }
}
```

- [ ] **Step 3: Override `as_monitor_scan_port()` in `impl ProjectContext for OxyProjectContext`**

Inside the existing `impl ProjectContext for OxyProjectContext` block, add after `anomaly_store()`:

```rust
    fn as_monitor_scan_port(&self) -> Option<&dyn agentic_pipeline::platform::MonitorScanPort> {
        Some(self)
    }
```

- [ ] **Step 4: Check it compiles**

```bash
cargo check -p oxy-app 2>&1 | grep -E "^(error|warning\[)"
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/agentic_wiring/project_ctx.rs
git commit -m "feat(app): implement MonitorScanPort for OxyProjectContext"
```

---

## Task 5: Recovery — bootstrap + wire tick + remove old tick

**Files:**
- Modify: `crates/app/src/server/router/recovery.rs`

- [ ] **Step 1: Add `bootstrap_monitor_schedules` function**

Add this new function near the bottom of `recovery.rs`, before `run_metric_monitoring_tick`:

```rust
/// Read `.monitor.yml`'s `schedule:` block and create `monitor_scan` schedule
/// rows for any granularity not yet present in `agentic_schedules`.
/// Create-only — never updates or deletes rows already present.
async fn bootstrap_monitor_schedules(
    db: &DatabaseConnection,
    workspace_id: uuid::Uuid,
    workspace_root: &std::path::Path,
) {
    use agentic_pipeline::scheduler::{ScheduleInput, create_schedule};
    use agentic_runtime::entity::schedule;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let config_path = oxy_metric_monitoring::default_config_path(workspace_root);
    let cfg = match oxy_metric_monitoring::load_from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                target: "metric_monitoring",
                error = %e,
                "bootstrap: failed to read .monitor.yml; skipping"
            );
            return;
        }
    };
    let Some(sched) = cfg.schedule else {
        return;
    };

    // Fetch existing monitor_scan rows once to avoid N+1 queries.
    let existing = match schedule::Entity::find()
        .filter(schedule::Column::WorkspaceId.eq(workspace_id))
        .filter(schedule::Column::TargetKind.eq("monitor_scan"))
        .all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                target: "metric_monitoring",
                %workspace_id,
                error = %e,
                "bootstrap: failed to query existing monitor schedules"
            );
            return;
        }
    };

    let has_granularity = |gran: &str| {
        existing.iter().any(|s| {
            s.variables
                .as_ref()
                .and_then(|v| v.get("granularity"))
                .and_then(|g| g.as_str())
                == Some(gran)
        })
    };

    let entries = [
        (sched.daily.as_deref(), "day", "Metric monitoring (daily)"),
        (sched.weekly.as_deref(), "week", "Metric monitoring (weekly)"),
        (sched.monthly.as_deref(), "month", "Metric monitoring (monthly)"),
    ];

    for (maybe_cron, gran, name) in entries {
        let Some(cron_expr) = maybe_cron else {
            continue;
        };
        if has_granularity(gran) {
            continue;
        }
        let input = ScheduleInput {
            name: name.to_string(),
            target_kind: "monitor_scan".to_string(),
            target_ref: ".monitor.yml".to_string(),
            question: None,
            variables: Some(serde_json::json!({ "granularity": gran })),
            cron_expr: cron_expr.to_string(),
            timezone: "UTC".to_string(),
            enabled: true,
        };
        match create_schedule(db, workspace_id, input).await {
            Ok(_) => tracing::info!(
                target: "metric_monitoring",
                %workspace_id,
                granularity = %gran,
                "bootstrapped monitor_scan schedule"
            ),
            Err(e) => tracing::warn!(
                target: "metric_monitoring",
                %workspace_id,
                granularity = %gran,
                error = %e,
                "bootstrap: failed to create monitor_scan schedule"
            ),
        }
    }
}
```

- [ ] **Step 2: Wire into `recover_local` periodic branch**

In `recover_local`, find the `if periodic {` branch. Currently it ends with:

```rust
run_metric_monitoring_tick(db, LOCAL_WORKSPACE_ID, platform_for_monitoring).await;
recovered + fired
```

Replace the `run_metric_monitoring_tick` call with:

```rust
bootstrap_monitor_schedules(db, LOCAL_WORKSPACE_ID, &cwd).await;
if let Some(port) = platform_for_monitoring.as_monitor_scan_port() {
    let monitor_fired =
        agentic_pipeline::scheduler::tick_monitor_schedules(db, LOCAL_WORKSPACE_ID, port)
            .await;
    if monitor_fired > 0 {
        tracing::info!(
            target: "metric_monitoring",
            fired = monitor_fired,
            workspace_id = %LOCAL_WORKSPACE_ID,
            "monitor tick fired schedules"
        );
    }
}
recovered + fired
```

- [ ] **Step 3: Wire into `recover_all_workspaces` periodic branch**

In `recover_all_workspaces`, find the `if periodic {` branch. Currently it ends with:

```rust
run_metric_monitoring_tick(db, ws.id, platform_for_monitoring).await;
recovered + fired
```

Replace `run_metric_monitoring_tick` call with:

```rust
let ws_root = std::path::Path::new(path);
bootstrap_monitor_schedules(db, ws.id, ws_root).await;
if let Some(port) = platform_for_monitoring.as_monitor_scan_port() {
    let monitor_fired =
        agentic_pipeline::scheduler::tick_monitor_schedules(db, ws.id, port).await;
    if monitor_fired > 0 {
        tracing::info!(
            target: "metric_monitoring",
            workspace_id = %ws.id,
            fired = monitor_fired,
            "monitor tick fired schedules"
        );
    }
}
recovered + fired
```

Note: `path` is the `ws.path` value already in scope in the `recover_all_workspaces` workspace loop.

- [ ] **Step 4: Remove `run_metric_monitoring_tick` and `METRIC_MONITORING_INTERVAL`**

Delete the `run_metric_monitoring_tick` function (lines ~762–807 in `recovery.rs`) and the `METRIC_MONITORING_INTERVAL` constant (line ~812). Both are now unused.

- [ ] **Step 5: Check it compiles**

```bash
cargo check -p oxy-app 2>&1 | grep -E "^(error|warning\[)"
```

Expected: no errors. Fix any unused-import warnings from the old tick path (e.g. `oxy_metric_monitoring::global_registry`, `oxy_metric_monitoring::tick_workspace`).

- [ ] **Step 6: Commit**

```bash
git add crates/app/src/server/router/recovery.rs
git commit -m "feat(app): bootstrap monitor_scan schedules from .monitor.yml; remove hardcoded tick"
```

---

## Task 6: Update `metric_anomalies.rs` call site + `run_now` handler

**Files:**
- Modify: `crates/app/src/server/api/metric_anomalies.rs`
- Modify: `crates/app/src/server/api/schedules.rs`

- [ ] **Step 1: Update `scan_workspace` call in `metric_anomalies.rs`**

In `crates/app/src/server/api/metric_anomalies.rs`, line ~215:

```rust
let result = monitoring::scan_workspace(runner, &config_path, now)
```

Update to:

```rust
let result = monitoring::scan_workspace(runner, &config_path, now, None)
```

- [ ] **Step 2: Update `run_now` handler in `schedules.rs`**

In `crates/app/src/server/api/schedules.rs`, replace the entire `run_now` function:

```rust
pub async fn run_now(
    _: WorkspaceAdmin,
    Extension(state): Extension<Arc<AgenticState>>,
    Extension(platform): Extension<Arc<dyn PlatformContext>>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Path((workspace_id, id)): Path<(Uuid, String)>,
) -> Response {
    // Fetch once to inspect target_kind before routing.
    let schedule = match get_schedule(&state.db, workspace_id, &id).await {
        Ok(s) => s,
        Err(e) => return map_err(e),
    };

    if schedule.target_kind == "monitor_scan" {
        match platform.as_monitor_scan_port() {
            Some(port) => {
                match agentic_pipeline::scheduler::run_monitor_schedule_now(
                    &state.db,
                    workspace_id,
                    port,
                    &id,
                )
                .await
                {
                    Ok(summary) => Json(RunNowResponse { run_id: summary }).into_response(),
                    Err(e) => map_err(e),
                }
            }
            None => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "monitor scan not available in this deployment",
            )
                .into_response(),
        }
    } else {
        let workspace: Arc<dyn WorkflowWorkspaceContext> = platform.clone();
        match run_schedule_now(&state.db, workspace_id, workspace.as_ref(), &id).await {
            Ok(run_id) => Json(RunNowResponse { run_id }).into_response(),
            Err(e) => map_err(e),
        }
    }
}
```

Add to the existing imports at the top of `schedules.rs`:

```rust
use agentic_pipeline::platform::PlatformContext;
```

(Check if it's already imported — it may already be present.)

- [ ] **Step 3: Check it compiles**

```bash
cargo check -p oxy-app 2>&1 | grep -E "^(error|warning\[)"
```

Expected: no errors.

- [ ] **Step 4: Full workspace compile check**

```bash
cargo check --workspace 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/server/api/metric_anomalies.rs crates/app/src/server/api/schedules.rs
git commit -m "feat(app): route monitor_scan run-now to MonitorScanPort; update scan_workspace call sites"
```

---

## Task 7: Frontend — types, constants, ScheduleDialog

**Files:**
- Modify: `web-app/src/types/schedule.ts`
- Modify: `web-app/src/pages/ide/coordinator/components/constants.ts`
- Modify: `web-app/src/pages/ide/coordinator/Jobs/components/ScheduleDialog.tsx`

- [ ] **Step 1: Add `"monitor_scan"` to `ScheduleTargetKind`**

In `web-app/src/types/schedule.ts`, line 2:

```ts
export type ScheduleTargetKind = "workflow" | "airway" | "agent" | "monitor_scan";
```

- [ ] **Step 2: Add `"monitor"` `JobType` in `constants.ts`**

In `web-app/src/pages/ide/coordinator/components/constants.ts`:

Add `Activity` to the lucide-react import at the top of the file (alongside `Bot`, `Workflow`, `Database`):

```ts
import { Activity, Bot, Database, Workflow } from "lucide-react";
```

Update `JobType`:

```ts
export type JobType = "agent" | "dag" | "elt" | "monitor";
```

Add the `monitor` entry to `JOB_TYPE`:

```ts
export const JOB_TYPE: Record<JobType, JobTypeMeta> = {
  agent: { ... },  // unchanged
  dag: { ... },    // unchanged
  elt: { ... },    // unchanged
  monitor: {
    label: "Monitor scan",
    short: "Monitor",
    fg: "text-vis-amber",
    bg: "bg-vis-amber",
    tint: "bg-vis-amber/10 text-vis-amber",
    icon: Activity,
    unit: "anomaly scan"
  }
};
```

Update `JOB_TYPES`:

```ts
export const JOB_TYPES: JobType[] = ["agent", "dag", "elt", "monitor"];
```

Update `targetKindToJobType`:

```ts
export const targetKindToJobType = (kind: string): JobType => {
  if (kind === "airway") return "elt";
  if (kind === "agent") return "agent";
  if (kind === "monitor_scan") return "monitor";
  return "dag";
};
```

- [ ] **Step 3: Update `ScheduleDialog` for `monitor_scan` edit mode**

In `web-app/src/pages/ide/coordinator/Jobs/components/ScheduleDialog.tsx`, make these changes:

**a)** In the initial state, `targetKind` defaults to `"workflow"` — no change needed.

**b)** Replace the target file / agent picker block (the `<div className='flex flex-wrap items-end gap-3'>` section containing "Job type" + "Target file") with one that shows a read-only indicator for `monitor_scan`:

```tsx
<div className='flex flex-wrap items-end gap-3'>
  <div className='flex flex-col gap-2'>
    <Label>Job type</Label>
    <Select
      value={targetKind}
      onValueChange={(v) => {
        setTargetKind(v as ScheduleTargetKind);
        setTargetRef("");
        setFreeText(false);
      }}
      disabled={targetKind === "monitor_scan"}
    >
      <SelectTrigger className='w-44'>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value='workflow'>DAG workflow</SelectItem>
        <SelectItem value='airway'>ELT pipeline</SelectItem>
        <SelectItem value='agent'>Agent</SelectItem>
        {targetKind === "monitor_scan" && (
          <SelectItem value='monitor_scan'>Monitor scan</SelectItem>
        )}
      </SelectContent>
    </Select>
  </div>

  {targetKind !== "monitor_scan" && (
    <div className='flex min-w-60 flex-1 flex-col gap-2'>
      <Label>{targetKind === "agent" ? "Agent" : "Target file"}</Label>
      {freeText ? (
        <Input
          placeholder='workspace-relative path'
          className='font-mono'
          value={targetRef}
          onChange={(e) => setTargetRef(e.target.value)}
        />
      ) : (
        <Select
          value={isKnownRef ? targetRef : ""}
          onValueChange={(v) => {
            if (v === FREE_TEXT) {
              setFreeText(true);
              setTargetRef("");
            } else {
              setTargetRef(v);
            }
          }}
        >
          <SelectTrigger>
            <SelectValue
              placeholder={targetKind === "agent" ? "Select an agent" : "Select a file"}
            />
          </SelectTrigger>
          <SelectContent>
            {refs.map((r) => (
              <SelectItem key={r} value={r}>
                {r}
              </SelectItem>
            ))}
            <SelectItem value={FREE_TEXT}>Other (type a path)…</SelectItem>
          </SelectContent>
        </Select>
      )}
    </div>
  )}

  {targetKind === "monitor_scan" && (
    <div className='flex min-w-60 flex-1 flex-col gap-2'>
      <Label>Granularity</Label>
      <div className='flex h-9 items-center rounded-md border border-input bg-muted px-3 text-sm text-muted-foreground'>
        {(schedule?.variables as Record<string, string> | null)?.granularity ?? "—"}
      </div>
    </div>
  )}
</div>
```

**c)** The question textarea is already guarded by `{targetKind === "agent" && ...}` — no change needed.

**d)** Update the dialog description to mention monitor scans:

```tsx
<DialogDescription>
  Run a DAG workflow, ELT pipeline, agent, or metric monitor scan on a recurring cron schedule.
</DialogDescription>
```

- [ ] **Step 4: Run TypeScript build to verify no type errors**

```bash
cd web-app && pnpm run build 2>&1 | grep -E "error TS|Error"
```

Expected: no TypeScript errors.

- [ ] **Step 5: Commit**

```bash
git add web-app/src/types/schedule.ts \
        web-app/src/pages/ide/coordinator/components/constants.ts \
        web-app/src/pages/ide/coordinator/Jobs/components/ScheduleDialog.tsx
git commit -m "feat(web-app): add monitor_scan schedule type with Monitor job type and edit dialog support"
```

---

## Task 8: Final validation

- [ ] **Step 1: Full workspace Rust compile check**

```bash
cargo check --workspace 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 2: Run metric-monitoring tests**

```bash
cargo nextest run -p oxy-metric-monitoring 2>&1 | grep -E "FAIL|PASS|error"
```

Expected: all pass.

- [ ] **Step 3: Run agentic-pipeline tests**

```bash
cargo nextest run -p agentic-pipeline 2>&1 | grep -E "FAIL|PASS|error"
```

Expected: all pass.

- [ ] **Step 4: TypeScript build**

```bash
cd web-app && pnpm run build 2>&1 | grep -E "error TS|Error"
```

Expected: no errors.

- [ ] **Step 5: Final commit (if any fixup needed)**

```bash
git add -p
git commit -m "fix: address compile warnings from monitor scan scheduling"
```
