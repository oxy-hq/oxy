//! Prometheus exposition for the `oxy worker` process.
//!
//! Single endpoint, `GET /metrics`, mounted on the same tiny health
//! server that already exposes `/healthz` and `/readyz`. The format
//! is the documented Prometheus text exposition; we hand-roll it to
//! avoid pulling in a heavyweight client crate for ~6 metrics.
//!
//! What gets exposed:
//!
//! | Metric | Type | Labels | Source |
//! |---|---|---|---|
//! | `oxy_queue_depth_queued` | gauge | `task_kind` | `agentic_task_queue` GROUP BY |
//! | `oxy_queue_depth_claimed` | gauge | `task_kind` | same |
//! | `oxy_queue_depth_dead` | gauge | `task_kind` | same |
//! | `oxy_worker_capacity` | gauge | `task_kind` | per-kind env caps |
//! | `oxy_worker_info` | gauge=1 | `worker_id`, `version` | identity |
//! | `oxy_metrics_scrape_db_ok` | gauge=0/1 | (none) | DB read status this scrape |
//!
//! Per-process inflight counters (current concurrent tasks per
//! worker) aren't surfaced yet because the orchestrator doesn't
//! expose them out of the box; they'll land alongside `claimed_by`
//! observability (refinement E of the scaling design). Scrapers
//! should derive in-flight from `oxy_queue_depth_claimed` summed
//! over the fleet meanwhile.
//!
//! The HPA on the worker fleet reads `oxy_queue_depth_queued` summed
//! across task_kinds (or per kind) and scales replicas to keep it
//! within a target band. See `internal-docs/compile-boundary.md`
//! for the operator runbook.
//!
//! Failure mode: if the DB read errors, we emit the in-process
//! metrics anyway and surface scrape health via the separate
//! `oxy_metrics_scrape_db_ok` gauge (1 = ok, 0 = the DB read failed
//! this scrape). HPA stays correct (treats the absence of queued tasks
//! as "no scale-up signal") rather than panicking on stale data.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sea_orm::{DatabaseBackend, DatabaseConnection, FromQueryResult, Statement};

#[derive(Clone)]
pub struct MetricsState {
    pub worker_id: Arc<String>,
    pub version: &'static str,
    pub db: Arc<DatabaseConnection>,
    /// Per-task-kind concurrency caps, surfaced for HPA target sizing
    /// (HPA can compare `queue_depth_queued` / `worker_capacity` to
    /// pick the right number of replicas).
    pub capacity: ConcurrencyCaps,
}

#[derive(Clone, Copy, Debug)]
pub struct ConcurrencyCaps {
    pub compile: u32,
    pub agent: u32,
    pub other: u32,
}

impl ConcurrencyCaps {
    /// All three caps share the single global concurrency knob
    /// (`OXY_WORKER_MAX_INFLIGHT`, default 32). The per-kind labels
    /// are kept so the HPA dashboard query (queue_depth / capacity
    /// per task_kind) stays well-defined; we just emit the same
    /// number for each label.
    pub fn from_env() -> Self {
        let global = std::env::var("OXY_WORKER_MAX_INFLIGHT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(32);
        Self {
            compile: global,
            agent: global,
            other: global,
        }
    }
}

/// Hand-rolled Prometheus exposition. Quick path that doesn't need a
/// metrics client crate — six gauges total.
pub async fn metrics(State(state): State<MetricsState>) -> Response {
    let mut body = String::new();

    // ── worker identity (gauge=1, labels carry the info) ─────────────
    body.push_str(
        "# HELP oxy_worker_info Worker identity (worker_id + version), value is always 1.\n",
    );
    body.push_str("# TYPE oxy_worker_info gauge\n");
    body.push_str(&format!(
        "oxy_worker_info{{worker_id=\"{}\",version=\"{}\"}} 1\n",
        escape_label(&state.worker_id),
        escape_label(state.version)
    ));

    // ── capacity ─────────────────────────────────────────────────────
    body.push_str(
        "# HELP oxy_worker_capacity Per-task-kind inflight cap for this worker process.\n",
    );
    body.push_str("# TYPE oxy_worker_capacity gauge\n");
    body.push_str(&format!(
        "oxy_worker_capacity{{task_kind=\"compile\"}} {}\n",
        state.capacity.compile
    ));
    body.push_str(&format!(
        "oxy_worker_capacity{{task_kind=\"agent\"}} {}\n",
        state.capacity.agent
    ));
    body.push_str(&format!(
        "oxy_worker_capacity{{task_kind=\"other\"}} {}\n",
        state.capacity.other
    ));

    // ── queue depth ─────────────────────────────────────────────────
    // Single query, GROUPed by status + source_type, single round trip.
    let queue_rows = read_queue_depth(&state.db).await;
    let compile_health = read_compile_health(&state.db).await;
    let db_error = queue_rows.is_err() || compile_health.is_err();
    let rows = queue_rows.unwrap_or_default();
    let compile = compile_health.unwrap_or_default();

    body.push_str(
        "# HELP oxy_queue_depth_queued Tasks in 'queued' status, by task kind. HPA scales on this.\n",
    );
    body.push_str("# TYPE oxy_queue_depth_queued gauge\n");
    // Scrape health is reported separately via `oxy_metrics_scrape_db_ok`
    // so the data labels here stay consistent with sibling gauges
    // (`_claimed`, `_dead`) and don't churn series identity between
    // scrapes — which would double the metric cardinality and
    // complicate HPA queries.
    for row in rows.iter().filter(|r| r.queue_status == "queued") {
        body.push_str(&format!(
            "oxy_queue_depth_queued{{task_kind=\"{}\"}} {}\n",
            escape_label(&row.task_kind_label()),
            row.count
        ));
    }

    body.push_str(
        "# HELP oxy_queue_depth_claimed Tasks currently claimed (in flight) by some worker.\n",
    );
    body.push_str("# TYPE oxy_queue_depth_claimed gauge\n");
    for row in rows.iter().filter(|r| r.queue_status == "claimed") {
        body.push_str(&format!(
            "oxy_queue_depth_claimed{{task_kind=\"{}\"}} {}\n",
            escape_label(&row.task_kind_label()),
            row.count
        ));
    }

    body.push_str(
        "# HELP oxy_queue_depth_dead Tasks that hit max_claims and entered the dead-letter state.\n",
    );
    body.push_str("# TYPE oxy_queue_depth_dead gauge\n");
    for row in rows.iter().filter(|r| r.queue_status == "dead") {
        body.push_str(&format!(
            "oxy_queue_depth_dead{{task_kind=\"{}\"}} {}\n",
            escape_label(&row.task_kind_label()),
            row.count
        ));
    }

    // ── compile-boundary health ─────────────────────────────────────
    // Sourced from the shared `revisions` table so a single worker scrape
    // surfaces them fleet-wide. `stuck_compiling` catches crashed compiles
    // (and, since it counts long-running `compiling` rows, also flags a
    // missing/un-draining worker); `promotion_lag` catches "compiles succeed
    // but the workspace pointer didn't move" — the silent regression class.
    body.push_str(
        "# HELP oxy_compile_stuck_compiling Revisions stuck in 'compiling' past the reaper threshold.\n",
    );
    body.push_str("# TYPE oxy_compile_stuck_compiling gauge\n");
    body.push_str(&format!(
        "oxy_compile_stuck_compiling {}\n",
        compile.stuck_compiling
    ));
    body.push_str(
        "# HELP oxy_compile_promotion_lag Recently-ready main revisions not promoted to current_revision_id.\n",
    );
    body.push_str("# TYPE oxy_compile_promotion_lag gauge\n");
    body.push_str(&format!(
        "oxy_compile_promotion_lag {}\n",
        compile.promotion_lag
    ));

    body.push_str(
        "# HELP oxy_metrics_scrape_db_ok Whether the queue-depth DB query succeeded on this scrape.\n",
    );
    body.push_str("# TYPE oxy_metrics_scrape_db_ok gauge\n");
    body.push_str(&format!(
        "oxy_metrics_scrape_db_ok {}\n",
        if db_error { 0 } else { 1 }
    ));

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        body,
    )
        .into_response()
}

#[derive(FromQueryResult, Debug, Clone)]
struct QueueDepthRow {
    queue_status: String,
    source_type: Option<String>,
    count: i64,
}

impl QueueDepthRow {
    fn task_kind_label(&self) -> String {
        // Label-space MUST match the kinds `oxy_worker_capacity` emits
        // (`compile`, `agent`, `other`). The documented HPA recipe joins
        // `oxy_queue_depth_queued` against `oxy_worker_capacity` per
        // `task_kind`; any queue-depth label that has no capacity peer
        // joins to empty and silently breaks the HPA query. Airway and
        // workflow tasks share the generic worker capacity pool and
        // therefore collapse into `other` rather than getting their own
        // label. Unrecognised `source_type` values also fold into
        // `other` so `Custom` tasks with arbitrary `kind` strings don't
        // explode the metric series count.
        match self.source_type.as_deref() {
            Some("compile") => "compile".to_string(),
            Some("analytics") | Some("builder") => "agent".to_string(),
            _ => "other".to_string(),
        }
    }
}

async fn read_queue_depth(db: &DatabaseConnection) -> Result<Vec<QueueDepthRow>, sea_orm::DbErr> {
    // Bounded query — `agentic_task_queue` has both `queue_status` and
    // `source_type` indexed (the existing internal_jobs admin reads
    // similarly). Single round trip per scrape.
    let sql = "\
        SELECT queue_status::text AS queue_status, \
               source_type, \
               COUNT(*)::bigint AS count \
        FROM agentic_task_queue \
        WHERE queue_status IN ('queued', 'claimed', 'dead') \
        GROUP BY queue_status, source_type";
    let stmt = Statement::from_string(DatabaseBackend::Postgres, sql.to_string());
    QueueDepthRow::find_by_statement(stmt).all(db).await
}

#[derive(FromQueryResult, Debug, Clone, Default)]
struct CompileHealthRow {
    stuck_compiling: i64,
    promotion_lag: i64,
}

/// One round trip for both compile-health gauges. `stuck_compiling` uses the
/// `idx_revisions_status_started` partial index; `promotion_lag` is bounded to
/// a 1-hour window so it stays a cheap point-in-time signal rather than a full
/// table scan.
async fn read_compile_health(db: &DatabaseConnection) -> Result<CompileHealthRow, sea_orm::DbErr> {
    let sql = "\
        SELECT \
          (SELECT COUNT(*) FROM revisions \
             WHERE status = 'compiling' \
               AND started_at < now() - interval '15 minutes')::bigint AS stuck_compiling, \
          (SELECT COUNT(*) FROM revisions r \
             JOIN workspaces w ON w.id = r.workspace_id \
             WHERE r.status = 'ready' AND r.kind = 'main' \
               AND r.finished_at > now() - interval '1 hour' \
               AND w.current_revision_id IS DISTINCT FROM r.revision_id)::bigint AS promotion_lag";
    let stmt = Statement::from_string(DatabaseBackend::Postgres, sql.to_string());
    Ok(CompileHealthRow::find_by_statement(stmt)
        .one(db)
        .await?
        .unwrap_or_default())
}

/// Escape a label value per the Prometheus text format spec: `\\`, `\n`, `"`.
fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}

/// Sanity cap on how long a scrape can block the worker process. The
/// scraper times out long before this fires; we set it for defence
/// in depth.
#[allow(dead_code)]
pub const METRICS_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_from_env_share_the_global_pool() {
        // Every task kind reads the same global cap. Per-kind env vars
        // were removed in the compile-boundary simplification — workers
        // share one pool, the per-kind labels just keep the HPA dashboard
        // query well-defined.
        unsafe {
            std::env::set_var("OXY_WORKER_MAX_INFLIGHT", "16");
        }
        let caps = ConcurrencyCaps::from_env();
        assert_eq!(caps.compile, 16);
        assert_eq!(caps.agent, 16);
        assert_eq!(caps.other, 16);
        unsafe {
            std::env::remove_var("OXY_WORKER_MAX_INFLIGHT");
        }
    }

    #[test]
    fn task_kind_label_folds_unknown_to_other() {
        let row = QueueDepthRow {
            queue_status: "queued".into(),
            source_type: Some("never_seen".into()),
            count: 1,
        };
        assert_eq!(row.task_kind_label(), "other");
    }

    #[test]
    fn task_kind_label_matches_capacity_label_space() {
        // `oxy_worker_capacity` only emits `compile` / `agent` / `other`.
        // Queue-depth labels must collapse into the same set so the HPA
        // recipe (queue_depth / capacity join per task_kind) is well-defined.
        // Airway and workflow tasks share the generic worker pool and fold
        // into `other`.
        for (input, expected) in [
            (Some("compile"), "compile"),
            (Some("analytics"), "agent"),
            (Some("builder"), "agent"),
            (Some("airway"), "other"),
            (Some("workflow"), "other"),
            (Some("workflow_step"), "other"),
            (None, "other"),
        ] {
            let row = QueueDepthRow {
                queue_status: "queued".into(),
                source_type: input.map(str::to_string),
                count: 1,
            };
            assert_eq!(row.task_kind_label(), expected, "input={input:?}");
        }
    }
}
