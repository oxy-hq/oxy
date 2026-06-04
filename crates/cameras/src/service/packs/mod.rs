//! Domain pack CRUD and starter management.
//!
//! A domain pack is a workspace-scoped bundle of camera roles + VLM
//! prompt templates + output schema.
//!
//! Submodules:
//!
//! - [`body`] — typed view of the persisted JSONB body.
//! - [`starter`] — Rust constants for the curated starter library.
//! - [`validate`] — minijinja render-validation used at PUT time.

pub mod body;
pub mod starter;
pub mod updates;
pub mod validate;

use crate::entities::domain_packs;
use crate::service::{ServiceError, ServiceResult};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};
use uuid::Uuid;

use body::PackBody;

/// Return the active pack for a workspace, auto-seeding the
/// `restaurant` starter on first read.
///
/// First-read seeding (rather than per-workspace backfill in the
/// migration) keeps the migration idempotent and lets us bump the
/// starter library independently of the schema. Trade-off: the
/// first call after workspace creation does extra work; subsequent
/// calls are a single SELECT.
pub async fn get_active(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<domain_packs::Model> {
    if let Some(row) = domain_packs::Entity::find()
        .filter(domain_packs::Column::WorkspaceId.eq(workspace_id))
        .filter(domain_packs::Column::IsActive.eq(true))
        .one(db)
        .await?
    {
        return Ok(row);
    }
    // No active pack — seed the default starter for this workspace.
    let starter_key = "restaurant";
    create_from_starter(db, workspace_id, starter_key).await?;
    domain_packs::Entity::find()
        .filter(domain_packs::Column::WorkspaceId.eq(workspace_id))
        .filter(domain_packs::Column::IsActive.eq(true))
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)
}

/// List every pack in a workspace (active + inactive).
pub async fn list(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<domain_packs::Model>> {
    Ok(domain_packs::Entity::find()
        .filter(domain_packs::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await?)
}

/// Fork a starter into the workspace as a new pack. The new pack
/// becomes the active one (atomically deactivates any previous
/// active pack via [`set_active`]'s transaction).
///
/// `pack_key` defaults to `starter_key` — operators can fork the
/// same starter multiple times with different pack_keys (e.g. one
/// `restaurant` pack per site cluster). Conflicts on
/// `(workspace_id, pack_key)` surface as `ServiceError::Conflict`.
pub async fn create_from_starter(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    starter_key: &str,
) -> ServiceResult<domain_packs::Model> {
    let starter = starter::list()
        .iter()
        .find(|s| s.key == starter_key)
        .ok_or_else(|| ServiceError::InvalidInput(format!("unknown starter `{starter_key}`")))?;
    let body = starter::body_for(starter_key).ok_or_else(|| {
        ServiceError::InvalidInput(format!("starter `{starter_key}` has no body"))
    })?;
    // Render-validate even the shipped starter — protects against a
    // bad commit landing a malformed template via the next deploy.
    validate::check_pack_renders(&body)?;

    let body_json = serde_json::to_value(&body)
        .map_err(|e| ServiceError::InvalidInput(format!("serialize pack body: {e}")))?;
    let now = chrono::Utc::now().fixed_offset();

    // Wrap in a transaction so the deactivation + insert are atomic
    // against the partial unique index.
    let tx = db.begin().await?;
    deactivate_all(&tx, workspace_id).await?;
    let inserted = domain_packs::ActiveModel {
        id: Set(Uuid::new_v4()),
        workspace_id: Set(workspace_id),
        pack_key: Set(starter.key.to_string()),
        name: Set(starter.name.to_string()),
        description: Set(Some(starter.description.to_string())),
        starter_key: Set(starter.key.to_string()),
        starter_version: Set(starter.version.to_string()),
        locally_modified: Set(false),
        body: Set(body_json),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&tx)
    .await?;
    tx.commit().await?;
    Ok(inserted)
}

/// Replace a pack's body. Marks `locally_modified = true` so the
/// curator UI can surface "discard local changes" alongside
/// upstream updates.
///
/// Render-validates the new body before persisting — a typo or a
/// removed variable that breaks a prompt is rejected at PUT time
/// rather than discovered on the next worker poll.
pub async fn update_body(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    pack_key: &str,
    new_body: PackBody,
) -> ServiceResult<domain_packs::Model> {
    validate::check_pack_renders(&new_body)?;
    let row = find_by_key(db, workspace_id, pack_key).await?;
    let body_json = serde_json::to_value(&new_body)
        .map_err(|e| ServiceError::InvalidInput(format!("serialize pack body: {e}")))?;
    let mut am: domain_packs::ActiveModel = row.into();
    am.body = Set(body_json);
    am.locally_modified = Set(true);
    am.updated_at = Set(chrono::Utc::now().fixed_offset());
    Ok(am.update(db).await?)
}

/// Switch which pack is active. Wraps the deactivate + activate in
/// a transaction to keep the partial unique index happy: the
/// previously-active row must drop `is_active` before the new one
/// flips it on.
pub async fn set_active(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    pack_key: &str,
) -> ServiceResult<domain_packs::Model> {
    let tx = db.begin().await?;
    deactivate_all(&tx, workspace_id).await?;
    let row = domain_packs::Entity::find()
        .filter(domain_packs::Column::WorkspaceId.eq(workspace_id))
        .filter(domain_packs::Column::PackKey.eq(pack_key))
        .one(&tx)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let mut am: domain_packs::ActiveModel = row.into();
    am.is_active = Set(true);
    am.updated_at = Set(chrono::Utc::now().fixed_offset());
    let updated = am.update(&tx).await?;
    tx.commit().await?;
    Ok(updated)
}

async fn find_by_key(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    pack_key: &str,
) -> ServiceResult<domain_packs::Model> {
    domain_packs::Entity::find()
        .filter(domain_packs::Column::WorkspaceId.eq(workspace_id))
        .filter(domain_packs::Column::PackKey.eq(pack_key))
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)
}

/// Deactivate every pack in `workspace_id`. Called inside the
/// transactions in [`create_from_starter`] and [`set_active`] so
/// the unique-active invariant always holds between commits.
async fn deactivate_all<C>(conn: &C, workspace_id: Uuid) -> ServiceResult<()>
where
    C: sea_orm::ConnectionTrait,
{
    domain_packs::Entity::update_many()
        .col_expr(
            domain_packs::Column::IsActive,
            sea_orm::sea_query::Expr::value(false),
        )
        .col_expr(
            domain_packs::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now().fixed_offset()),
        )
        .filter(domain_packs::Column::WorkspaceId.eq(workspace_id))
        .filter(domain_packs::Column::IsActive.eq(true))
        .exec(conn)
        .await?;
    Ok(())
}
