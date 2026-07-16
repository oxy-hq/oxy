//! `/api/partners` — the partner self-service surface.
//!
//! Distinct from `/api/admin/partners` (Oxy-staff provisioning): this is what a
//! partner's own people use to run their subtree. Every org-scoped handler passes
//! through [`require_org_scope`] — the cross-tenant boundary — which checks the
//! capability AND that **this person** is assigned the org, returning `404` for
//! out-of-set orgs so existence isn't leaked.
//!
//! Two people at the same partner can see completely different things here: an
//! `account_manager` scoped to northwind can onboard clients and manage members
//! but cannot query data; a `developer` scoped to globex can query and ship apps
//! but cannot invite anyone. That is `role ∩ ceiling ∩ assignment`, resolved once
//! in `partner_authz::resolve_scope` and enforced by Cedar.

mod app_mgmt;
mod health;
mod orgs;
mod people;
mod publish_tokens;
mod workspaces;
mod write;

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use entity::prelude::{Apps, OrgMembers, Organizations, PartnerOrgs, Users};
use entity::{apps, org_members, organizations, partner_orgs, users};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::server::api::audit::events_for_partner;
use crate::server::api::middlewares::partner_authz::{
    PartnerCapability, PartnerScope, scopes_for_user,
};
use crate::server::api::middlewares::partner_context::{PartnerActor, partner_middleware};
use crate::server::api::middlewares::partner_policy;
use crate::server::router::AppState;

/// Top-level list route + the partner-scoped subtree (wrapped in
/// `partner_middleware`, which resolves [`PartnerScope`] and 403s anyone holding
/// no partner role in that org).
pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/partners", get(list_my_partners))
}

pub(crate) fn scoped_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(overview))
        // A partner can now ONBOARD a client itself (`create_orgs`) — the hole that
        // made the reseller channel a support queue. Attaching an *existing* org
        // stays Oxy-only; creating a brand-new one affects nobody else's tenant.
        .route("/orgs", get(list_orgs).post(orgs::create_org))
        .route("/orgs/{org_id}", patch(orgs::update_org))
        .route(
            "/orgs/{org_id}/members",
            get(list_org_members).post(write::invite_member),
        )
        .route(
            "/orgs/{org_id}/members/{user_id}",
            patch(write::update_member_role).delete(write::remove_member),
        )
        .route(
            "/orgs/{org_id}/workspaces",
            get(workspaces::list_org_workspaces),
        )
        .route("/orgs/{org_id}/apps", get(app_mgmt::list_org_apps))
        .route(
            "/apps/{app_id}/publish",
            post(app_mgmt::publish_app).delete(app_mgmt::unpublish_app),
        )
        // App-scoped publish tokens for a client's app (CI credentials). Confined to
        // the one app + consent at publish time — see `publish_tokens`.
        .route(
            "/apps/{app_id}/publish-tokens",
            get(publish_tokens::list_tokens).post(publish_tokens::create_token),
        )
        .route(
            "/apps/{app_id}/publish-tokens/{token_id}",
            axum::routing::delete(publish_tokens::revoke_token),
        )
        // The partner's OWN people: who is an operator. Managed by the partner org's
        // owner/admin — see `people.rs`.
        .route("/people", get(people::list_people))
        .route(
            "/people/{org_member_id}",
            put(people::grant_access).delete(people::revoke_access),
        )
        .route("/health", get(health::list_health))
        .route("/audit", get(partner_audit))
        .layer(axum::middleware::from_fn(partner_middleware))
}

pub(super) async fn db() -> Result<DatabaseConnection, StatusCode> {
    establish_connection().await.map_err(|e| {
        tracing::error!("partner_console: DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub(super) fn internal<E: std::fmt::Display>(ctx: &'static str) -> impl Fn(E) -> StatusCode {
    move |e| {
        tracing::error!("partner_console: {ctx}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// The cross-tenant boundary. A partner action on a client org is allowed only if
/// the actor's **role ∩ ceiling** holds `cap` AND the org is in their assigned set.
/// Out-of-set orgs return `404` (not `403`) so a partner can't probe which orgs
/// exist outside their subtree.
pub(super) async fn require_org_scope(
    _db: &DatabaseConnection,
    scope: &PartnerScope,
    org_id: Uuid,
    cap: PartnerCapability,
) -> Result<(), StatusCode> {
    // `scope.org_ids` is the partner's managed clients — every operator reaches all
    // of them. Hand it to Cedar and let it decide both ownership (`resource in
    // principal`) and capability.
    let managed = &scope.org_ids;
    if partner_policy::authorize_org(scope, org_id, managed, cap) {
        return Ok(());
    }
    // Deny: 404 when the partner doesn't manage the org (don't leak existence),
    // else 403 (manages it but the ceiling lacks the capability).
    // Presentation only — the security decision was Cedar's above.
    Err(if managed.contains(&org_id) {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::NOT_FOUND
    })
}

// ── DTOs ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct MyPartner {
    /// The partner IS an org, so this is an org id.
    pub partner_id: String,
    pub slug: String,
    pub name: String,
    pub org_count: usize,
    /// The partner's ceiling — what any operator here may do. The UI shows exactly
    /// what is possible; there are no per-person roles.
    pub capabilities: CapabilitiesDto,
}

#[derive(Serialize)]
pub struct CapabilitiesDto {
    pub manage_members: bool,
    pub manage_apps: bool,
    pub develop_apps: bool,
    pub view_audit: bool,
    pub manage_billing: bool,
    pub manage_secrets: bool,
    pub create_orgs: bool,
    pub manage_org_settings: bool,
}

impl From<&PartnerScope> for CapabilitiesDto {
    fn from(s: &PartnerScope) -> Self {
        Self {
            manage_members: s.capabilities.manage_members,
            manage_apps: s.capabilities.manage_apps,
            develop_apps: s.capabilities.develop_apps,
            view_audit: s.capabilities.view_audit,
            manage_billing: s.capabilities.manage_billing,
            manage_secrets: s.capabilities.manage_secrets,
            create_orgs: s.capabilities.create_orgs,
            manage_org_settings: s.capabilities.manage_org_settings,
        }
    }
}

#[derive(Serialize)]
pub struct ChildOrg {
    pub org_id: Uuid,
    pub name: String,
    pub slug: String,
    pub member_count: usize,
    pub app_count: usize,
}

#[derive(Serialize)]
pub struct OrgMemberDto {
    pub user_id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub role: String,
}

#[derive(Serialize)]
pub struct AuditEventDto {
    pub id: Uuid,
    pub created_at: String,
    pub actor_email: String,
    pub action: String,
    pub org_id: Option<Uuid>,
    pub target_label: Option<String>,
    pub outcome: String,
}

// ── handlers ─────────────────────────────────────────────────────────────

/// `GET /partners` — partners the caller holds a role at (drives the console entry).
pub async fn list_my_partners(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
) -> Result<Json<Vec<MyPartner>>, StatusCode> {
    let db = db().await?;
    let scopes = scopes_for_user(&db, user.id, &user.email).await;
    if scopes.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let ids: Vec<Uuid> = scopes.iter().map(|s| s.partner_id).collect();
    // The partner's name comes from the ORG — there is no separate partner record.
    let names: HashMap<Uuid, String> = Organizations::find()
        .filter(organizations::Column::Id.is_in(ids.clone()))
        .all(&db)
        .await
        .map_err(internal("load partner orgs"))?
        .into_iter()
        .map(|o| (o.id, o.name))
        .collect();

    let mut org_counts: HashMap<Uuid, usize> = HashMap::new();
    for row in PartnerOrgs::find()
        .filter(partner_orgs::Column::PartnerOrgId.is_in(ids))
        .all(&db)
        .await
        .map_err(internal("count orgs"))?
    {
        *org_counts.entry(row.partner_org_id).or_default() += 1;
    }

    let out = scopes
        .iter()
        .map(|s| MyPartner {
            partner_id: s.partner_id.to_string(),
            slug: s.slug.clone(),
            name: names.get(&s.partner_id).cloned().unwrap_or_default(),
            org_count: org_counts.get(&s.partner_id).copied().unwrap_or(0),
            capabilities: s.into(),
        })
        .collect();
    Ok(Json(out))
}

/// `GET /partners/{id}` — the clients THIS person handles, with counts.
pub async fn overview(
    PartnerActor(scope): PartnerActor,
) -> Result<Json<Vec<ChildOrg>>, StatusCode> {
    let db = db().await?;
    child_orgs(&db, &scope).await.map(Json)
}

/// `GET /partners/{id}/orgs` — same list (explicit route for the UI).
pub async fn list_orgs(
    PartnerActor(scope): PartnerActor,
) -> Result<Json<Vec<ChildOrg>>, StatusCode> {
    let db = db().await?;
    child_orgs(&db, &scope).await.map(Json)
}

pub(super) async fn child_orgs(
    db: &DatabaseConnection,
    scope: &PartnerScope,
) -> Result<Vec<ChildOrg>, StatusCode> {
    let org_ids = scope.org_ids.clone();
    if org_ids.is_empty() {
        return Ok(Vec::new());
    }

    let orgs = Organizations::find()
        .filter(organizations::Column::Id.is_in(org_ids.clone()))
        .all(db)
        .await
        .map_err(internal("load orgs"))?;

    let mut member_counts: HashMap<Uuid, usize> = HashMap::new();
    for m in OrgMembers::find()
        .filter(org_members::Column::OrgId.is_in(org_ids.clone()))
        .all(db)
        .await
        .map_err(internal("count members"))?
    {
        *member_counts.entry(m.org_id).or_default() += 1;
    }

    let mut app_counts: HashMap<Uuid, usize> = HashMap::new();
    for a in Apps::find()
        .filter(apps::Column::OrgId.is_in(org_ids))
        .all(db)
        .await
        .map_err(internal("count apps"))?
    {
        *app_counts.entry(a.org_id).or_default() += 1;
    }

    Ok(orgs
        .into_iter()
        .map(|o| ChildOrg {
            member_count: member_counts.get(&o.id).copied().unwrap_or(0),
            app_count: app_counts.get(&o.id).copied().unwrap_or(0),
            org_id: o.id,
            name: o.name,
            slug: o.slug,
        })
        .collect())
}

/// `GET /partners/{id}/orgs/{org_id}/members` — members of one client org.
pub async fn list_org_members(
    PartnerActor(scope): PartnerActor,
    Path((_partner_org_id, org_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<OrgMemberDto>>, StatusCode> {
    let db = db().await?;
    require_org_scope(&db, &scope, org_id, PartnerCapability::ManageMembers).await?;

    let members = OrgMembers::find()
        .filter(org_members::Column::OrgId.eq(org_id))
        .all(&db)
        .await
        .map_err(internal("load members"))?;

    let user_ids: Vec<Uuid> = members.iter().map(|m| m.user_id).collect();
    let users: HashMap<Uuid, users::Model> = Users::find()
        .filter(users::Column::Id.is_in(user_ids))
        .all(&db)
        .await
        .map_err(internal("load users"))?
        .into_iter()
        .map(|u| (u.id, u))
        .collect();

    let out = members
        .into_iter()
        .map(|m| {
            let u = users.get(&m.user_id);
            OrgMemberDto {
                user_id: m.user_id,
                email: u.map(|u| u.email.clone()).unwrap_or_default(),
                name: u.map(|u| u.name.clone()),
                role: m.role.as_str().to_string(),
            }
        })
        .collect();
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct AuditQuery {
    pub limit: Option<u64>,
}

/// `GET /partners/{id}/audit` — the audit view, scoped to this person's clients.
pub async fn partner_audit(
    PartnerActor(scope): PartnerActor,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEventDto>>, StatusCode> {
    if !partner_policy::authorize_capability(&scope, PartnerCapability::ViewAudit) {
        return Err(StatusCode::FORBIDDEN);
    }
    let db = db().await?;
    let org_ids = scope.org_ids.clone();
    let limit = q.limit.unwrap_or(200).min(1000);

    let events = events_for_partner(&db, scope.partner_id, &org_ids, limit)
        .await
        .map_err(internal("load audit"))?;

    let out = events
        .into_iter()
        .map(|e| AuditEventDto {
            id: e.id,
            created_at: e.created_at.to_rfc3339(),
            actor_email: e.actor_email,
            action: e.action,
            org_id: e.org_id,
            target_label: e.target_label,
            outcome: e.outcome,
        })
        .collect();
    Ok(Json(out))
}
