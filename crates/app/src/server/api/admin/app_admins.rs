//! `/api/admin/app-admins` — the **platform-grant control plane**, OXY_OWNER-managed.
//!
//! Sits behind the `oxy_owner_guard` middleware so only Oxy staff (members of the
//! `OXY_OWNER` email allow-list) can issue or revoke a grant. That is not merely
//! "sensitive": the grant table is the one surface no capability may reach, because a
//! capability that could edit it would let its holder widen their own ceiling and the
//! ceiling would mean nothing. It is a boolean the model cannot touch, on purpose.
//!
//! A grant is `(role × scope)`:
//! * **role** — a `PlatformRole` preset, expanded to capabilities in `oxy-authz`.
//!   `global_admin` is the historical meaning of a row here; `app_operator` ships and
//!   develops custom apps and holds nothing else.
//! * **scope** — every org, or an explicit list. Scope narrows *tenant* reach; it does
//!   not hide console sections (see `platform_cap_guard`).
//!
//! Replaces the legacy `OXY_APP_ADMINS` env var, which can only ever seed
//! `global_admin` at unbounded scope.

use std::collections::HashMap;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use entity::prelude::AppAdmins;
use entity::{app_admin_scope_orgs, app_admins};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_authz::PlatformRole;
use sea_orm::{ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::authz::globals::invalidate_admin_cache;
use crate::server::router::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/app-admins", get(list_app_admins).post(create_app_admin))
        .route("/app-admins/{id}", delete(delete_app_admin))
}

#[derive(Serialize)]
pub struct AppAdminResponse {
    pub id: Uuid,
    pub email: String,
    pub granted_by: Option<Uuid>,
    pub created_at: String,
    /// When the grant last changed. Equal to `created_at` for one that never has.
    pub updated_at: String,
    /// `PlatformRole::as_str` — `global_admin` or `app_operator`.
    pub role: String,
    /// `true` = every org. `false` = the orgs in `scope_org_ids`.
    pub scope_all: bool,
    /// Empty when `scope_all` is true. Populated only by the detail read, which is why
    /// the list endpoint leaves it empty rather than issuing a query per row.
    pub scope_org_ids: Vec<Uuid>,
    /// The capabilities `role` expands to, so the console can render what a grant
    /// actually buys without duplicating the expansion in TypeScript. Derived, never
    /// stored — the model stays the only definition.
    pub capabilities: Vec<String>,
}

impl From<app_admins::Model> for AppAdminResponse {
    fn from(m: app_admins::Model) -> Self {
        let capabilities = PlatformRole::from_str(&m.role)
            .map(|r| r.caps().iter().map(|c| c.as_str().to_string()).collect())
            .unwrap_or_default();
        Self {
            id: m.id,
            email: m.email,
            granted_by: m.granted_by,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
            role: m.role,
            scope_all: m.scope_all,
            scope_org_ids: Vec::new(),
            capabilities,
        }
    }
}

pub async fn list_app_admins() -> Result<Json<Vec<AppAdminResponse>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("list_app_admins DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let rows = AppAdmins::find()
        .order_by_asc(app_admins::Column::Email)
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("list_app_admins query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Scope in ONE query for the whole page, grouped in memory. The console renders a
    // grant's reach inline, so an N+1 here would be a query per staff member on every
    // page load. Only bounded grants have rows, so this is usually empty.
    let mut scopes: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    if rows.iter().any(|r| !r.scope_all) {
        let bounded: Vec<Uuid> = rows.iter().filter(|r| !r.scope_all).map(|r| r.id).collect();
        for s in app_admin_scope_orgs::Entity::find()
            .filter(app_admin_scope_orgs::Column::AppAdminId.is_in(bounded))
            .all(&db)
            .await
            .map_err(|e| {
                tracing::error!("list_app_admins scope query failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        {
            scopes.entry(s.app_admin_id).or_default().push(s.org_id);
        }
    }

    Ok(Json(
        rows.into_iter()
            .map(|m| {
                let id = m.id;
                let mut r: AppAdminResponse = m.into();
                r.scope_org_ids = scopes.remove(&id).unwrap_or_default();
                r
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct CreateAppAdminBody {
    pub email: String,
    /// Omitted = `global_admin`, which is what every row meant before roles existed.
    /// Defaulting to the *broad* role is the wrong direction for least privilege, but
    /// it is the right direction for compatibility: an existing client that posts only
    /// an email must keep creating the grant it always did.
    #[serde(default)]
    pub role: Option<String>,
    /// Omitted = unbounded. Supplying an empty list bounds the grant to nothing, which
    /// is a valid (if useless) grant and deliberately not an error — the alternative is
    /// silently promoting it to unbounded.
    #[serde(default)]
    pub scope_org_ids: Option<Vec<Uuid>>,
}

pub async fn create_app_admin(
    State(_): State<AppState>,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Json(body): Json<CreateAppAdminBody>,
) -> Result<Json<AppAdminResponse>, StatusCode> {
    let email = body.email.trim().to_ascii_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let db = establish_connection().await.map_err(|e| {
        tracing::error!("create_app_admin DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Reject a role this build cannot expand, rather than storing a string the loader
    // will later drop. The write side and `PlatformRole::from_str` must agree, or a
    // typo becomes a grant that silently authorizes nothing.
    let role = match body.role.as_deref() {
        None => PlatformRole::GlobalAdmin,
        Some(r) => PlatformRole::from_str(r).ok_or(StatusCode::BAD_REQUEST)?,
    };
    let scope_all = body.scope_org_ids.is_none();
    let scope_org_ids = body.scope_org_ids.unwrap_or_default();

    // **Upsert, not create-or-ignore.** This used to return the existing row untouched,
    // which made a grant permanently unchangeable: with no PATCH on the router, there
    // was no way to downgrade a Global Admin to App Operator or bound an unbounded
    // grant — and the call answered 200 with the OLD role, so the caller was told a
    // change landed that never did. Enforcement with no usable write path is the exact
    // failure this whole model exists to correct; a half-present one is worse, because
    // it lies.
    //
    // `ON CONFLICT (email) DO UPDATE` rather than read-then-branch: the check-then-write
    // shape let two concurrent POSTs for a new email both see "absent", both insert, and
    // one die on the unique index. Rare and owner-only, but the atomic form is no harder
    // to read and has no such window.
    //
    // `granted_by` follows the most recent decision — the audit question an owner asks
    // is "who set it to THIS", not "who first granted anything" — and `updated_at`
    // records when, which `created_at` can no longer answer now that rows mutate.
    //
    // Both timestamps come from the SAME instant. Leaving `created_at` NotSet lets it
    // fall to the column default `now()` — the DATABASE clock, evaluated when the
    // statement runs — while `updated_at` carries the app clock, read before the
    // statement was even sent. The two never match, so "never changed" (rendered as an
    // em-dash) became unreachable for every console-issued grant, and a brand-new one
    // typically showed Changed *earlier* than Added.
    //
    // The other two write paths hid it: the migration backfills the two equal, and the
    // env bootstrap leaves both NotSet so they share one transaction timestamp. Only
    // this path — the one the column was added for — got it wrong, which is why
    // verifying the migration against a real database said nothing about it.
    let now = chrono::Utc::now().fixed_offset();
    app_admins::Entity::insert(app_admins::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        email: ActiveValue::Set(email.clone()),
        granted_by: ActiveValue::Set(Some(actor.id)),
        created_at: ActiveValue::Set(now),
        role: ActiveValue::Set(role.as_str().to_string()),
        scope_all: ActiveValue::Set(scope_all),
        updated_at: ActiveValue::Set(now),
    })
    .on_conflict(
        sea_orm::sea_query::OnConflict::column(app_admins::Column::Email)
            .update_columns([
                app_admins::Column::Role,
                app_admins::Column::ScopeAll,
                app_admins::Column::GrantedBy,
                app_admins::Column::UpdatedAt,
            ])
            .to_owned(),
    )
    .exec(&db)
    .await
    .map_err(|e| {
        tracing::error!("create_app_admin upsert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Re-read rather than trust the insert's returned id: on the conflict path the row
    // keeps its ORIGINAL id, and the scope rows below are keyed by it.
    let model = AppAdmins::find()
        .filter(app_admins::Column::Email.eq(email))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("create_app_admin read-back failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let admin_id = model.id;

    // Scope is REPLACED, never merged: the request states the whole reach, so stale rows
    // from a previous bounding must go or a re-scoped grant keeps orgs the owner just
    // removed. Delete-then-insert in that order — the window between them reaches
    // fewer orgs, never more.
    app_admin_scope_orgs::Entity::delete_many()
        .filter(app_admin_scope_orgs::Column::AppAdminId.eq(admin_id))
        .exec(&db)
        .await
        .map_err(|e| {
            tracing::error!("create_app_admin scope clear failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Scope rows go in AFTER the grant row (FK) and only when bounded. A failure here
    // leaves a grant with `scope_all = false` and no rows — which reaches NOTHING. The
    // partial state is the safe one, which is why this isn't wrapped in a transaction
    // to "protect" it.
    if !scope_all && !scope_org_ids.is_empty() {
        let rows: Vec<_> = scope_org_ids
            .iter()
            .map(|org_id| app_admin_scope_orgs::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                app_admin_id: ActiveValue::Set(admin_id),
                org_id: ActiveValue::Set(*org_id),
                created_at: ActiveValue::NotSet,
                created_by: ActiveValue::Set(Some(actor.id)),
            })
            .collect();
        app_admin_scope_orgs::Entity::insert_many(rows)
            .exec(&db)
            .await
            .map_err(|e| {
                tracing::error!("create_app_admin scope insert failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    }

    invalidate_admin_cache();
    let mut response: AppAdminResponse = model.into();
    // The response must describe what was just WRITTEN — `From<Model>` defaults
    // `scope_org_ids` to empty, which would render an unbounded-looking reach for a
    // grant that is in fact bounded. No branch on `scope_all` needed: it is exactly
    // `body.scope_org_ids.is_none()`, so this vector is already empty when it's true.
    response.scope_org_ids = scope_org_ids;
    Ok(Json(response))
}

pub async fn delete_app_admin(Path(id): Path<Uuid>) -> Result<StatusCode, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("delete_app_admin DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let res = AppAdmins::delete_by_id(id).exec(&db).await.map_err(|e| {
        tracing::error!("delete_app_admin failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if res.rows_affected == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    invalidate_admin_cache();
    Ok(StatusCode::NO_CONTENT)
}
