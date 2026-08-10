//! `/api/admin/audit` — platform audit-log search for Oxy staff.
//!
//! Read-only view over the append-only `audit_events` stream at the **platform**
//! scope (all orgs/partners). Runs under the permissive `/admin` guard
//! (owner-or-app-admin), like the other ops surfaces. Filtering/paging is done
//! in the DB via [`audit::search_events`].

use axum::Json;
use axum::Router;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::get;
use oxy::database::client::establish_connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::router::AppState;
use oxy_app_core::audit::{self, AuditFilter};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/audit", get(list_audit))
        // Makes the tamper-evident chain actually falsifiable (review #9): walks
        // one org's chain in seq order and recomputes every link.
        .route("/audit/verify/{org_id}", get(verify_audit_chain))
}

/// `GET /admin/audit/verify/{org_id}` — recompute an org's hash chain.
pub async fn verify_audit_chain(
    oxy_auth::extractor::AuthenticatedUserExtractor(actor): oxy_auth::extractor::AuthenticatedUserExtractor,
    axum::extract::Path(org_id): axum::extract::Path<Uuid>,
) -> Result<Json<audit::ChainReport>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("admin/audit: DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Scope. Found by the merge-derived coverage test on its first run — a fifth
    // `{org_id}` router nobody had swept. Verifying another tenant's hash chain is a
    // read of that tenant's audit trail.
    crate::server::api::admin::scope::deny_out_of_scope(&db, &actor, org_id).await?;
    audit::verify_chain(&db, org_id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("admin/audit: chain verification failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
pub struct AuditQuery {
    pub action: Option<String>,
    pub actor: Option<String>,
    pub org_id: Option<Uuid>,
    pub outcome: Option<String>,
    /// Free-text search across action / actor / target label.
    pub q: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize)]
pub struct AuditEventDto {
    pub id: Uuid,
    pub created_at: String,
    pub actor_email: String,
    pub actor_type: String,
    pub action: String,
    pub org_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub partner_id: Option<Uuid>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub target_label: Option<String>,
    pub outcome: String,
    pub reason: Option<String>,
    /// Staff/org action taken through the assume-role override — the single most
    /// important thing to spot when auditing, so it's lifted out of `metadata`.
    pub via_global_override: bool,
}

impl From<entity::audit_events::Model> for AuditEventDto {
    fn from(e: entity::audit_events::Model) -> Self {
        let via_global_override = e
            .metadata
            .get("via_global_override")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Self {
            id: e.id,
            created_at: e.created_at.to_rfc3339(),
            actor_email: e.actor_email,
            actor_type: e.actor_type,
            action: e.action,
            org_id: e.org_id,
            workspace_id: e.workspace_id,
            partner_id: e.partner_id,
            target_type: e.target_type,
            target_id: e.target_id,
            target_label: e.target_label,
            outcome: e.outcome,
            reason: e.reason,
            via_global_override,
        }
    }
}

pub async fn list_audit(
    Query(q): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEventDto>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("admin/audit: DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let filter = AuditFilter {
        action: q.action,
        actor: q.actor,
        org_id: q.org_id,
        outcome: q.outcome,
        q: q.q,
    };
    let limit = q.limit.unwrap_or(100).min(500);
    let offset = q.offset.unwrap_or(0);
    let events = audit::search_events(&db, &filter, limit, offset)
        .await
        .map_err(|e| {
            tracing::error!("admin/audit: search failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(events.into_iter().map(Into::into).collect()))
}
