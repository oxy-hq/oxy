//! Restart recovery for an airway run whose load had already finished.
//!
//! An airway worker ends a load by emitting `load_completed` and then
//! returning `Done`. `DurableTransport` stamps the queue row `completed` and
//! only then hands the outcome to the in-process coordinator, which is what
//! writes `task_status = done`. A restart between the two leaves a run whose
//! load finished but which reads non-terminal. Recovery then found no queued
//! task and no suspension (airway never suspends), `resume_from_state` refused
//! it, and `recover_active_runs` retired it as `failed`, emitting no event:
//! the only trace was a `recovery_resumed` marker after `load_completed`.
//!
//! This module decides from the run's own events whether the load finished,
//! and if it did, finishes the run as `done` instead of re-driving it. It is
//! deliberately conservative: any failure evidence for the finished load
//! leaves the run on the existing path, so a real failure still reads
//! `failed` after recovery.

use agentic_runtime::crud::events::EventRow;
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

/// What an airway run's persisted events say about its load.
#[derive(Debug, PartialEq)]
pub(crate) enum AirwayLoadVerdict {
    /// The load emitted `load_completed` with no failure evidence. Carries the
    /// task metadata the worker's `Done` outcome would have written.
    Completed { metadata: Value },
    /// A failure was recorded: a table or resource failed in the finished
    /// load, a fold failed, or the run errored. Recovery keeps its existing
    /// behaviour for these.
    Failed,
    /// No `load_completed` yet — the load was interrupted and must be re-driven.
    Unfinished,
}

/// Classify a run's events. Pure, so the rules are tested without a database.
///
/// Scoped to the **last** `load_completed`, and only while no later
/// `load_started` exists: a table failure from an earlier,
/// interrupted attempt (different `load_id`) does not taint a later load that
/// finished cleanly. Run-level failures (`pipeline_error`, `task_failed`) count
/// if they belong to that load or come after it.
pub(crate) fn classify_airway_events(events: &[EventRow]) -> AirwayLoadVerdict {
    let Some(done) = events
        .iter()
        .filter(|e| e.event_type == "load_completed")
        .max_by_key(|e| e.seq)
    else {
        return AirwayLoadVerdict::Unfinished;
    };
    // A newer attempt started after that load finished, so the `load_completed`
    // describes an earlier attempt. Same rule as the web app's
    // `airwayFailedTables.ts`: each `load_started` opens a new attempt.
    if events
        .iter()
        .any(|e| e.event_type == "load_started" && e.seq > done.seq)
    {
        return AirwayLoadVerdict::Unfinished;
    }
    let load_id = done.payload.get("load_id").and_then(Value::as_str);
    let same_load = |e: &EventRow| {
        load_id.is_some() && e.payload.get("load_id").and_then(Value::as_str) == load_id
    };

    let failed = events.iter().any(|e| match e.event_type.as_str() {
        "table_load_failed" | "resource_failed" => same_load(e),
        "pipeline_error" | "task_failed" => e.seq > done.seq || same_load(e),
        _ => false,
    });
    if failed || any_fold_failed(&done.payload) {
        return AirwayLoadVerdict::Failed;
    }
    AirwayLoadVerdict::Completed {
        metadata: json!({
            "load_id": load_id,
            "folds": done.payload.get("folds").cloned().unwrap_or(Value::Null),
        }),
    }
}

/// `load_completed` is emitted even when the end-of-load fold failed — the rows
/// are written but not queryable, and the run reports an error.
///
/// An allow-list, so it fails closed: the engine's `FoldOutcome` is external
/// (`status` = `committed` | `skipped_empty` | `failed` at the pinned rev), and
/// a tag added or renamed there must read as failure, not as a clean load.
fn any_fold_failed(payload: &Value) -> bool {
    const GOOD: [&str; 2] = ["committed", "skipped_empty"];
    payload
        .get("folds")
        .and_then(Value::as_array)
        .is_some_and(|folds| {
            folds.iter().any(|f| {
                !f.pointer("/outcome/status")
                    .and_then(Value::as_str)
                    .is_some_and(|s| GOOD.contains(&s))
            })
        })
}

/// If `run_id`'s load already finished, mark the run `done` and return `true`.
///
/// `false` means recovery should carry on as before. The caller must hold the
/// run's driver lease.
pub(crate) async fn finish_if_load_completed(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<bool, String> {
    let events = agentic_runtime::crud::get_all_events(db, run_id)
        .await
        .map_err(|e| format!("failed to read events for {run_id}: {e}"))?;
    let AirwayLoadVerdict::Completed { metadata } = classify_airway_events(&events) else {
        return Ok(false);
    };
    let finished =
        agentic_runtime::crud::complete_run_with_lost_outcome(db, run_id, Some(metadata))
            .await
            .map_err(|e| format!("failed to finish completed airway run {run_id}: {e}"))?;
    if !finished {
        // A task is still queued or claimed, or the run went terminal
        // meanwhile. Either way it is not ours to finish; recover as before.
        tracing::debug!(
            target: "recovery",
            run_id,
            "airway run has load_completed but live work or a terminal status; not finishing it"
        );
        return Ok(false);
    }
    // The worker releases the single-flight lease before it reports, so this is
    // normally a no-op; it is not when the process died inside that window.
    crate::airway_run::release_airway_lease(db, run_id).await;
    tracing::info!(
        target: "recovery",
        run_id,
        "airway load had already completed before the restart; marked the run done"
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: i64, event_type: &str, payload: Value) -> EventRow {
        EventRow {
            seq,
            event_type: event_type.to_string(),
            payload,
            attempt: 0,
        }
    }

    fn completed(seq: i64, load_id: &str, folds: Value) -> EventRow {
        ev(
            seq,
            "load_completed",
            json!({"load_id": load_id, "tables": ["t"], "folds": folds}),
        )
    }

    #[test]
    fn a_clean_load_is_completed_with_its_load_id() {
        let events = vec![
            ev(0, "table_loaded", json!({"load_id": "L1", "table": "t"})),
            completed(1, "L1", json!([])),
            ev(2, "recovery_resumed", json!({})),
        ];
        assert_eq!(
            classify_airway_events(&events),
            AirwayLoadVerdict::Completed {
                metadata: json!({"load_id": "L1", "folds": []})
            }
        );
    }

    #[test]
    fn no_load_completed_is_unfinished() {
        let events = vec![ev(0, "table_loaded", json!({"load_id": "L1"}))];
        assert_eq!(
            classify_airway_events(&events),
            AirwayLoadVerdict::Unfinished
        );
        assert_eq!(classify_airway_events(&[]), AirwayLoadVerdict::Unfinished);
    }

    #[test]
    fn a_table_failure_in_the_finished_load_is_failed() {
        let events = vec![
            ev(
                0,
                "table_load_failed",
                json!({"load_id": "L1", "table": "t"}),
            ),
            completed(1, "L1", json!([])),
        ];
        assert_eq!(classify_airway_events(&events), AirwayLoadVerdict::Failed);
    }

    #[test]
    fn a_resource_failure_in_the_finished_load_is_failed() {
        let events = vec![
            ev(0, "resource_failed", json!({"load_id": "L1", "table": "t"})),
            completed(1, "L1", json!([])),
        ];
        assert_eq!(classify_airway_events(&events), AirwayLoadVerdict::Failed);
    }

    #[test]
    fn a_run_error_after_the_load_is_failed() {
        for kind in ["pipeline_error", "task_failed"] {
            let events = vec![completed(0, "L1", json!([])), ev(1, kind, json!({}))];
            assert_eq!(
                classify_airway_events(&events),
                AirwayLoadVerdict::Failed,
                "{kind} after load_completed"
            );
        }
    }

    #[test]
    fn a_failed_fold_is_failed() {
        let folds = json!([{"table": "t", "outcome": {"status": "failed"}}]);
        let events = vec![completed(0, "L1", folds)];
        assert_eq!(classify_airway_events(&events), AirwayLoadVerdict::Failed);
    }

    #[test]
    fn an_unrecognised_or_missing_fold_status_is_failed() {
        for outcome in [json!({"status": "conflict"}), json!({})] {
            let folds = json!([{"table": "t", "outcome": outcome}]);
            let events = vec![completed(0, "L1", folds)];
            assert_eq!(classify_airway_events(&events), AirwayLoadVerdict::Failed);
        }
    }

    #[test]
    fn committed_and_skipped_empty_folds_are_clean() {
        let folds = json!([
            {"table": "a", "outcome": {"status": "committed", "watermark": "w", "attempts": 1}},
            {"table": "b", "outcome": {"status": "skipped_empty"}}
        ]);
        let events = vec![completed(0, "L1", folds)];
        assert!(matches!(
            classify_airway_events(&events),
            AirwayLoadVerdict::Completed { .. }
        ));
    }

    #[test]
    fn a_load_started_after_the_last_load_completed_is_unfinished() {
        let events = vec![
            completed(0, "L1", json!([])),
            ev(1, "load_started", json!({"load_id": "L2"})),
        ];
        assert_eq!(
            classify_airway_events(&events),
            AirwayLoadVerdict::Unfinished
        );
    }

    #[test]
    fn a_failure_from_an_earlier_interrupted_load_does_not_taint_a_clean_one() {
        let events = vec![
            ev(
                0,
                "table_load_failed",
                json!({"load_id": "L0", "table": "t"}),
            ),
            ev(1, "recovery_resumed", json!({})),
            completed(
                2,
                "L1",
                json!([{"table": "t", "outcome": {"status": "committed"}}]),
            ),
        ];
        assert!(matches!(
            classify_airway_events(&events),
            AirwayLoadVerdict::Completed { .. }
        ));
    }
}
