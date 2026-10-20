//! Audit for the **tenant project-secrets routes** when they write an app
//! secret.
//!
//! `PUT` and `DELETE /api/workspaces/{ws}/secrets/{id}` address a row by id,
//! so they reach a production app secret (`apps/<app_id>/<KEY>`) as well as a
//! plain project secret — Settings → Secrets rotates and deletes one there
//! (`EditSecretDialog`, `UnifiedSecretsTable`). Without this, those two were
//! the one way to change a production app secret that left no
//! `custom_app.secret.*` row, so an incident query for who changed a webhook
//! key found nothing. Rows are written exactly as the custom-app routes write
//! them (`ops::WriteAudit`): the same actions, target and environment.
//!
//! A non-production app secret never gets here: those rows are staff-only and
//! the tenant routes answer 404 for them (`secrets::staff_only`).

use entity::apps;
use oxy_app_core::audit::RequestActor;
use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::{DatabaseConnection, EntityTrait};

use super::ops::WriteAudit;
use super::scope;

/// What a tenant project-secrets route did to a row.
#[derive(Clone, Copy, Debug)]
pub enum ProjectSecretWrite {
    /// `PUT` with a new value. A description-only edit changes no value a run
    /// reads, and is not one.
    Rotated,
    Deleted,
}

impl ProjectSecretWrite {
    fn action(self) -> &'static str {
        match self {
            Self::Rotated => "custom_app.secret.set",
            Self::Deleted => "custom_app.secret.deleted",
        }
    }
}

/// Audit `write` of the stored secret `name` when it is a production app
/// secret; nothing for any other name. Best-effort like every secret audit: a
/// failed lookup or insert is logged and never fails the write, which has
/// already happened.
pub async fn audit_if_app_secret(
    db: &DatabaseConnection,
    name: &str,
    write: ProjectSecretWrite,
    actor: &RequestActor,
) {
    let Some((app_id, key)) = scope::production_app_secret(name) else {
        return;
    };
    let app = match apps::Entity::find_by_id(app_id).one(db).await {
        Ok(Some(app)) => app,
        Ok(None) => {
            // A row left behind by a deleted app: there is no org to file it
            // under, so the log line is the trail.
            tracing::warn!(%app_id, secret.key = %key, ?write, "app secret of no app written");
            return;
        }
        Err(e) => {
            tracing::warn!(%app_id, secret.key = %key, ?write, "app lookup for audit failed: {e}");
            return;
        }
    };
    let audit = WriteAudit {
        app: &app,
        environment: &AppEnvironment::Production,
        key,
        actor,
    };
    audit.record(db, write.action(), None).await;
}
