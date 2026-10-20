//! The one gate between a child's result and its parent.
//!
//! A result may touch a parent only while the parent is `WaitingOnChildren`
//! **and** names the child in its current fan-out. Anything else is late or
//! stale — the parent timed out or failed, was resumed by a sibling's
//! fail-fast, re-suspended on a new fan-out, or is waiting on a human — and
//! acting on it revives a dead parent, hands a human question a child's
//! output, or counts a stranger into a fan-out.

use std::collections::HashMap;

use super::{ChildResult, TaskStatus};

/// Why a child's result was not admitted.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum StaleChild {
    /// The parent is not `WaitingOnChildren` (running, human-suspended,
    /// done, or failed/timed out/cancelled).
    ParentNotWaiting,
    /// The parent waits on children, but not on this one.
    NotInFanout,
    /// This child's result is already on record.
    AlreadyRecorded,
}

impl StaleChild {
    pub(super) fn reason(&self) -> &'static str {
        match self {
            Self::ParentNotWaiting => "parent is not waiting on children",
            Self::NotInFanout => "child is not in the parent's current fan-out",
            Self::AlreadyRecorded => "child's result is already recorded",
        }
    }
}

/// `Ok(())` iff `child_id`'s result may be recorded against a parent in `status`.
pub(super) fn admit_child_result(status: &TaskStatus, child_id: &str) -> Result<(), StaleChild> {
    let TaskStatus::WaitingOnChildren {
        child_task_ids,
        completed,
        ..
    } = status
    else {
        return Err(StaleChild::ParentNotWaiting);
    };
    admit_into_fanout(child_task_ids, completed, child_id)
}

fn admit_into_fanout(
    child_task_ids: &[String],
    completed: &HashMap<String, ChildResult>,
    child_id: &str,
) -> Result<(), StaleChild> {
    if !child_task_ids.iter().any(|id| id == child_id) {
        return Err(StaleChild::NotInFanout);
    }
    if completed.contains_key(child_id) {
        return Err(StaleChild::AlreadyRecorded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use agentic_core::delegation::FanoutFailurePolicy;

    use super::*;

    fn waiting(children: &[&str], done: &[&str]) -> TaskStatus {
        TaskStatus::WaitingOnChildren {
            child_task_ids: children.iter().map(ToString::to_string).collect(),
            completed: done
                .iter()
                .map(|id| (id.to_string(), ChildResult::Done("a".into())))
                .collect(),
            failure_policy: FanoutFailurePolicy::FailFast,
        }
    }

    #[test]
    fn a_member_of_the_current_fan_out_is_admitted() {
        assert_eq!(
            admit_child_result(&waiting(&["p.1", "p.2"], &[]), "p.2"),
            Ok(())
        );
        assert_eq!(
            admit_child_result(&waiting(&["p.1", "p.2"], &["p.1"]), "p.2"),
            Ok(())
        );
    }

    #[test]
    fn a_child_outside_the_fan_out_is_stale() {
        assert_eq!(
            admit_child_result(&waiting(&["p.3"], &[]), "p.2"),
            Err(StaleChild::NotInFanout)
        );
    }

    #[test]
    fn a_second_result_for_one_child_is_dropped() {
        assert_eq!(
            admit_child_result(&waiting(&["p.1", "p.2"], &["p.1"]), "p.1"),
            Err(StaleChild::AlreadyRecorded)
        );
    }

    #[test]
    fn a_parent_in_any_other_state_admits_nothing() {
        for status in [
            TaskStatus::Running,
            TaskStatus::SuspendedHuman,
            TaskStatus::Done,
            TaskStatus::Failed,
        ] {
            assert_eq!(
                admit_child_result(&status, "p.1"),
                Err(StaleChild::ParentNotWaiting),
                "{status:?}"
            );
        }
    }
}
