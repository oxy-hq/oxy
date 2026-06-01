//! `GET /api/{workspace_id}/custom-apps` — published customer apps
//! for the current workspace.
//!
//! Powers the workspace sidebar's "Custom Apps" section. Mounted
//! inside the workspace router so the workspace_middleware handles
//! org-membership authorization — anyone who can see the workspace
//! can list its published custom apps.
//!
//! Drafts are intentionally absent. Oxy staff iterating on an
//! unpublished app reach it via `/admin/apps`, not the customer
//! sidebar.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use entity::organizations;
use entity::prelude::{Apps, Organizations};
use entity::{apps, apps::Model as AppsModel};
use oxy::database::client::establish_connection;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::admin::apps::handlers::build_pretty_url;

#[derive(Deserialize)]
pub struct WorkspaceIdPath {
    pub workspace_id: Uuid,
}

#[derive(Serialize)]
pub struct CustomAppSummary {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub org_slug: String,
    /// Canonical URL the sidebar links to. Same-tab navigation lands
    /// the user on the bespoke app surface (no embed, no workspace
    /// chrome — these apps own their own UX).
    pub url: String,
    pub published_at: String,
}

impl CustomAppSummary {
    fn from_model(m: AppsModel, org_slug: &str, published_at: String) -> Self {
        let url = build_pretty_url(org_slug, &m.slug);
        Self {
            id: m.id,
            slug: m.slug,
            name: m.name,
            org_slug: org_slug.to_string(),
            url,
            published_at,
        }
    }
}

pub async fn list_custom_apps(
    Path(WorkspaceIdPath { workspace_id }): Path<WorkspaceIdPath>,
) -> Result<Json<Vec<CustomAppSummary>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("list_custom_apps DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let rows = Apps::find()
        .filter(apps::Column::ProjectId.eq(workspace_id))
        .filter(apps::Column::PublishedAt.is_not_null())
        .order_by_asc(apps::Column::Name)
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("list_custom_apps query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if rows.is_empty() {
        return Ok(Json(vec![]));
    }

    // Bulk-resolve org slugs in one query (apps for a workspace will
    // usually share one org, but the field is denormalised so we don't
    // assume).
    use std::collections::{HashMap, HashSet};
    let org_ids: HashSet<Uuid> = rows.iter().map(|a| a.org_id).collect();
    let orgs = Organizations::find()
        .filter(organizations::Column::Id.is_in(org_ids.iter().copied()))
        .all(&db)
        .await
        .map_err(|e| {
            tracing::error!("list_custom_apps org lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let slugs: HashMap<Uuid, String> = orgs.into_iter().map(|o| (o.id, o.slug)).collect();

    let out = rows
        .into_iter()
        .filter_map(|app| {
            let slug = slugs.get(&app.org_id)?.clone();
            // SAFETY-ish: we filtered on PublishedAt.is_not_null above.
            let published = app.published_at.map(|d| d.to_rfc3339())?;
            Some(CustomAppSummary::from_model(app, &slug, published))
        })
        .collect();
    Ok(Json(out))
}
