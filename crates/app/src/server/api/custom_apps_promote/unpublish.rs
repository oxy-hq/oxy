//! Taking production's build away: the console's *Unpublish*, single and
//! batch, and the partner console's. The other half of the production
//! pointer's writers, held to the same rule as [`super::production`]: the
//! pointer, `published_at` and the `unpublish` event move together, in one
//! transaction holding the app's row `FOR UPDATE`.
//!
//! No admission applies — nothing is being made live — and no migration is
//! touched: the bundle bytes and every schema stay as they were.

use chrono::Utc;
use entity::apps;
use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection, TransactionTrait};
use uuid::Uuid;

use super::error::PromoteError;
use super::production::lock_app;
use crate::server::api::custom_apps_cache;
use crate::server::api::custom_apps_environments::{self, EnvAction};

/// Null `published_at` and production's pointer, and record the `unpublish`
/// event for `actor`. Drops the serve path's resolution cache once committed;
/// the global access cache is the caller's (a batch drops it once).
pub async fn unpublish(
    db: &DatabaseConnection,
    app_id: Uuid,
    actor: Option<Uuid>,
) -> Result<apps::Model, PromoteError> {
    let txn = db
        .begin()
        .await
        .map_err(|e| PromoteError::internal("begin the unpublish", e))?;
    let row = lock_app(&txn, app_id).await?;
    let mut active: apps::ActiveModel = row.into();
    active.published_at = ActiveValue::Set(None);
    active.updated_at = ActiveValue::Set(Utc::now().fixed_offset());
    let updated = active
        .update(&txn)
        .await
        .map_err(|e| PromoteError::internal("clear production's pointer", e))?;
    custom_apps_environments::record_move(
        &txn,
        app_id,
        &AppEnvironment::Production,
        None,
        EnvAction::Unpublish,
        actor,
    )
    .await
    .map_err(|e| PromoteError::internal("record production's event", e))?;
    txn.commit()
        .await
        .map_err(|e| PromoteError::internal("commit the unpublish", e))?;
    // The serve path caches the `apps` row itself (pointers, `published_at`):
    // without this the unpublish takes up to the cache TTL to appear.
    custom_apps_cache::invalidate_app_resolution_cache();
    Ok(updated)
}
