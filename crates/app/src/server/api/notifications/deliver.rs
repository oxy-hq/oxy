//! The push-delivery seam, and the inbox write that does not depend on it.
//!
//! [`notify`] is the one function the rest of the product calls. It writes the
//! durable row FIRST and then attempts delivery, in that order and never the
//! other: a push that arrives for a notification the inbox does not have is a
//! tap that lands on nothing.

use std::sync::{Arc, OnceLock};

use chrono::Utc;
use entity::{device_tokens, notifications};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use tracing::{info, warn};
use uuid::Uuid;

/// One thing to tell one person.
#[derive(Debug, Clone)]
pub struct Notice {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    /// Relative, so it resolves on whichever host the reader is on.
    pub link: Option<String>,
    pub subject_kind: Option<String>,
    pub subject_id: Option<String>,
}

/// A push transport.
///
/// One method, taking already-resolved tokens: the seam deliberately knows
/// nothing about users, orgs or the inbox, so an adapter cannot accidentally
/// become a second place that decides who gets told what.
#[async_trait::async_trait]
pub trait Push: Send + Sync {
    /// Best-effort. An error here must never propagate to the caller of
    /// [`notify`] — the durable row is already written, and failing the
    /// business operation because a phone was unreachable is the wrong trade.
    ///
    /// `async` because every transport this seam exists for is network I/O:
    /// APNs is an HTTP/2 request, FCM an HTTPS call, Web Push one HTTPS POST
    /// per endpoint. A blocking signature would leave an adapter two bad
    /// options — park a Tokio worker for a whole fan-out, or `spawn` internally
    /// and lose the ability to report its own failures, which is exactly the
    /// distinction [`Push::name`] exists to preserve. Cheaper to settle here,
    /// while the logging default is the only implementation, than to change
    /// under an adapter later.
    async fn send(&self, tokens: &[device_tokens::Model], notice: &Notice);

    /// For the log line, so an operator can tell "no push configured" from
    /// "push configured and silent".
    fn name(&self) -> &'static str;
}

/// The default: records what WOULD have been sent.
///
/// Not a silent no-op. A silent one is indistinguishable from a working sender
/// whose messages nobody receives, which is the single most expensive way for
/// this subsystem to be wrong.
pub struct LoggingPush;

#[async_trait::async_trait]
impl Push for LoggingPush {
    async fn send(&self, tokens: &[device_tokens::Model], notice: &Notice) {
        info!(
            user = %notice.user_id,
            kind = %notice.kind,
            devices = tokens.len(),
            "push not configured — notification is in the inbox only"
        );
    }
    fn name(&self) -> &'static str {
        "logging"
    }
}

static PUSH: OnceLock<Arc<dyn Push>> = OnceLock::new();

/// Register the transport. First call wins; the app does this once at startup.
pub fn set_push(push: Arc<dyn Push>) {
    if PUSH.set(push).is_err() {
        warn!("push transport already registered; ignoring the second");
    }
}

fn push() -> Arc<dyn Push> {
    PUSH.get().cloned().unwrap_or_else(|| Arc::new(LoggingPush))
}

/// Tell somebody something.
///
/// Writes the inbox row, then attempts a push. Returns the row, so a caller
/// that wants to link to it can.
///
/// The write is the contract; the push is decoration. If the row fails, that IS
/// an error worth propagating — nobody was told, and a caller that thought they
/// had notified somebody should find out.
pub async fn notify(
    db: &DatabaseConnection,
    notice: Notice,
) -> Result<notifications::Model, sea_orm::DbErr> {
    let row = notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        org_id: Set(notice.org_id),
        user_id: Set(notice.user_id),
        kind: Set(notice.kind.clone()),
        title: Set(notice.title.clone()),
        body: Set(notice.body.clone()),
        link: Set(notice.link.clone()),
        subject_kind: Set(notice.subject_kind.clone()),
        subject_id: Set(notice.subject_id.clone()),
        created_at: Set(Utc::now().fixed_offset()),
        read_at: Set(None),
    }
    .insert(db)
    .await?;

    // Everything below is best-effort and must not fail the caller. Best-effort
    // is not the same as silent: an empty token list because the query failed
    // must not read in the log as "this user has no devices", which is the
    // distinction the rest of this module is careful about.
    let tokens = match device_tokens::Entity::find()
        .filter(device_tokens::Column::UserId.eq(notice.user_id))
        .all(db)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            warn!(
                user = %notice.user_id,
                error = %e,
                "could not load device tokens — notification is in the inbox, push skipped"
            );
            Vec::new()
        }
    };
    push().send(&tokens, &notice).await;

    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_transport_is_loud_about_being_absent() {
        // A silent no-op is indistinguishable from a working sender whose
        // messages nobody receives. The name is what lets an operator tell
        // "push not configured" from "push configured and broken".
        assert_eq!(LoggingPush.name(), "logging");
        assert_eq!(
            push().name(),
            "logging",
            "unregistered must fall back, not panic"
        );
    }

    #[test]
    fn a_notice_carries_a_relative_link() {
        // Absolute links break the moment the reader is on an org subdomain
        // rather than the admin host — and the sender cannot know which they
        // will be on, because a notification outlives the request that made it.
        let n = Notice {
            org_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            kind: "work_assigned".into(),
            title: "Re-test the sanitiser".into(),
            body: None,
            link: Some("/work/123".into()),
            subject_kind: Some("work_item".into()),
            subject_id: Some("123".into()),
        };
        assert!(n.link.as_deref().is_some_and(|l| l.starts_with('/')));
        assert!(!n.link.as_deref().is_some_and(|l| l.contains("://")));
    }
}
