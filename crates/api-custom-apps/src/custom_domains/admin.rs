//! The staff console's custom-domain routes, under `/api/customer-apps/{id}`:
//! list, add, verify, remove. They sit in the admin tree, so the app-admin
//! guard stack and the app's org scope have run before any handler here.
//! Transport only — the rules are `oxy_custom_apps::custom_domains`.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, FixedOffset};
use entity::app_custom_domains;
use oxy::database::client::establish_connection;
use oxy_app_core::agent_refusal::RefuseSandboxAgent;
use oxy_app_core::audit::RequestActor;
use oxy_app_core::custom_domain_host::{challenge_name, challenge_value, cname_target};
use oxy_custom_apps::custom_domains::{self, DomainOpError, dns::DohResolver};
use sea_orm::{DatabaseConnection, EntityTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One domain as the console shows it, with the records the customer creates.
#[derive(Debug, Serialize)]
pub struct DomainView {
    pub id: Uuid,
    pub domain: String,
    /// `pending` until the TXT record matched, then `verified`.
    pub status: &'static str,
    pub url: String,
    pub txt_name: String,
    pub txt_value: String,
    pub verified_at: Option<DateTime<FixedOffset>>,
    pub last_checked_at: Option<DateTime<FixedOffset>>,
    pub last_check_error: Option<String>,
    pub created_at: DateTime<FixedOffset>,
}

impl From<app_custom_domains::Model> for DomainView {
    fn from(row: app_custom_domains::Model) -> Self {
        Self {
            id: row.id,
            status: if row.verified_at.is_some() {
                "verified"
            } else {
                "pending"
            },
            url: format!("https://{}/", row.domain),
            txt_name: challenge_name(&row.domain),
            txt_value: challenge_value(&row.verification_token),
            domain: row.domain,
            verified_at: row.verified_at,
            last_checked_at: row.last_checked_at,
            last_check_error: row.last_check_error,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DomainList {
    pub domains: Vec<DomainView>,
    /// Where the customer points the domain's CNAME (`OXY_CUSTOM_DOMAIN_TARGET`);
    /// `null` on a deployment with no custom-domain edge.
    pub cname_target: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddDomain {
    pub domain: String,
}

/// A refusal, in the console's `{ error, message }` shape.
#[derive(Debug)]
pub struct DomainHttpError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl DomainHttpError {
    fn app_not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "app_not_found",
            message: "no such app".into(),
        }
    }

    fn internal(e: impl std::fmt::Display) -> Self {
        tracing::error!("custom domain route failed: {e}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "custom domain request failed".into(),
        }
    }
}

impl From<DomainOpError> for DomainHttpError {
    fn from(e: DomainOpError) -> Self {
        let (status, code) = match &e {
            DomainOpError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_domain"),
            DomainOpError::Exists(_) => (StatusCode::CONFLICT, "domain_exists"),
            DomainOpError::Taken(_) => (StatusCode::CONFLICT, "domain_taken"),
            DomainOpError::NotFound => (StatusCode::NOT_FOUND, "domain_not_found"),
            DomainOpError::Db(_) => return Self::internal(e),
        };
        Self {
            status,
            code,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for DomainHttpError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.code, "message": self.message })),
        )
            .into_response()
    }
}

/// The database, after checking app `id` exists.
async fn app_db(app_id: Uuid) -> Result<DatabaseConnection, DomainHttpError> {
    let db = establish_connection()
        .await
        .map_err(DomainHttpError::internal)?;
    entity::prelude::Apps::find_by_id(app_id)
        .one(&db)
        .await
        .map_err(DomainHttpError::internal)?
        .ok_or_else(DomainHttpError::app_not_found)?;
    Ok(db)
}

/// `GET /customer-apps/{id}/domains`
pub async fn list_domains(Path(app_id): Path<Uuid>) -> Result<Json<DomainList>, DomainHttpError> {
    let db = app_db(app_id).await?;
    let rows = custom_domains::list(&db, app_id).await?;
    Ok(Json(DomainList {
        domains: rows.into_iter().map(DomainView::from).collect(),
        cname_target: cname_target(),
    }))
}

/// `POST /customer-apps/{id}/domains`
pub async fn add_domain(
    _: RefuseSandboxAgent,
    actor: RequestActor,
    Path(app_id): Path<Uuid>,
    Json(body): Json<AddDomain>,
) -> Result<(StatusCode, Json<DomainView>), DomainHttpError> {
    let db = app_db(app_id).await?;
    let row = custom_domains::claim(&db, app_id, &body.domain, actor.id).await?;
    tracing::info!(%app_id, domain = %row.domain, actor = %actor.id, "custom domain claimed");
    Ok((StatusCode::CREATED, Json(row.into())))
}

/// `POST /customer-apps/{id}/domains/{domain_id}/verify`
pub async fn verify_domain(
    _: RefuseSandboxAgent,
    Path((app_id, domain_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DomainView>, DomainHttpError> {
    let db = app_db(app_id).await?;
    let resolver = DohResolver::from_env();
    let row = custom_domains::verify(&db, &resolver, app_id, domain_id).await?;
    if row.verified_at.is_some() {
        tracing::info!(%app_id, domain = %row.domain, "custom domain verified");
    }
    Ok(Json(row.into()))
}

/// `DELETE /customer-apps/{id}/domains/{domain_id}`
pub async fn remove_domain(
    _: RefuseSandboxAgent,
    actor: RequestActor,
    Path((app_id, domain_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, DomainHttpError> {
    let db = app_db(app_id).await?;
    let row = custom_domains::remove(&db, app_id, domain_id, actor.id).await?;
    tracing::info!(%app_id, domain = %row.domain, actor = %actor.id, "custom domain removed");
    Ok(StatusCode::NO_CONTENT)
}
