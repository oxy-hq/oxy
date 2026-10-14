//! The push-delivery seam, and the platform's own entry point.
//!
//! [`notify`] is what the rest of the product calls to tell one person
//! something; `ctx.notify` reaches the same [`send`](super::send::send) from a
//! custom app. Either way the durable inbox row is written FIRST and delivery is
//! attempted after, in that order and never the other: a push that arrives for
//! a notification the inbox does not have is a tap that lands on nothing.

use std::sync::{Arc, OnceLock};

use entity::device_tokens;
use sea_orm::DatabaseConnection;
use tracing::info;
use uuid::Uuid;

use super::send::{Outgoing, SendError, Sent, send};

/// One thing the platform tells one person.
#[derive(Debug, Clone)]
pub struct Notice {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    /// Relative, so it resolves on whichever host the reader is on.
    pub link: Option<String>,
}

/// What a push carries: nothing but who it is for and what kind it is.
///
/// A push is a wake signal, not the message (`web_push` explains why it is
/// bodiless), so the transport is never handed the content.
#[derive(Debug, Clone)]
pub struct PushSignal {
    pub user_id: Uuid,
    pub kind: String,
}

/// What one fan-out to a person's devices achieved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PushReport {
    /// Devices the push service accepted the message for.
    pub delivered: usize,
    /// Devices whose push failed for a reason other than being gone.
    pub failed: usize,
}

/// A push transport.
///
/// One method, taking already-resolved tokens: the seam deliberately knows
/// nothing about users, orgs or the inbox, so an adapter cannot accidentally
/// become a second place that decides who gets told what.
#[async_trait::async_trait]
pub trait Push: Send + Sync {
    /// Best-effort. A failure is reported, never raised — the durable row is
    /// already written, and failing the business operation because a phone was
    /// unreachable is the wrong trade.
    ///
    /// `async` because every transport this seam exists for is network I/O:
    /// APNs is an HTTP/2 request, FCM an HTTPS call, Web Push one HTTPS POST
    /// per endpoint.
    async fn send(&self, tokens: &[device_tokens::Model], signal: &PushSignal) -> PushReport;

    /// For the log line, so an operator can tell "no push configured" from
    /// "push configured and silent".
    fn name(&self) -> &'static str;

    /// Whether this transport sends anything at all. The delivery ledger
    /// records an unconfigured one as `not_configured`, never as `sent`.
    fn configured(&self) -> bool {
        true
    }
}

/// The default: records what WOULD have been sent.
///
/// Not a silent no-op. A silent one is indistinguishable from a working sender
/// whose messages nobody receives, which is the single most expensive way for
/// this subsystem to be wrong.
pub struct LoggingPush;

#[async_trait::async_trait]
impl Push for LoggingPush {
    async fn send(&self, tokens: &[device_tokens::Model], signal: &PushSignal) -> PushReport {
        info!(
            user = %signal.user_id,
            kind = %signal.kind,
            devices = tokens.len(),
            "push not configured — notification is in the inbox only"
        );
        PushReport::default()
    }
    fn name(&self) -> &'static str {
        "logging"
    }
    fn configured(&self) -> bool {
        false
    }
}

static PUSH: OnceLock<Arc<dyn Push>> = OnceLock::new();

/// Register the transport. First call wins; the app does this once at startup.
pub fn set_push(push: Arc<dyn Push>) {
    if PUSH.set(push).is_err() {
        tracing::warn!("push transport already registered; ignoring the second");
    }
}

pub(super) fn push() -> Arc<dyn Push> {
    PUSH.get().cloned().unwrap_or_else(|| Arc::new(LoggingPush))
}

/// Tell somebody something, from the platform itself.
///
/// Writes the inbox content to the org's store and the signal to Oxy's, then
/// attempts a push inline — there is no app and no workspace here to queue a
/// delivery run under. Fails when the inbox cannot be written (the org has no
/// OLTP store, say): nobody was told, and a caller that thought they had
/// notified somebody should find out.
pub async fn notify(db: &DatabaseConnection, notice: Notice) -> Result<Sent, SendError> {
    send(
        db,
        Outgoing {
            org_id: notice.org_id,
            app: None,
            environment: "production".to_string(),
            workspace_id: None,
            kind: notice.kind,
            critical: false,
            title: notice.title,
            body: notice.body,
            link: notice.link,
            email: None,
            slack: false,
            idempotency_key: None,
            recipients: vec![notice.user_id],
        },
    )
    .await
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
        assert!(!LoggingPush.configured());
        assert_eq!(
            push().name(),
            "logging",
            "unregistered must fall back, not panic"
        );
    }
}
