use sea_orm::{DatabaseBackend, DatabaseConnection, DbErr, FromQueryResult, Statement};
use std::collections::HashMap;
use uuid::Uuid;

use super::evaluator::{HealthThresholds, WorkspaceSignals};

#[derive(Debug, FromQueryResult)]
struct RunAggRow {
    workspace_id: Uuid,
    total_runs: i64,
    failed_runs: i64,
    timed_out_runs: i64,
}

#[derive(Debug, FromQueryResult)]
struct AnomalyAggRow {
    workspace_id: Uuid,
    open_high: i64,
    open_medium: i64,
}

#[derive(Debug, FromQueryResult)]
struct DeadLetterRow {
    workspace_id: Uuid,
    dead_letter_count: i64,
}

/// Gather per-workspace signal counts across all tenants in one pass.
///
/// Each sub-query is grouped by `workspace_id`; results are merged in memory.
/// Only workspaces with ANY activity in the window, an open anomaly, or a
/// dead-letter entry appear in the output.
pub async fn gather_signals(
    db: &DatabaseConnection,
    t: &HealthThresholds,
    workspace_id: Option<Uuid>,
) -> Result<Vec<WorkspaceSignals>, DbErr> {
    let mut map: HashMap<Uuid, WorkspaceSignals> = HashMap::new();

    // 1. Run liveness — failed / timed_out / total over the window.
    //
    // `task_status` is nullable in the schema; NULL rows are excluded from the
    // FILTER aggregates naturally (SQL ignores NULLs in COUNT FILTER).
    //
    // make_interval's `hours` param is int4; bind i32 so Postgres doesn't see a
    // bigint (which fails named-arg function resolution). An optional
    // workspace_id filter binds $2 when scoping to a single workspace.
    let mut run_sql = String::from(
        "SELECT workspace_id, \
                COUNT(*)::bigint AS total_runs, \
                COUNT(*) FILTER (WHERE task_status = 'failed')::bigint AS failed_runs, \
                COUNT(*) FILTER (WHERE task_status = 'timed_out')::bigint AS timed_out_runs \
         FROM agentic_runs \
         WHERE created_at > now() - make_interval(hours => $1)",
    );
    let mut run_params: Vec<sea_orm::Value> = vec![(t.window_hours as i32).into()];
    if let Some(ws) = workspace_id {
        run_sql.push_str(" AND workspace_id = $2");
        run_params.push(ws.into());
    }
    run_sql.push_str(" GROUP BY workspace_id");
    let run_rows = RunAggRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        run_sql,
        run_params,
    ))
    .all(db)
    .await?;
    for r in run_rows {
        ensure_entry(&mut map, r.workspace_id);
        let s = map.get_mut(&r.workspace_id).unwrap();
        s.total_runs = r.total_runs;
        s.failed_runs = r.failed_runs;
        s.timed_out_runs = r.timed_out_runs;
    }

    // 2. Open anomalies by severity (status = 'new'). The optional workspace_id
    // filter is the only bound param here, so it binds $1.
    let mut anomaly_sql = String::from(
        "SELECT workspace_id, \
                COUNT(*) FILTER (WHERE severity = 'high')::bigint AS open_high, \
                COUNT(*) FILTER (WHERE severity = 'medium')::bigint AS open_medium \
         FROM metric_anomalies \
         WHERE status = 'new'",
    );
    let mut anomaly_params: Vec<sea_orm::Value> = Vec::new();
    if let Some(ws) = workspace_id {
        anomaly_sql.push_str(" AND workspace_id = $1");
        anomaly_params.push(ws.into());
    }
    anomaly_sql.push_str(" GROUP BY workspace_id");
    let anomaly_rows = AnomalyAggRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        anomaly_sql,
        anomaly_params,
    ))
    .all(db)
    .await?;
    for r in anomaly_rows {
        ensure_entry(&mut map, r.workspace_id);
        let s = map.get_mut(&r.workspace_id).unwrap();
        s.open_high_anomalies = r.open_high;
        s.open_medium_anomalies = r.open_medium;
    }

    // 3. Dead-letter queue entries (queue_status = 'dead').
    //
    // `agentic_task_queue` has no `workspace_id` column; workspace attribution
    // is obtained by joining to `agentic_runs` via the `run_id` FK.
    // The INNER JOIN is safe: `run_id` is NOT NULL with an ON DELETE CASCADE FK,
    // so every dead task has a live run row. Caveat: an orphaned dead task (only
    // possible if a run row is deleted via direct SQL bypassing CASCADE) would be
    // undercounted.
    let mut dl_sql = String::from(
        "SELECT r.workspace_id, COUNT(*)::bigint AS dead_letter_count \
         FROM agentic_task_queue q \
         JOIN agentic_runs r ON r.id = q.run_id \
         WHERE q.queue_status = 'dead'",
    );
    let mut dl_params: Vec<sea_orm::Value> = Vec::new();
    if let Some(ws) = workspace_id {
        dl_sql.push_str(" AND r.workspace_id = $1");
        dl_params.push(ws.into());
    }
    dl_sql.push_str(" GROUP BY r.workspace_id");
    let dl_rows = DeadLetterRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        dl_sql,
        dl_params,
    ))
    .all(db)
    .await?;
    for r in dl_rows {
        ensure_entry(&mut map, r.workspace_id);
        map.get_mut(&r.workspace_id).unwrap().dead_letter_count = r.dead_letter_count;
    }

    // 4. Airway pipeline status on most-recent run per workspace.
    apply_airway_status(db, t, &mut map, workspace_id).await?;

    Ok(map.into_values().collect())
}

/// Mark the Airway flags from each workspace's most recent Airway run in window.
///
/// Airway runs carry `source_type = 'airway'` (set by the HTTP handler in
/// `crates/agentic/http/src/routes/airway.rs` before inserting the run row).
async fn apply_airway_status(
    db: &DatabaseConnection,
    t: &HealthThresholds,
    map: &mut HashMap<Uuid, WorkspaceSignals>,
    workspace_id: Option<Uuid>,
) -> Result<(), DbErr> {
    #[derive(Debug, FromQueryResult)]
    struct AirwayRow {
        workspace_id: Uuid,
        task_status: String,
    }
    // DISTINCT ON returns the latest Airway run per workspace within the window.
    // Window binds $1; the optional workspace_id filter binds $2, before ORDER BY.
    let mut sql = String::from(
        "SELECT DISTINCT ON (workspace_id) workspace_id, \
                COALESCE(task_status, '') AS task_status \
         FROM agentic_runs \
         WHERE source_type = 'airway' \
           AND created_at > now() - make_interval(hours => $1)",
    );
    let mut params: Vec<sea_orm::Value> = vec![(t.window_hours as i32).into()];
    if let Some(ws) = workspace_id {
        sql.push_str(" AND workspace_id = $2");
        params.push(ws.into());
    }
    sql.push_str(" ORDER BY workspace_id, created_at DESC");
    let rows = AirwayRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        params,
    ))
    .all(db)
    .await?;
    for r in rows {
        map.entry(r.workspace_id)
            .or_insert_with(|| blank_signals(r.workspace_id));
        let s = map.get_mut(&r.workspace_id).unwrap();
        s.airway_last_run_failed = r.task_status == "failed" || r.task_status == "timed_out";
        s.airway_completed_with_errors = r.task_status == "completed_with_errors";
    }
    Ok(())
}

/// Human-readable label for a workspace: its own name plus the owning org name
/// (org is nullable). Used to render Slack alerts as "name (org)" instead of a
/// bare UUID.
#[derive(Debug, Clone)]
pub struct WorkspaceLabel {
    pub name: String,
    pub org_name: Option<String>,
}

#[derive(Debug, FromQueryResult)]
struct WorkspaceLabelRow {
    id: Uuid,
    name: String,
    org_name: Option<String>,
}

/// Fetch display labels (workspace name + org name) for every workspace, keyed
/// by id. A LEFT JOIN keeps workspaces whose `org_id` is NULL. Cheap enough to
/// fetch unfiltered — the alert path only looks up the few ids it needs.
pub async fn gather_workspace_labels(
    db: &DatabaseConnection,
) -> Result<HashMap<Uuid, WorkspaceLabel>, DbErr> {
    let rows = WorkspaceLabelRow::find_by_statement(Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT w.id AS id, w.name AS name, o.name AS org_name \
         FROM workspaces w \
         LEFT JOIN organizations o ON o.id = w.org_id"
            .to_string(),
    ))
    .all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                WorkspaceLabel {
                    name: r.name,
                    org_name: r.org_name,
                },
            )
        })
        .collect())
}

fn ensure_entry(map: &mut HashMap<Uuid, WorkspaceSignals>, ws: Uuid) {
    map.entry(ws).or_insert_with(|| blank_signals(ws));
}

fn blank_signals(workspace_id: Uuid) -> WorkspaceSignals {
    WorkspaceSignals::empty(workspace_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::MigratorTrait;
    use sea_orm::{ConnectionTrait, Database};

    // Connects to the test DB and runs all migrations.
    // Gated on OXY_TEST_DATABASE_URL; skips cleanly when not set.
    async fn test_db() -> Option<DatabaseConnection> {
        let url = std::env::var("OXY_TEST_DATABASE_URL").ok()?;
        let db = Database::connect(&url).await.ok()?;
        // Run all four migrators in startup order.
        migration::Migrator::up(&db, None).await.ok()?;
        agentic_runtime::migration::RuntimeMigrator::up(&db, None)
            .await
            .ok()?;
        Some(db)
    }

    #[tokio::test]
    async fn gathers_failed_runs_per_workspace() {
        let Some(db) = test_db().await else {
            eprintln!("skipping: OXY_TEST_DATABASE_URL not set");
            return;
        };
        let ws = Uuid::new_v4();
        // Seed three failed runs for `ws` in-window.
        // `agentic_runs.id` is a String PK (not UUID); `question`, `attempt`,
        // `created_at`, and `updated_at` are NOT NULL.
        for _ in 0..3 {
            db.execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "INSERT INTO agentic_runs \
                    (id, workspace_id, question, task_status, attempt, created_at, updated_at) \
                 VALUES ($1, $2, '', 'failed', 0, now(), now())",
                [Uuid::new_v4().to_string().into(), ws.into()],
            ))
            .await
            .unwrap();
        }
        let signals = gather_signals(&db, &HealthThresholds::default(), None)
            .await
            .unwrap();
        let mine = signals.iter().find(|s| s.workspace_id == ws).unwrap();
        assert_eq!(mine.failed_runs, 3);
        assert_eq!(mine.total_runs, 3);
    }

    #[tokio::test]
    async fn scoped_gather_returns_only_target_workspace() {
        let Some(db) = test_db().await else {
            eprintln!("skipping: OXY_TEST_DATABASE_URL not set");
            return;
        };
        let target = Uuid::new_v4();
        let other = Uuid::new_v4();
        for ws in [target, other] {
            db.execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "INSERT INTO agentic_runs \
                    (id, workspace_id, question, task_status, attempt, created_at, updated_at) \
                 VALUES ($1, $2, '', 'failed', 0, now(), now())",
                [Uuid::new_v4().to_string().into(), ws.into()],
            ))
            .await
            .unwrap();
        }
        let signals = gather_signals(&db, &HealthThresholds::default(), Some(target))
            .await
            .unwrap();
        assert!(signals.iter().all(|s| s.workspace_id == target));
        assert!(signals.iter().any(|s| s.workspace_id == target));
    }
}
