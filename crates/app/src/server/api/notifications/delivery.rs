//! Getting a sent notification to each person: push, email, Slack.
//!
//! The inbox row already exists — that is the notification. Everything here is
//! an attempt to draw attention to it, recorded per (recipient, channel) in
//! `notification_deliveries`:
//!
//! - **Claimed before it is sent.** An insert that finds a row owns nothing, so
//!   a re-run of the task sends nothing twice. The one exception is a row
//!   recorded `failed`: a retry re-claims it, because a refusal means it did
//!   not go.
//! - **Skips are recorded, with a code.** "Muted", "no devices", "no address",
//!   "not linked to Slack" and "push not configured" are answers someone will
//!   ask for; an absent row could mean any of them, or that nothing ran.
//! - **Mutes apply per (app, kind, channel)**, or per (kind, channel) for a
//!   platform notice, except to a critical kind.

use std::collections::HashSet;

use entity::{notification_mutes, notification_sends, notifications};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, Statement,
};
use uuid::Uuid;

use super::channels::{ChannelOutcome, Channels};

/// The out-of-band channels, as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Push,
    Email,
    Slack,
}

impl Channel {
    pub const ALL: [Channel; 3] = [Channel::Push, Channel::Email, Channel::Slack];

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Push => "push",
            Channel::Email => "email",
            Channel::Slack => "slack",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// What one delivery run did, for the run's answer and its logs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeliveryReport {
    pub sent: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Set when the run could not get as far as delivering anything.
    pub error: Option<String>,
}

impl DeliveryReport {
    pub fn summary(&self) -> String {
        match &self.error {
            Some(e) => format!("delivery could not run: {e}"),
            None => format!(
                "sent {}, skipped {}, failed {}",
                self.sent, self.skipped, self.failed
            ),
        }
    }

    fn add(&mut self, outcome: &ChannelOutcome) {
        match outcome {
            ChannelOutcome::Sent => self.sent += 1,
            ChannelOutcome::Skipped(_) => self.skipped += 1,
            ChannelOutcome::Failed(_) => self.failed += 1,
        }
    }
}

/// Deliver send `send_id` to everyone it was sent to. Never fails: what went
/// wrong is in the report, and a report with `failed > 0` fails the task so
/// the queue retries.
pub async fn deliver_send(db: &DatabaseConnection, send_id: Uuid) -> DeliveryReport {
    match deliver(db, send_id).await {
        Ok(report) => report,
        Err(e) => {
            tracing::warn!(%send_id, error = %e, "notification delivery could not run");
            DeliveryReport {
                failed: 1,
                error: Some(e),
                ..DeliveryReport::default()
            }
        }
    }
}

async fn deliver(db: &DatabaseConnection, send_id: Uuid) -> Result<DeliveryReport, String> {
    let send = notification_sends::Entity::find_by_id(send_id)
        .one(db)
        .await
        .map_err(|e| format!("load the send: {e}"))?
        .ok_or("the send no longer exists (its app or org was deleted)")?;
    let signals = notifications::Entity::find()
        .filter(notifications::Column::MessageId.eq(send_id))
        .all(db)
        .await
        .map_err(|e| format!("load the recipients: {e}"))?;
    let mut report = DeliveryReport::default();
    if signals.is_empty() {
        return Ok(report);
    }
    let users: Vec<Uuid> = signals.iter().map(|s| s.user_id).collect();
    let muted = mutes(db, &send, &users)
        .await
        .map_err(|e| format!("load mutes: {e}"))?;
    let channels = Channels::load(db, &send, &signals).await?;
    for signal in &signals {
        for channel in requested(&send) {
            let outcome = deliver_one(db, &channels, signal, channel, &muted)
                .await
                .map_err(|e| format!("record a delivery: {e}"))?;
            report.add(&outcome);
        }
    }
    Ok(report)
}

/// The channels a send asked for. Push always; email and Slack when asked.
fn requested(send: &notification_sends::Model) -> Vec<Channel> {
    let mut out = vec![Channel::Push];
    if send.email {
        out.push(Channel::Email);
    }
    if send.slack {
        out.push(Channel::Slack);
    }
    out
}

async fn deliver_one(
    db: &DatabaseConnection,
    channels: &Channels,
    signal: &notifications::Model,
    channel: Channel,
    muted: &HashSet<(Uuid, Channel)>,
) -> Result<ChannelOutcome, DbErr> {
    if !claim(db, signal.id, channel).await? {
        // Already sent, skipped, or claimed by a run that may have sent it.
        return Ok(ChannelOutcome::Skipped("already_attempted"));
    }
    let outcome = if muted.contains(&(signal.user_id, channel)) {
        ChannelOutcome::Skipped("muted")
    } else {
        channels.send(channel, signal.user_id).await
    };
    finish(db, signal.id, channel, &outcome).await?;
    Ok(outcome)
}

/// Who muted which channel for this send's app and kind — or, for a
/// platform notice, for its kind alone. Empty for a critical kind.
async fn mutes(
    db: &DatabaseConnection,
    send: &notification_sends::Model,
    users: &[Uuid],
) -> Result<HashSet<(Uuid, Channel)>, DbErr> {
    if send.critical {
        return Ok(HashSet::new());
    }
    let Some(app_id) = send.app_id else {
        return super::platform::preferences::muted(db, &send.kind, users).await;
    };
    Ok(notification_mutes::Entity::find()
        .filter(notification_mutes::Column::AppId.eq(app_id))
        .filter(notification_mutes::Column::Kind.eq(&send.kind))
        .filter(notification_mutes::Column::UserId.is_in(users.iter().copied()))
        .all(db)
        .await?
        .into_iter()
        .filter_map(|m| Channel::parse(&m.channel).map(|c| (m.user_id, c)))
        .collect())
}

/// Take (signal, channel) for this attempt. `true` when it is ours to send: a
/// new row, or a `failed` one being retried.
async fn claim(
    db: &DatabaseConnection,
    notification_id: Uuid,
    channel: Channel,
) -> Result<bool, DbErr> {
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "INSERT INTO notification_deliveries (notification_id, channel, status) \
             VALUES ($1, $2, 'claimed') \
             ON CONFLICT (notification_id, channel) DO UPDATE \
               SET status = 'claimed', reason = NULL, attempted_at = now() \
               WHERE notification_deliveries.status = 'failed' \
             RETURNING notification_id",
            [notification_id.into(), channel.as_str().into()],
        ))
        .await?;
    Ok(row.is_some())
}

async fn finish(
    db: &DatabaseConnection,
    notification_id: Uuid,
    channel: Channel,
    outcome: &ChannelOutcome,
) -> Result<(), DbErr> {
    let (status, reason) = match outcome {
        ChannelOutcome::Sent => ("sent", None),
        ChannelOutcome::Skipped(r) => ("skipped", Some(*r)),
        ChannelOutcome::Failed(r) => ("failed", Some(*r)),
    };
    db.execute_raw(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE notification_deliveries SET status = $3, reason = $4, attempted_at = now() \
         WHERE notification_id = $1 AND channel = $2",
        [
            notification_id.into(),
            channel.as_str().into(),
            status.into(),
            reason.map(str::to_string).into(),
        ],
    ))
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_round_trip_through_their_stored_names() {
        for c in Channel::ALL {
            assert_eq!(Channel::parse(c.as_str()), Some(c));
        }
        assert_eq!(Channel::parse("sms"), None, "no SMS channel exists yet");
    }

    #[test]
    fn a_report_with_a_failure_says_so() {
        let mut r = DeliveryReport::default();
        r.add(&ChannelOutcome::Sent);
        r.add(&ChannelOutcome::Skipped("muted"));
        r.add(&ChannelOutcome::Failed("provider_error"));
        assert_eq!(r.summary(), "sent 1, skipped 1, failed 1");
    }
}
