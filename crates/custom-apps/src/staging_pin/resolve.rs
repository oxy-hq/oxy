//! Resolve the pin a non-production data-plane request reads, once the
//! request has named its app and its environment.
//!
//! The request-reading half (`x-oxy-app`, `Referer`, `Host`, `X-Oxy-App-Env`)
//! needs an axum `HeaderMap` and stays in `oxy_app::server::api::
//! custom_apps_staging_pin::request`; it hands the result to
//! [`pin_for_environment`]. Everything here reads the database only.

use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

/// How a request named its app, before any database lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppRef {
    Id(Uuid),
    Slugs {
        org: String,
        app: String,
    },
    /// `/a/<slug>/` on an org subdomain: the org comes from the host.
    OrgHostSlug {
        org: String,
        app: String,
    },
}

/// The pin for a request that named `app` and the non-production
/// `environment`, or `None` for every other request. `project_id` is the
/// workspace the request is addressed to: an app published from another
/// workspace is no match.
///
/// The viewer must be allowed to open non-production
/// (`may_open_non_production`, the rule that serves them that environment's
/// HTML), and the pin is read from the build **that environment** serves. A
/// sandbox whose build pins nothing reads the promoted model; it never borrows
/// staging's pin.
pub async fn pin_for_environment(
    db: &DatabaseConnection,
    caller: &oxy_server_authz::Caller,
    project_id: Uuid,
    app: &AppRef,
    environment: &AppEnvironment,
) -> Option<Uuid> {
    let app = find_app(db, app).await?;
    if app.project_id != project_id {
        return None;
    }
    let build_id = environment_build(db, caller, &app, environment).await?;
    super::pinned_revision_for(db, build_id).await
}

/// The build `environment` — staging or a sandbox — serves, for a viewer who
/// may open the app's non-production environments.
async fn environment_build(
    db: &DatabaseConnection,
    caller: &oxy_server_authz::Caller,
    app: &entity::apps::Model,
    environment: &AppEnvironment,
) -> Option<Uuid> {
    use crate::env_resolve::resolve_environment;
    use crate::non_production::may_open_non_production;
    if !may_open_non_production(db, caller, app).await {
        return None;
    }
    resolve_environment(db, app, environment)
        .await
        .ok()?
        .build_id
}

pub async fn find_app(db: &DatabaseConnection, r: &AppRef) -> Option<entity::apps::Model> {
    let found = match r {
        AppRef::Id(id) => entity::apps::Entity::find_by_id(*id).one(db).await,
        AppRef::Slugs { org, app } | AppRef::OrgHostSlug { org, app } => {
            let Ok(Some(org)) = entity::organizations::Entity::find()
                .filter(entity::organizations::Column::Slug.eq(org.as_str()))
                .one(db)
                .await
            else {
                return None;
            };
            entity::apps::Entity::find()
                .filter(entity::apps::Column::OrgId.eq(org.id))
                .filter(entity::apps::Column::Slug.eq(app.as_str()))
                .one(db)
                .await
        }
    };
    found.ok().flatten()
}
