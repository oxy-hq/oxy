//! Pure status mapping for [`super::recovery`]: persisted statuses to in-memory ones.

use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::crud;

use super::{ChildResult, TaskStatus};

/// The outcomes on record for the children `parent_id` is waiting on **now**.
///
/// `agentic_task_outcomes` keeps every outcome a parent has ever been sent,
/// and a parent that delegates more than once — an automation does, once per
/// step — still has the rows from its earlier delegations. Only the children
/// of the delegation it is parked at count. Counting all of them made a parent
/// whose one child was still running read as "every child in" (one old outcome
/// against one expected child), and recovery resumed it with an answer
/// aggregated from children that had not reported.
pub(super) async fn completed_children(
    db: &DatabaseConnection,
    parent_id: &str,
    child_task_ids: &[String],
) -> Result<HashMap<String, ChildResult>, sea_orm::DbErr> {
    let outcomes = crud::get_outcomes_for_parent(db, parent_id).await?;
    Ok(outcomes
        .into_iter()
        .filter(|o| child_task_ids.contains(&o.child_id))
        .filter_map(|o| {
            let result = child_result_from_outcome(&o.status, o.answer.unwrap_or_default());
            if result.is_none() {
                tracing::warn!(
                    target: "coordinator",
                    parent_id,
                    child_id = %o.child_id,
                    status = %o.status,
                    "outcome with a non-terminal status ignored; child stays live"
                );
            }
            result.map(|r| (o.child_id, r))
        })
        .collect())
}

/// The fan-out result an `agentic_task_outcomes` row stands for, or `None` for
/// a status that is not terminal. `cancelled` and `timed_out` are failures to
/// the parent, as they are on the live path.
fn child_result_from_outcome(status: &str, answer: String) -> Option<ChildResult> {
    match status {
        "done" => Some(ChildResult::Done(answer)),
        "failed" | "cancelled" | "timed_out" => Some(ChildResult::Failed(answer)),
        _ => None,
    }
}

/// How the parent's resume reads a fan-out: `"failed"` if any child failed,
/// else `"done"` — what the live `AllDone` path reports.
pub(super) fn fanout_status(completed: &HashMap<String, ChildResult>) -> &'static str {
    if completed
        .values()
        .any(|r| matches!(r, ChildResult::Failed(_)))
    {
        "failed"
    } else {
        "done"
    }
}

/// The in-memory status for a persisted `task_status` that is terminal, or
/// `None` for one that is live or needs the tree to interpret.
pub(super) fn terminal_status(task_status: Option<&str>) -> Option<TaskStatus> {
    match task_status {
        Some("done") => Some(TaskStatus::Done),
        // Cancelled and timed-out tasks are `Failed` in memory, as live.
        Some("failed" | "cancelled" | "timed_out") => Some(TaskStatus::Failed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_terminal_outcome_status_maps_and_a_live_one_does_not() {
        assert!(matches!(
            child_result_from_outcome("done", "a".into()),
            Some(ChildResult::Done(a)) if a == "a"
        ));
        for failed in ["failed", "cancelled", "timed_out"] {
            assert!(
                matches!(
                    child_result_from_outcome(failed, "why".into()),
                    Some(ChildResult::Failed(m)) if m == "why"
                ),
                "{failed}"
            );
        }
        for live in ["running", "delegating", "awaiting_input", ""] {
            assert!(
                child_result_from_outcome(live, "x".into()).is_none(),
                "{live}"
            );
        }
    }

    #[test]
    fn every_terminal_task_status_is_terminal_in_memory() {
        assert!(matches!(
            terminal_status(Some("done")),
            Some(TaskStatus::Done)
        ));
        for failed in ["failed", "cancelled", "timed_out"] {
            assert!(
                matches!(terminal_status(Some(failed)), Some(TaskStatus::Failed)),
                "{failed}"
            );
        }
        for live in [
            Some("running"),
            Some("delegating"),
            Some("awaiting_input"),
            None,
        ] {
            assert!(terminal_status(live).is_none(), "{live:?}");
        }
    }

    #[test]
    fn a_fan_out_with_any_failed_child_resumes_as_failed() {
        let mut completed = HashMap::new();
        completed.insert("a".to_string(), ChildResult::Done("ok".into()));
        assert_eq!(fanout_status(&completed), "done");
        completed.insert("b".to_string(), ChildResult::Failed("no".into()));
        assert_eq!(fanout_status(&completed), "failed");
    }
}
