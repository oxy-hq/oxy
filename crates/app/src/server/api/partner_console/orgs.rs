//! `POST/PATCH /api/partners/{partner_org_id}/orgs…` — **partner-initiated
//! onboarding**, the product hole this permission model was written to close.
//!
//! Before this, a partner's org list was read-only and `attach_org` was
//! Oxy-staff-only, so every new client needed an Oxy employee. A reseller channel
//! whose resellers cannot onboard customers is a support queue, not a channel.
//!
//! Creating a client is safe to delegate in a way that *attaching* one is not:
//! a brand-new org affects nobody else's tenant, whereas attaching hands
//! administration of a **live** tenant to a third party (that stays staff-only,
//! in `admin::partners::membership`). Detaching likewise — a partner must not be
//! able to orphan a customer.
//!
//! Everything here is one transaction: org → membership → billing row →
//! attachment → audit, via `record_in_txn`. A half-created client is not a state
//! we allow.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use chrono::Utc;
use entity::org_members::{self, OrgRole};
use entity::prelude::{Apps, OrgMembers, Organizations, Users};
use entity::{organizations, partner_orgs, users};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ChildOrg, db, internal, require_org_scope};
use crate::server::api::audit::{self, ActorType, AuditEntry};
use crate::server::api::middlewares::partner_authz::PartnerCapability;
use crate::server::api::middlewares::partner_context::PartnerActor;
use crate::server::api::organizations::{is_reserved_slug, slugify_name};

#[derive(Deserialize)]
pub struct CreateOrgBody {
    pub name: String,
    pub slug: String,
    /// Optional: the client's first Owner. They own their org from day one — the
    /// partner administers it, it never *belongs* to the partner.
    pub owner_email: Option<String>,
}

#[derive(Serialize)]
pub struct CreatedOrg {
    pub org: ChildOrg,
    /// True when `owner_email` matched no existing user, so nobody was seeded.
    /// The partner should invite them through the normal members flow.
    pub owner_pending: bool,
}

/// `POST /partners/{partner_org_id}/orgs` — create + attach a client org.
pub async fn create_org(
    PartnerActor(scope): PartnerActor,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Json(body): Json<CreateOrgBody>,
) -> Result<Json<CreatedOrg>, StatusCode> {
    // Capability only — there is no target org to scope against yet. This is the
    // one partner write that isn't gated on an existing assignment, which is
    // exactly why `create_orgs` is a ceiling flag that defaults OFF: it mints
    // billable tenants.
    if !crate::server::authz::partner_allows(&scope, None, PartnerCapability::CreateOrgs) {
        return Err(StatusCode::FORBIDDEN);
    }

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let slug = slugify_name(&body.slug);
    if slug.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if is_reserved_slug(&slug) {
        // Not a collision — the name is forbidden because it would shadow a
        // top-level frontend route. 422 lets the client tell the two apart.
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let db = db().await?;

    // Resolve the first owner BEFORE opening the txn. An unknown email is not an
    // error — the org is still created, and the partner invites them after.
    let owner = match body.owner_email.as_deref().map(str::trim) {
        Some(e) if !e.is_empty() => Users::find()
            .filter(users::Column::Email.eq(e.to_ascii_lowercase()))
            .one(&db)
            .await
            .map_err(internal("load owner"))?,
        _ => None,
    };
    let owner_pending = body.owner_email.is_some() && owner.is_none();

    let now = Utc::now().fixed_offset();
    let org_id = Uuid::new_v4();

    let txn = db.begin().await.map_err(internal("begin create org"))?;

    organizations::ActiveModel {
        id: ActiveValue::Set(org_id),
        name: ActiveValue::Set(name.clone()),
        slug: ActiveValue::Set(slug.clone()),
        logo: ActiveValue::NotSet,
        logo_content_type: ActiveValue::NotSet,
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
    }
    .insert(&txn)
    .await
    // The DB UNIQUE on slug is the real guard; a duplicate is a 409, not a 500.
    .map_err(|_| StatusCode::CONFLICT)?;

    if let Some(u) = &owner {
        org_members::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            org_id: ActiveValue::Set(org_id),
            user_id: ActiveValue::Set(u.id),
            role: ActiveValue::Set(OrgRole::Owner),
            created_at: ActiveValue::Set(now),
            updated_at: ActiveValue::Set(now),
        }
        .insert(&txn)
        .await
        .map_err(internal("seed owner"))?;
    }

    // The SubscriptionGuard expects this row to exist for every org. Today a
    // partner-created org bills for itself, exactly like any other — who pays is
    // a business decision we deliberately deferred (design §6).
    entity::org_billing::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(org_id),
        status: ActiveValue::Set(entity::org_billing::BillingStatus::Incomplete),
        seats_paid: ActiveValue::Set(0),
        created_at: ActiveValue::Set(now),
        updated_at: ActiveValue::Set(now),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .map_err(internal("insert billing"))?;

    partner_orgs::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        partner_org_id: ActiveValue::Set(scope.partner_id),
        managed_org_id: ActiveValue::Set(org_id),
        created_by: ActiveValue::Set(Some(actor.id)),
        created_at: ActiveValue::NotSet,
    }
    .insert(&txn)
    .await
    .map_err(internal("attach new org"))?;

    audit::record_in_txn(
        &txn,
        AuditEntry::new(actor.email.clone(), "partner.org.created")
            .actor(actor.id, ActorType::User)
            .partner(scope.partner_id)
            .org(org_id)
            .target("organization", org_id.to_string(), name.clone())
            .metadata(serde_json::json!({
                "slug": slug,
                "owner_email": body.owner_email,
                "owner_seeded": owner.is_some(),
            })),
    )
    .await
    .map_err(internal("audit org.created"))?;

    txn.commit().await.map_err(internal("commit create org"))?;

    Ok(Json(CreatedOrg {
        org: ChildOrg {
            org_id,
            name,
            slug,
            member_count: usize::from(owner.is_some()),
            app_count: 0,
        },
        owner_pending,
    }))
}

#[derive(Deserialize)]
pub struct UpdateOrgBody {
    pub name: Option<String>,
}

/// `PATCH /partners/{partner_org_id}/orgs/{org_id}` — rename a client org.
///
/// The slug is deliberately NOT editable here: it is the org's public identity
/// (subdomains, custom-app URLs), so changing it breaks live links. That is the
/// client's own call, not their partner's.
pub async fn update_org(
    PartnerActor(scope): PartnerActor,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path((_partner_org_id, org_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateOrgBody>,
) -> Result<Json<ChildOrg>, StatusCode> {
    let db = db().await?;
    require_org_scope(&db, &scope, org_id, PartnerCapability::ManageOrgSettings).await?;

    let org = Organizations::find_by_id(org_id)
        .one(&db)
        .await
        .map_err(internal("load org"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    let Some(name) = body.name.as_deref().map(str::trim) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    if name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let before = org.name.clone();

    let txn = db.begin().await.map_err(internal("begin update org"))?;
    let mut active: organizations::ActiveModel = org.into();
    active.name = ActiveValue::Set(name.to_string());
    active.updated_at = ActiveValue::Set(Utc::now().fixed_offset());
    let saved = active.update(&txn).await.map_err(internal("update org"))?;

    audit::record_in_txn(
        &txn,
        AuditEntry::new(actor.email.clone(), "partner.org.updated")
            .actor(actor.id, ActorType::User)
            .partner(scope.partner_id)
            .org(org_id)
            .target("organization", org_id.to_string(), saved.name.clone())
            .change(
                serde_json::json!({ "name": before }),
                serde_json::json!({ "name": saved.name }),
            ),
    )
    .await
    .map_err(internal("audit org.updated"))?;
    txn.commit().await.map_err(internal("commit update org"))?;

    let member_count = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .count(&db)
        .await
        .map_err(internal("count members"))? as usize;
    let app_count = Apps::find()
        .filter(entity::apps::Column::OrgId.eq(org_id))
        .count(&db)
        .await
        .map_err(internal("count apps"))? as usize;

    Ok(Json(ChildOrg {
        org_id,
        name: saved.name,
        slug: saved.slug,
        member_count,
        app_count,
    }))
}
