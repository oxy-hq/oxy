//! What a build declares it needs — the `env` block of its `oxy-app.json` and
//! every function's `webhook.secretVar` — read for the secrets view.
//!
//! Two ways to name the build: the one an environment serves (the view's
//! default, [`for_environment`]), or one the caller names by `app_builds.id`
//! ([`for_requested_build`]) — "what would this environment be missing if this
//! build went live here", which the console asks before a promote or a Make
//! live. The second is the same reconciliation over a different build, so a
//! warning before the move and the view after it can never disagree.

use std::collections::BTreeMap;

use axum::http::StatusCode;
use entity::{app_builds, app_functions, apps};
use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use super::{Failure, declared};

/// What the build declares, plus why the answer might be incomplete.
///
/// A struct rather than a tuple because the second field is the interesting
/// one: every path that yields *no* declarations has to say whether that means
/// "this app declares nothing" or "we could not find out".
pub(super) struct Declarations {
    pub keys: BTreeMap<String, declared::DeclaredKey>,
    /// Non-`None` when the list is not the whole truth — a malformed `env`
    /// block, or a query that failed.
    pub error: Option<String>,
    pub build_id: Option<String>,
    /// The declaring build's manifest, for the `shared` overlay.
    pub manifest: Option<serde_json::Value>,
}

impl Declarations {
    /// The app has no build at all. Genuinely declares nothing — not an error.
    fn none() -> Self {
        Self {
            keys: Default::default(),
            error: None,
            build_id: None,
            manifest: None,
        }
    }

    /// We could not read the declarations. Distinct from `none()` on purpose:
    /// silently returning zero keys is the exact failure this module argues
    /// against for a malformed manifest — every stored key would flip to
    /// **Undeclared**, `missing_required` would fall to 0, and the header badge
    /// would vanish, all reading as a healthy app.
    fn unreadable(reason: &str) -> Self {
        Self {
            keys: Default::default(),
            error: Some(format!(
                "Could not read this app's declared keys ({reason}), so only what is \
                 already stored is listed below."
            )),
            build_id: None,
            manifest: None,
        }
    }
}

/// The build whose declarations are in force in `environment`: for
/// production, production's, else staging's for an app never promoted; for
/// staging, staging's. Matches how `/fn` and the storage sweeper pick a
/// manifest — the declarations have to come from the same build as the code
/// that reads them — so it asks the same resolver
/// (`custom_apps_env_resolve::resolve_function_environment`).
async fn declaring_build(
    db: &DatabaseConnection,
    app: &apps::Model,
    environment: &AppEnvironment,
) -> Result<Option<Uuid>, String> {
    crate::custom_apps_env_resolve::resolve_function_environment(db, app, environment)
        .await
        .map(|resolved| resolved.build_id)
        .map_err(|e| e.to_string())
}

/// What the build `environment` serves declares.
pub(super) async fn for_environment(
    db: &DatabaseConnection,
    app: &apps::Model,
    environment: &AppEnvironment,
) -> Declarations {
    let build_pk = match declaring_build(db, app, environment).await {
        Ok(Some(build_pk)) => build_pk,
        Ok(None) => return Declarations::none(),
        Err(e) => {
            tracing::error!(app_id = %app.id, "environment lookup failed: {e}");
            return Declarations::unreadable("its environments could not be read");
        }
    };
    let build = match app_builds::Entity::find_by_id(build_pk).one(db).await {
        // A build row that has gone away declares nothing, which is true rather
        // than unknown — only the query FAILING is unknown.
        Ok(row) => row,
        Err(e) => {
            tracing::error!(app_id = %app.id, %build_pk, "build lookup failed: {e}");
            return Declarations::unreadable("its build could not be loaded");
        }
    };
    of_build(db, app, build_pk, build).await
}

/// What the build `build_pk` of this app declares — named by the caller rather
/// than resolved from an environment. A 404 for a build that does not exist or
/// is another app's: the staff mount reaches any app, but one app's view never
/// reconciles against another app's manifest.
pub(super) async fn for_requested_build(
    db: &DatabaseConnection,
    app: &apps::Model,
    build_pk: Uuid,
) -> Result<Declarations, Failure> {
    let build = app_builds::Entity::find_by_id(build_pk)
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!(app_id = %app.id, %build_pk, "build lookup failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "build lookup failed".to_string(),
            )
        })?;
    owned_by(app.id, build.as_ref().map(|b| b.app_id))?;
    Ok(of_build(db, app, build_pk, build).await)
}

/// A requested build must exist and be `app_id`'s.
pub(super) fn owned_by(app_id: Uuid, build_app_id: Option<Uuid>) -> Result<(), Failure> {
    match build_app_id {
        Some(owner) if owner == app_id => Ok(()),
        _ => Err((StatusCode::NOT_FOUND, "build not found".to_string())),
    }
}

async fn of_build(
    db: &DatabaseConnection,
    app: &apps::Model,
    build_pk: Uuid,
    build: Option<app_builds::Model>,
) -> Declarations {
    let (env, manifest_error) = declared::declared_env(
        build.as_ref().and_then(|b| b.manifest_json.as_ref()),
        app.id,
    );

    // Every function in the same build, for their `webhook.secretVar`s.
    let fn_manifests: Vec<serde_json::Value> = match app_functions::Entity::find()
        .filter(app_functions::Column::BuildId.eq(build_pk))
        .all(db)
        .await
    {
        Ok(rows) => rows.into_iter().filter_map(|r| r.manifest_json).collect(),
        Err(e) => {
            tracing::error!(app_id = %app.id, "function lookup failed: {e}");
            return Declarations::unreadable("its functions could not be loaded");
        }
    };

    Declarations {
        keys: declared::merge_declared(env, declared::webhook_secret_vars(fn_manifests.iter())),
        error: manifest_error,
        manifest: build.as_ref().and_then(|b| b.manifest_json.clone()),
        build_id: build.map(|b| b.build_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A build named on the list route must be this app's: another app's, or
    /// one that does not exist, is not found — never reconciled against.
    #[test]
    fn a_requested_build_is_this_apps_or_not_found() {
        let (app, other) = (Uuid::from_u128(1), Uuid::from_u128(2));
        assert!(owned_by(app, Some(app)).is_ok());
        for owner in [Some(other), None] {
            let (code, _) = owned_by(app, owner).unwrap_err();
            assert_eq!(code, StatusCode::NOT_FOUND, "{owner:?}");
        }
    }
}
