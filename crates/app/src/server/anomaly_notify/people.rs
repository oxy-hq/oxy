//! The `notify.people` destination: an `oxy.anomaly` inbox notice per event to
//! the org members a `.monitor.yml` names.
//!
//! It claims under its own ledger channel (`inbox`), so it and the Slack post
//! succeed or fail apart, and each event is announced to the people once. The
//! notice itself is also at most once per (person, event), so a claim lost to
//! a crash and taken again tells nobody twice.
//!
//! `people` is resolved when the task runs, not when the file was compiled: a
//! new admin hears about the next event, and a removed member does not.

use std::collections::BTreeSet;

use async_trait::async_trait;
use oxy_metric_monitoring::NotifyConfig;
use oxy_metric_monitoring::notify::ledger::CHANNEL_INBOX;
use oxy_metric_monitoring::notify::{Bucket, Destination, Heading};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::server::api::notifications::platform::{PlatformNotice, notify, people};

/// The org members one workspace's `notify.people` names.
pub struct PeopleInbox {
    db: DatabaseConnection,
    org_id: Uuid,
    workspace_id: Uuid,
    people: Vec<Uuid>,
}

impl PeopleInbox {
    /// Resolve `notify.people` against the org now. Entries that name nobody
    /// are logged; a list that names nobody at all is still a destination,
    /// and refuses, so the run says why.
    pub async fn resolve(
        db: &DatabaseConnection,
        org_id: Uuid,
        workspace_id: Uuid,
        notify: &NotifyConfig,
    ) -> Result<Self, String> {
        let targets = notify.people()?;
        let (people, nobody) = people::named_in_monitor(db, org_id, &targets)
            .await
            .map_err(|e| format!("resolve `notify.people`: {e}"))?;
        for entry in nobody {
            tracing::info!(
                target: "metric_anomalies",
                %workspace_id,
                entry = %entry,
                "notify.people entry names no org member"
            );
        }
        Ok(Self {
            db: db.clone(),
            org_id,
            workspace_id,
            people,
        })
    }

    /// How many people it reaches.
    pub fn reach(&self) -> usize {
        self.people.len()
    }
}

#[async_trait]
impl Destination for PeopleInbox {
    fn id(&self) -> &str {
        "people"
    }

    fn channel(&self) -> &'static str {
        CHANNEL_INBOX
    }

    async fn deliver(
        &self,
        _heading: Heading<'_>,
        buckets: &[Bucket],
    ) -> Result<Option<usize>, String> {
        let events: BTreeSet<Uuid> = buckets.iter().map(|b| b.event_id).collect();
        if events.is_empty() {
            return Ok(None);
        }
        if self.people.is_empty() {
            return Err(
                "`notify.people` names no member of this org: check the emails, roles and team \
                 names in .monitor.yml"
                    .to_string(),
            );
        }
        for event_id in &events {
            let notice = PlatformNotice::Anomaly {
                org_id: self.org_id,
                workspace_id: self.workspace_id,
                event_id: *event_id,
            };
            notify(&self.db, &notice, &self.people)
                .await
                .map_err(|e| format!("write the inbox notices: {e}"))?;
        }
        Ok(Some(events.len()))
    }
}
