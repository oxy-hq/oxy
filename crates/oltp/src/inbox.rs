//! The notification inbox's content, in the org's own database.
//!
//! A notification is two things kept in two places. **Who was told, and
//! whether they read it** is a signal Oxy routes on: a `notifications` row in
//! Oxy's database, ids only. **What they were told** — title, body, link, the
//! email body — is the org's data, so it lives here, in `oxy_inbox.messages`
//! inside the org's tenant database (platform step 4), and the signal points
//! at it by id. One content row serves every recipient of a send.
//!
//! Written and read as the tenant's **owner**, never as an app's writer role:
//! `oxy_inbox` belongs to no writer, so a custom app reaches it only through
//! `ctx.notify`, never through `ctx.oltp`.
//!
//! An org with no tenant has nowhere to keep content, so a write fails with
//! [`InboxError::NotProvisioned`], whose message names the operator step.
//! Provisioning stays manual (`internal-docs/per-org-oltp-postgres.md`).
//!
//! Design: `internal-docs/custom-app-notifications.md`.

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::tenants::{self as oltp_tenants, TenantStatus};
use crate::migrator::{MigrateError, owner_dsn, tenant_for_org};
use crate::platform::INBOX_MESSAGES_TABLE;
use crate::provisioner::reconcile_platform_schema_as_owner;
use crate::sql::PgSqlExecutor;

/// What one notification says. Everything here is the org's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxMessage {
    /// Also the `notification_sends` id and every recipient signal's
    /// `message_id`.
    pub id: Uuid,
    pub app_id: Option<Uuid>,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    /// Relative: to the app's root for an app's notification, to the product
    /// for a platform one. The reader resolves it against wherever it is.
    pub link: Option<String>,
    pub email_subject: Option<String>,
    pub email_html: Option<String>,
    pub email_text: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum InboxError {
    /// The org has no OLTP tenant, so it has no inbox to write to.
    #[error(
        "this org has no OLTP store, which is where notification content is kept; an operator \
         provisions one with `oxyc oltp provision --org <org>`"
    )]
    NotProvisioned(Uuid),
    /// The tenant exists but is not active (a failed or deleting provision).
    #[error("this org's OLTP store is {0}, not active; an operator re-runs `oxyc oltp provision`")]
    Inactive(String),
    #[error("open the org's OLTP store: {0}")]
    Connect(String),
    #[error("bring the org's OLTP store up to date: {0}")]
    Reconcile(String),
    #[error("inbox query: {0}")]
    Query(String),
}

impl InboxError {
    /// The two states an operator fixes by provisioning, as opposed to a fault.
    pub fn is_not_provisioned(&self) -> bool {
        matches!(self, Self::NotProvisioned(_) | Self::Inactive(_))
    }
}

/// The org's tenant row, refusing one that cannot be written to.
async fn active_tenant(
    control: &DatabaseConnection,
    org_id: Uuid,
) -> Result<oltp_tenants::Model, InboxError> {
    let tenant = tenant_for_org(control, org_id).await.map_err(|e| match e {
        MigrateError::NotProvisioned(org) => InboxError::NotProvisioned(org),
        other => InboxError::Connect(other.to_string()),
    })?;
    if tenant.status != TenantStatus::Active {
        return Err(InboxError::Inactive(format!("{:?}", tenant.status)));
    }
    Ok(tenant)
}

async fn owner_client(tenant: &oltp_tenants::Model) -> Result<tokio_postgres::Client, InboxError> {
    let dsn = owner_dsn(tenant).map_err(|e| InboxError::Connect(e.to_string()))?;
    crate::connect::connect(&dsn, "oxy_inbox")
        .await
        .map_err(|e| InboxError::Connect(e.to_string()))
}

/// Whether the org can hold inbox content at all — asked before a send does
/// any other work, so an org with no store refuses up front rather than after
/// recipients were resolved.
pub async fn ensure_writable(control: &DatabaseConnection, org_id: Uuid) -> Result<(), InboxError> {
    active_tenant(control, org_id).await.map(|_| ())
}

/// Write one message. Brings the tenant's Oxy-owned objects current first, so
/// a tenant provisioned before the inbox existed gets its table on first use.
pub async fn write(
    control: &DatabaseConnection,
    org_id: Uuid,
    msg: &InboxMessage,
) -> Result<(), InboxError> {
    let tenant = active_tenant(control, org_id).await?;
    let tenant = reconcile_platform_schema_as_owner(control, &PgSqlExecutor, &tenant)
        .await
        .map_err(|e| InboxError::Reconcile(e.to_string()))?;
    let client = owner_client(&tenant).await?;
    client
        .execute(
            &format!(
                "INSERT INTO {INBOX_MESSAGES_TABLE} \
                 (id, app_id, kind, title, body, link, email_subject, email_html, email_text) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
                 ON CONFLICT (id) DO NOTHING"
            ),
            &[
                &msg.id,
                &msg.app_id,
                &msg.kind,
                &msg.title,
                &msg.body,
                &msg.link,
                &msg.email_subject,
                &msg.email_html,
                &msg.email_text,
            ],
        )
        .await
        .map_err(|e| InboxError::Query(e.to_string()))?;
    Ok(())
}

/// Read the messages `ids` names. A missing id is simply absent from the
/// result: the caller decides how a signal with no content reads.
pub async fn read(
    control: &DatabaseConnection,
    org_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<InboxMessage>, InboxError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let tenant = active_tenant(control, org_id).await?;
    let client = owner_client(&tenant).await?;
    let rows = client
        .query(
            &format!(
                "SELECT id, app_id, kind, title, body, link, email_subject, email_html, \
                 email_text FROM {INBOX_MESSAGES_TABLE} WHERE id = ANY($1)"
            ),
            &[&ids],
        )
        .await
        .map_err(|e| InboxError::Query(e.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|r| InboxMessage {
            id: r.get(0),
            app_id: r.get(1),
            kind: r.get(2),
            title: r.get(3),
            body: r.get(4),
            link: r.get(5),
            email_subject: r.get(6),
            email_html: r.get(7),
            email_text: r.get(8),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_store_says_how_to_get_one() {
        let msg = InboxError::NotProvisioned(Uuid::nil()).to_string();
        assert!(msg.contains("oxyc oltp provision"), "{msg}");
        assert!(InboxError::NotProvisioned(Uuid::nil()).is_not_provisioned());
        assert!(InboxError::Inactive("Failed".into()).is_not_provisioned());
        assert!(!InboxError::Query("boom".into()).is_not_provisioned());
    }
}
