//! `ctx.email.send`'s `idempotencyKey`, made to do what it always said.
//!
//! The key was accepted, length-checked and dropped, while the SDK told
//! authors to "adopt it now" — so a retried scheduled run that already mailed
//! the CFO mailed them again. Now the key is claimed in `app_email_sends`
//! before the message is handed to SES:
//!
//! - **a new claim** sends, then records SES's message id;
//! - **a claim that exists** sends nothing and answers with the first send's
//!   message id (empty while that first send is still in flight);
//! - **a send that fails** releases its claim, so the retry that follows can
//!   send. A refusal means the message did not go.
//!
//! Scoped per app and environment: staging's `report:2026-10-14` is not
//! production's. Only the key's SHA-256 is stored — the key is the app's text.

use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use serde_json::{Value, json};

use super::ProjectFunctionHost;
use oxy_app_core::notifications::send::key_hash;

/// What a claim found.
pub(super) enum EmailClaim {
    /// Ours: send, then [`ProjectFunctionHost::settle_email_claim`].
    Fresh(String),
    /// An earlier send with this key: answer with it, send nothing.
    Taken(Value),
}

impl ProjectFunctionHost {
    pub(super) async fn claim_email_key(&self, key: &str) -> Result<EmailClaim, String> {
        let hash = key_hash(key);
        let environment = self.policy.environment().name();
        let inserted = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "INSERT INTO app_email_sends (app_id, environment, idempotency_key_sha256) \
                 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING RETURNING app_id",
                [
                    self.app_id.into(),
                    environment.clone().into(),
                    hash.clone().into(),
                ],
            ))
            .await
            .map_err(|e| format!("EmailSendFailed: idempotency claim: {e}"))?;
        if inserted.is_some() {
            return Ok(EmailClaim::Fresh(hash));
        }
        let prior = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "SELECT message_id FROM app_email_sends \
                 WHERE app_id = $1 AND environment = $2 AND idempotency_key_sha256 = $3",
                [self.app_id.into(), environment.into(), hash.into()],
            ))
            .await
            .map_err(|e| format!("EmailSendFailed: idempotency claim: {e}"))?
            .and_then(|row| {
                row.try_get::<Option<String>>("", "message_id")
                    .ok()
                    .flatten()
            });
        Ok(EmailClaim::Taken(json!({
            "messageId": prior.unwrap_or_default(),
            "deduplicated": true,
        })))
    }

    /// Record how the claimed send went: SES's id on success, the claim
    /// released on failure. Best-effort — the send already happened (or did
    /// not), and failing it now over bookkeeping would invite the retry this
    /// exists to stop.
    pub(super) async fn settle_email_claim(&self, hash: &str, result: &Result<Value, String>) {
        let environment = self.policy.environment().name();
        let statement = match result {
            Ok(sent) => Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "UPDATE app_email_sends SET message_id = $4 \
                 WHERE app_id = $1 AND environment = $2 AND idempotency_key_sha256 = $3",
                [
                    self.app_id.into(),
                    environment.into(),
                    hash.into(),
                    sent.get("messageId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                        .into(),
                ],
            ),
            Err(_) => Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                "DELETE FROM app_email_sends \
                 WHERE app_id = $1 AND environment = $2 AND idempotency_key_sha256 = $3",
                [self.app_id.into(), environment.into(), hash.into()],
            ),
        };
        if let Err(e) = self.db.execute_raw(statement).await {
            tracing::warn!(app = %self.app_id, error = %e, "could not settle an email idempotency claim");
        }
    }
}
