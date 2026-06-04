//! Unit tests for the internal-jobs admin module.
//!
//! These tests cover the pieces that don't require a live Postgres
//! connection — task-type extraction, status-counter accumulation,
//! scheduled-jobs registry shape. The DB-dependent endpoints
//! (`reenqueue_dead`, `delete_dead`, `fetch_queue_stats`) are covered
//! end-to-end by the integration test suite under
//! `tests/admin_internal_jobs.rs` when a Postgres connection is
//! available; the unit suite here keeps fast feedback on the parts that
//! can run without one.

use super::{QueueStatusCounts, accumulate, extract_task_type, scheduled_jobs};
use serde_json::json;

#[test]
fn accumulate_routes_each_status_into_the_right_field() {
    let mut counts = QueueStatusCounts::default();
    accumulate(&mut counts, "queued", 2);
    accumulate(&mut counts, "claimed", 3);
    accumulate(&mut counts, "completed", 5);
    accumulate(&mut counts, "failed", 7);
    accumulate(&mut counts, "cancelled", 11);
    accumulate(&mut counts, "dead", 13);
    assert_eq!(counts.queued, 2);
    assert_eq!(counts.claimed, 3);
    assert_eq!(counts.completed, 5);
    assert_eq!(counts.failed, 7);
    assert_eq!(counts.cancelled, 11);
    assert_eq!(counts.dead, 13);
}

#[test]
fn accumulate_ignores_unknown_statuses() {
    let mut counts = QueueStatusCounts::default();
    accumulate(&mut counts, "completed", 4);
    accumulate(&mut counts, "novel_status_we_do_not_track", 99);
    assert_eq!(counts.completed, 4);
    // Everything else stayed at default 0.
    assert_eq!(
        counts,
        QueueStatusCounts {
            queued: 0,
            claimed: 0,
            completed: 4,
            failed: 0,
            cancelled: 0,
            dead: 0,
        }
    );
}

#[test]
fn extract_task_type_returns_first_object_key() {
    let spec = json!({"AnalyticsTurn": {"foo": 1}});
    assert_eq!(extract_task_type(&spec), Some("AnalyticsTurn".to_string()));
}

#[test]
fn extract_task_type_returns_none_for_non_object() {
    assert_eq!(extract_task_type(&json!("plain string")), None);
    assert_eq!(extract_task_type(&json!(42)), None);
    assert_eq!(extract_task_type(&json!(null)), None);
}

#[test]
fn extract_task_type_returns_none_for_empty_object() {
    assert_eq!(extract_task_type(&json!({})), None);
}

#[test]
fn scheduled_jobs_registry_lists_the_three_known_loops() {
    let jobs = scheduled_jobs();
    assert_eq!(jobs.len(), 3);
    let names: Vec<&str> = jobs.iter().map(|j| j.name).collect();
    assert!(names.contains(&"reaper"));
    assert!(names.contains(&"matcher_health_probe"));
    assert!(names.contains(&"worker_recovery_loop"));
}

#[test]
fn reaper_is_the_only_manually_triggerable_periodic_job() {
    let jobs = scheduled_jobs();
    let reaper = jobs.iter().find(|j| j.name == "reaper").unwrap();
    assert_eq!(
        reaper.trigger_path,
        Some("/api/admin/internal-jobs/run-reaper")
    );
    // The other two are observation-only.
    for j in jobs.iter().filter(|j| j.name != "reaper") {
        assert!(
            j.trigger_path.is_none(),
            "{} should not have a trigger_path",
            j.name
        );
    }
}
