//! `oxy.run_failed`: an automation or Airway run failed and nothing will retry
//! it.
//!
//! `agentic-runtime` calls [`RunFailureNotices`] after it writes a root run
//! terminal-failed (the coordinator past its retry check, a root timeout, a
//! recovery retirement or dead letter). This reads the row and decides:
//!
//! - only automations and Airway pipelines, and only the root of a run tree;
//! - not a cancel. A cancel can land as `failed` ("cancelled by user") on
//!   several defensive paths, but every one of them sets
//!   `cancel_requested_at` first;
//! - not a workspace preview's run, which nobody started on purpose.
//!
//! Told: whoever started it (`metadata.started_by`, stamped by the HTTP
//! routes) while they are a member of the org; otherwise — a scheduled run,
//! or a starter who left — the workspace's admins. Schedules record no
//! creator, so there is no one closer to tell.

use std::sync::Arc;

use agentic_runtime::entity::run;
use agentic_runtime::lifecycle::failure_hook::{RunFailureObserver, set_run_failure_observer};
use async_trait::async_trait;
use entity::workspaces;
use sea_orm::{DatabaseConnection, DbErr, EntityTrait};
use uuid::Uuid;

use super::kinds::PlatformNotice;
use super::people;
use super::send::{PlatformSent, notify};
use super::subjects::run_record;

/// Install the observer. Called at `oxy serve` and `oxy worker` boot: either
/// can drive a run to its end.
pub fn install() {
    set_run_failure_observer(Arc::new(RunFailureNotices));
}

/// The observer. Holds no connection: it takes the process's pool when a run
/// fails, so installing it at boot needs no database yet.
pub struct RunFailureNotices;

#[async_trait]
impl RunFailureObserver for RunFailureNotices {
    async fn root_run_failed(&self, run_id: &str) {
        let db = match oxy::database::client::establish_connection().await {
            Ok(db) => db,
            Err(e) => {
                tracing::warn!(%run_id, error = %e, "run failure notice not sent: no database");
                return;
            }
        };
        match notify_run_failed(&db, run_id).await {
            Ok(Some(sent)) => {
                tracing::info!(%run_id, recipients = sent.recipients, "run failure notice sent")
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(%run_id, error = %e, "run failure notice not sent"),
        }
    }
}

/// The notice for run `run_id`, if it is one that warrants one.
pub async fn notify_run_failed(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<PlatformSent>, DbErr> {
    let Some(row) = run::Entity::find_by_id(run_id.to_string()).one(db).await? else {
        return Ok(None);
    };
    if !warrants_notice(&row) {
        return Ok(None);
    }
    let Some(record) = run_record(&row) else {
        return Ok(None);
    };
    let Some(org_id) = workspaces::Entity::find_by_id(row.workspace_id)
        .one(db)
        .await?
        .and_then(|w| w.org_id)
    else {
        return Ok(None);
    };
    let notice = match record.airway {
        true => PlatformNotice::AirwayRunFailed {
            org_id,
            workspace_id: row.workspace_id,
            run_id: row.id.clone(),
        },
        false => PlatformNotice::AutomationRunFailed {
            org_id,
            workspace_id: row.workspace_id,
            run_id: row.id.clone(),
        },
    };
    let recipients = recipients(db, org_id, &row).await?;
    notify(db, &notice, &recipients).await.map(Some)
}

/// Whether a run row is a terminal failure someone should hear about. Pure,
/// so each exclusion is pinned by a unit test.
pub fn warrants_notice(row: &run::Model) -> bool {
    let failed = matches!(row.task_status.as_deref(), Some("failed" | "timed_out"));
    let cancelled = row.cancel_requested_at.is_some()
        || row.error_message.as_deref() == Some("cancelled by user");
    let preview = row
        .metadata
        .as_ref()
        .and_then(|m| m.get(agentic_runtime::crud::PREVIEW_STAMP_KEY))
        .is_some_and(|v| !v.is_null());
    failed && !cancelled && !preview && row.parent_run_id.is_none() && run_record(row).is_some()
}

/// The starter while still a member, else the workspace's admins.
async fn recipients(
    db: &DatabaseConnection,
    org_id: Uuid,
    row: &run::Model,
) -> Result<Vec<Uuid>, DbErr> {
    let starter = row
        .metadata
        .as_ref()
        .and_then(|m| m.get(agentic_pipeline::STARTED_BY_KEY))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());
    if let Some(user) = starter
        && people::is_org_member(db, org_id, user).await?
    {
        return Ok(vec![user]);
    }
    people::workspace_admins(db, org_id, row.workspace_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn failed_automation() -> run::Model {
        let now = Utc::now().fixed_offset();
        run::Model {
            id: "r-1".into(),
            question: String::new(),
            answer: None,
            error_message: Some("step 2: query failed".into()),
            thread_id: None,
            source_type: Some(agentic_pipeline::WORKFLOW_SOURCE_TYPE.into()),
            metadata: Some(serde_json::json!({ "workflow_ref": "a.automation.yml" })),
            parent_run_id: None,
            schedule_id: None,
            task_status: Some("failed".into()),
            task_metadata: None,
            attempt: 0,
            recovery_requested_at: None,
            driver_id: None,
            driver_heartbeat_at: None,
            cancel_requested_at: None,
            workspace_id: Uuid::nil(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn a_failed_or_timed_out_root_automation_warrants_a_notice() {
        assert!(warrants_notice(&failed_automation()));
        let mut timed_out = failed_automation();
        timed_out.task_status = Some("timed_out".into());
        assert!(warrants_notice(&timed_out));
    }

    #[test]
    fn cancels_children_previews_and_other_runs_do_not() {
        let mut r = failed_automation();
        r.cancel_requested_at = Some(Utc::now().fixed_offset());
        assert!(!warrants_notice(&r), "a cancel recorded as failed");
        let mut r = failed_automation();
        r.error_message = Some("cancelled by user".into());
        assert!(!warrants_notice(&r), "a cancel without the flag");
        let mut r = failed_automation();
        r.parent_run_id = Some("r-0".into());
        assert!(!warrants_notice(&r), "a child run");
        let mut r = failed_automation();
        r.metadata = Some(serde_json::json!({
            "workflow_ref": "a.automation.yml",
            agentic_runtime::crud::PREVIEW_STAMP_KEY: { "revision_id": "x" },
        }));
        assert!(!warrants_notice(&r), "a preview run");
        let mut r = failed_automation();
        r.source_type = Some("analytics".into());
        assert!(!warrants_notice(&r), "an analytics run");
        let mut r = failed_automation();
        r.task_status = Some("done".into());
        assert!(!warrants_notice(&r), "a finished run");
    }
}
