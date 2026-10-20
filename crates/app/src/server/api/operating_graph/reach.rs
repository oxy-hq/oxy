//! Reach lives in `oxy_app_core::operating_graph::reach`; re-exported at the old
//! path. Only [`reach_for_viewer`] stays here: it resolves the viewer's app role
//! through `custom_apps_auth`, which sits in `oxy-custom-apps`, above `oxy-app-core`.

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

pub use oxy_app_core::operating_graph::reach::*;

/// A signed-in viewer's reach on a customer-app surface: the same rule as
/// `ctx.user.reach`, with app-admin standing resolved only when the surface
/// names its app. Every lookup fails closed to nowhere — a blip must not
/// widen what a screen shows or a query returns.
pub async fn reach_for_viewer(
    db: &DatabaseConnection,
    org_id: Uuid,
    user: &oxy_auth::types::AuthenticatedUser,
    project_id: Uuid,
    app_id: Option<Uuid>,
) -> Reach {
    // Three independent facts, read together: this sits on the app-bootstrap
    // path, which is per-viewer and uncached, so a serial chain here is paid
    // on every app load.
    let (app_admin, member, held) = tokio::join!(
        async {
            let Some(id) = app_id else { return false };
            let app = entity::apps::Entity::find_by_id(id)
                .filter(entity::apps::Column::ProjectId.eq(project_id))
                .one(db)
                .await
                .ok()
                .flatten();
            match app {
                Some(app) => {
                    // The user WITH their credential: an API token's grants and
                    // standing flags decide app-admin here as everywhere else.
                    crate::server::api::custom_apps_auth::resolve_app_role(
                        db,
                        &oxy_server_authz::Caller::from_user(user),
                        &app,
                    )
                    .await
                    .ok()
                    .flatten()
                        == Some("admin")
                }
                None => false,
            }
        },
        async {
            crate::server::api::custom_apps_auth::resolve_org_role(db, user.id, org_id)
                .await
                .map(|role| role.is_some())
                .unwrap_or(false)
        },
        held_by(db, org_id, user.id),
    );
    match held {
        Ok(held) => reach_of(
            Caller {
                system: false,
                app_admin,
                member,
            },
            &held,
        ),
        Err(e) => {
            tracing::error!("reach lookup failed: {e}");
            Reach::nowhere()
        }
    }
}
