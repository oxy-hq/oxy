//! `GET /api/customer-apps/oxy-access` — list every workspace that has
//! granted Oxy access, with its org + grant metadata.
//!
//! Powers the admin console's Orgs / Projects browser. App-admins may only
//! operate on workspaces whose org opted in (a `workspace_oxy_access` row),
//! so this is the canonical "what may we touch" list. Gated by the
//! `/customer-apps` nest's `oxy_app_admin_guard` (same population that uses
//! the customer-apps admin surface).

use std::collections::HashMap;

use axum::Json;
use axum::http::StatusCode;
use entity::prelude::{Organizations, Users, WorkspaceOxyAccess, Workspaces};
use entity::{organizations, users, workspace_oxy_access, workspaces};
use oxy::database::client::establish_connection;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Serialize;
use uuid::Uuid;

/// One Oxy-access grant, flattened with the workspace + org it belongs to
/// for direct rendering in the admin browser.
#[derive(Debug, Serialize)]
pub struct OxyAccessGrant {
    pub workspace_id: Uuid,
    pub workspace_name: String,
    pub org_id: Uuid,
    pub org_name: String,
    pub org_slug: String,
    /// Email of the org owner who granted access, if still resolvable.
    pub granted_by_email: Option<String>,
    pub granted_at: String,
}

pub async fn list_grants() -> Result<Json<Vec<OxyAccessGrant>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("oxy-access list: DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let grants = WorkspaceOxyAccess::find().all(&db).await.map_err(db_err)?;
    if grants.is_empty() {
        return Ok(Json(vec![]));
    }

    // Resolve the related rows in three batched lookups (grant counts are
    // small — one row per opted-in workspace).
    let ws_ids = grants.iter().map(|g| g.workspace_id).collect::<Vec<_>>();
    let ws_map = Workspaces::find()
        .filter(workspaces::Column::Id.is_in(ws_ids))
        .all(&db)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|w| (w.id, w))
        .collect::<HashMap<_, _>>();

    let org_ids = ws_map.values().filter_map(|w| w.org_id).collect::<Vec<_>>();
    let org_map = Organizations::find()
        .filter(organizations::Column::Id.is_in(org_ids))
        .all(&db)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|o| (o.id, o))
        .collect::<HashMap<_, _>>();

    let user_ids = grants
        .iter()
        .filter_map(|g| g.granted_by)
        .collect::<Vec<_>>();
    let email_map = Users::find()
        .filter(users::Column::Id.is_in(user_ids))
        .all(&db)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|u| (u.id, u.email))
        .collect::<HashMap<_, _>>();

    let mut out: Vec<OxyAccessGrant> = grants
        .into_iter()
        .filter_map(|g| build_grant(g, &ws_map, &org_map, &email_map))
        .collect();
    // Stable order for the browser: org, then workspace.
    out.sort_by(|a, b| {
        a.org_name
            .cmp(&b.org_name)
            .then_with(|| a.workspace_name.cmp(&b.workspace_name))
    });
    Ok(Json(out))
}

/// Assemble one row, dropping grants whose workspace or org no longer
/// resolves (orphaned toggle after a delete) — those aren't actionable.
fn build_grant(
    grant: workspace_oxy_access::Model,
    ws_map: &HashMap<Uuid, workspaces::Model>,
    org_map: &HashMap<Uuid, organizations::Model>,
    email_map: &HashMap<Uuid, String>,
) -> Option<OxyAccessGrant> {
    let ws = ws_map.get(&grant.workspace_id)?;
    let org = org_map.get(&ws.org_id?)?;
    Some(OxyAccessGrant {
        workspace_id: ws.id,
        workspace_name: ws.name.clone(),
        org_id: org.id,
        org_name: org.name.clone(),
        org_slug: org.slug.clone(),
        granted_by_email: grant.granted_by.and_then(|id| email_map.get(&id).cloned()),
        granted_at: grant.created_at.to_rfc3339(),
    })
}

fn db_err(e: sea_orm::DbErr) -> StatusCode {
    tracing::error!("oxy-access list query failed: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}
