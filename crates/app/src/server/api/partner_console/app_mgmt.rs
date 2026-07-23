//! Partner custom-app management for `/api/partners/{id}/...`.
//!
//! The `manage_apps` capability: a partner admin can see the custom apps in a
//! managed org and control their published (live-to-viewers) state. Every
//! handler passes through [`require_org_scope`] with `ManageApps`, and
//! app-centric routes additionally confirm the app's org is one the partner
//! manages (via [`load_managed_app`]) before acting.
//!
//! Publish/unpublish are pointer/flag moves on the `apps` row (the bytes already
//! live in S3). Build-pointer rollback/promote-latest reuse the heavier admin
//! build logic and land in a follow-up — through the same gate.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use entity::apps;
use entity::prelude::Apps;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;
use uuid::Uuid;

use super::{db, internal, require_org_scope};
use crate::server::api::admin::apps::handlers as admin_apps;
use crate::server::api::audit::{self, ActorType, AuditEntry};
use crate::server::api::middlewares::partner_authz::{PartnerCapability, PartnerScope};
use crate::server::api::middlewares::partner_context::PartnerActor;

#[derive(Serialize)]
pub struct PartnerAppDto {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub published: bool,
}

impl From<apps::Model> for PartnerAppDto {
    fn from(a: apps::Model) -> Self {
        Self {
            published: a.published_at.is_some(),
            id: a.id,
            slug: a.slug,
            name: a.name,
        }
    }
}

/// `GET /partners/{id}/orgs/{org_id}/apps` — custom apps in a managed org.
pub async fn list_org_apps(
    PartnerActor(scope): PartnerActor,
    Path((_partner_id, org_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<PartnerAppDto>>, StatusCode> {
    let db = db().await?;
    require_org_scope(&db, &scope, org_id, PartnerCapability::ManageApps).await?;

    let apps = Apps::find()
        .filter(apps::Column::OrgId.eq(org_id))
        .order_by_asc(apps::Column::Name)
        .all(&db)
        .await
        .map_err(internal("list apps"))?;
    Ok(Json(apps.into_iter().map(Into::into).collect()))
}

/// Load an app and confirm the partner manages its org (with `ManageApps`).
/// Returns `404` for a missing app or one outside the partner's subtree.
async fn load_managed_app(
    db: &sea_orm::DatabaseConnection,
    scope: &PartnerScope,
    app_id: Uuid,
) -> Result<apps::Model, StatusCode> {
    let app = Apps::find_by_id(app_id)
        .one(db)
        .await
        .map_err(internal("load app"))?
        .ok_or(StatusCode::NOT_FOUND)?;
    require_org_scope(db, scope, app.org_id, PartnerCapability::ManageApps).await?;
    Ok(app)
}

/// `POST /partners/{id}/apps/{app_id}/publish` — make the app live to viewers.
pub async fn publish_app(
    PartnerActor(scope): PartnerActor,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path((_partner_id, app_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<PartnerAppDto>, StatusCode> {
    let db = db().await?;
    let app = load_managed_app(&db, &scope, app_id).await?;
    let org_id = app.org_id;
    let (name, slug) = (app.name.clone(), app.slug.clone());

    // Reuse the canonical publish so the partner path can't drift from admin:
    // it repoints published_build_id at the draft, stamps last_promoted_*, and
    // drops the per-app canonical-dir caches. Re-stamping published_at by hand
    // would publish a channel with no live bytes behind it.
    let saved = admin_apps::publish_one(&db, app_id, actor.id)
        .await
        .map_err(|e| e.status)?;
    // Viewers' cached access must drop now, not at TTL (admin does the same).
    crate::server::api::custom_apps_auth::invalidate_access_cache();

    audit::record_best_effort(
        &db,
        AuditEntry::new(actor.email.clone(), "partner.app.published")
            .actor(actor.id, ActorType::PartnerAdmin)
            .partner(scope.partner_id)
            .org(org_id)
            .target("app", app_id.to_string(), format!("{name} ({slug})")),
    )
    .await;
    Ok(Json(saved.into()))
}

/// `DELETE /partners/{id}/apps/{app_id}/publish` — take the app out of view.
pub async fn unpublish_app(
    PartnerActor(scope): PartnerActor,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path((_partner_id, app_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<PartnerAppDto>, StatusCode> {
    let db = db().await?;
    let app = load_managed_app(&db, &scope, app_id).await?;
    let org_id = app.org_id;
    let (name, slug) = (app.name.clone(), app.slug.clone());

    // Canonical unpublish: also nulls published_build_id (no dangling pointer)
    // and drops the per-app canonical-dir caches.
    let saved = admin_apps::unpublish_one(&db, app_id)
        .await
        .map_err(|e| e.status)?;
    crate::server::api::custom_apps_auth::invalidate_access_cache();

    audit::record_best_effort(
        &db,
        AuditEntry::new(actor.email.clone(), "partner.app.unpublished")
            .actor(actor.id, ActorType::PartnerAdmin)
            .partner(scope.partner_id)
            .org(org_id)
            .target("app", app_id.to_string(), format!("{name} ({slug})")),
    )
    .await;
    Ok(Json(saved.into()))
}
