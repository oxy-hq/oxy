//! Middleware used only by the local-mode router. Replaces `workspace_middleware`
//! for the single-tenant local server.
//!
//! Contract: the caller has already been authenticated by `auth_middleware`
//! running in guest-only mode, so `AuthenticatedUser` is already in extensions.
//! This middleware:
//!   1. Resolves the workspace directory via `resolve_local_workspace_path()`
//!      (walks up from CWD looking for config.yml).
//!   2. Fabricates an in-memory `workspaces::Model` at `LOCAL_WORKSPACE_ID`
//!      (Uuid::nil()). No DB read.
//!   3. Builds a `WorkspaceManager` from that path and attaches the full
//!      extension set: the `Model`, `EffectiveWorkspaceRole(Owner)`, and the
//!      `WorkspaceManager` itself.
//!
//! Local mode has no orgs, so no `OrgMembership` extension is inserted. Any
//! handler that calls `OrgMembershipExtractor` must not be mounted on the
//! local router.

use crate::server::api::middlewares::workspace_context::{EffectiveWorkspaceRole, PreaggCacheCtx};
use crate::server::router::AppState;
use crate::server::serve_mode::LOCAL_WORKSPACE_ID;
use crate::server::service::retrieval::EnumIndexManager;
use crate::server::service::secret_manager::SecretManagerService;
use agentic_semantic::refresh_key_cache::RefreshKeyCache;
use axum::extract::State;
use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use entity::workspace_members::WorkspaceRole;
use entity::workspaces::{Model as WorkspaceModel, WorkspaceStatus};
use oxy::adapters::runs::RunsManager;
use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::effective_workspace_path;
use oxy::config::resolve_local_workspace_path;
use uuid::Uuid;

pub async fn local_context_middleware(
    State(app_state): State<AppState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let resolved_path = resolve_local_workspace_path().ok();

    let now = Utc::now().into();
    let (status, path_str) = match &resolved_path {
        Some(p) => (
            WorkspaceStatus::Ready,
            Some(p.to_string_lossy().into_owned()),
        ),
        None => {
            tracing::debug!(
                "local mode: no config.yml resolvable; continuing without WorkspaceManager"
            );
            (WorkspaceStatus::Failed, None)
        }
    };

    let workspace = WorkspaceModel {
        id: LOCAL_WORKSPACE_ID,
        name: "local".to_string(),
        git_namespace_id: None,
        git_remote_url: None,
        created_at: now,
        updated_at: now,
        path: path_str,
        last_opened_at: None,
        created_by: None,
        org_id: None,
        status,
        error: None,
    };

    request.extensions_mut().insert(workspace.clone());
    request
        .extensions_mut()
        .insert(EffectiveWorkspaceRole(WorkspaceRole::Owner));

    let user_id = local_user_id(&request);
    if resolved_path.is_some() {
        attach_workspace_manager(
            &mut request,
            &workspace,
            user_id,
            app_state.preagg_cache.clone(),
            app_state.preagg_renewal_threshold_secs,
        )
        .await?;
    }
    // Always expose the preagg cache + threshold to handlers via a typed
    // extension so endpoints like POST /semantic can resolve preagg without
    // routing through OxyProjectContext. Mirrors the cloud-mode middleware.
    request.extensions_mut().insert(PreaggCacheCtx {
        cache: app_state.preagg_cache,
        renewal_threshold_secs: app_state.preagg_renewal_threshold_secs,
    });
    Ok(next.run(request).await)
}

/// Read the local-mode authenticated user's id from request extensions.
/// `auth_middleware` runs before this middleware in guest-only mode and
/// inserts an [`oxy_auth::types::AuthenticatedUser`] for the local seed user;
/// returns `None` if it didn't (e.g. a never-authenticated request path).
fn local_user_id(request: &Request<axum::body::Body>) -> Option<Uuid> {
    request
        .extensions()
        .get::<oxy_auth::types::AuthenticatedUser>()
        .map(|u| u.id)
}

/// Builds the `WorkspaceManager` and inserts it into request extensions.
/// Best-effort: failures are logged and the request continues without the
/// manager (matches the behavior of `workspace_context::try_attach_workspace_manager`).
async fn attach_workspace_manager(
    request: &mut Request<axum::body::Body>,
    workspace_row: &WorkspaceModel,
    user_id: Option<Uuid>,
    preagg_cache: Option<std::sync::Arc<std::sync::RwLock<RefreshKeyCache>>>,
    preagg_renewal_threshold_secs: Option<u64>,
) -> Result<(), StatusCode> {
    let effective_path = effective_workspace_path(workspace_row, None)
        .await
        .map_err(|e| {
            tracing::error!(
                "local_context: effective_workspace_path failed on fabricated workspace: {}",
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut builder = match WorkspaceBuilder::new(LOCAL_WORKSPACE_ID)
        .with_workspace_path_and_fallback_config(&effective_path)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                "local_context: builder init failed: {}, continuing without manager",
                e
            );
            return Ok(());
        }
    };

    match SecretsManager::from_database_with_env_fallback(SecretManagerService::new(
        LOCAL_WORKSPACE_ID,
    )) {
        Ok(secrets_manager) => builder = builder.with_secrets_manager(secrets_manager),
        Err(_) => {
            tracing::warn!("local_context: failed to create secrets manager, continuing without it")
        }
    }

    // Local mode has no branch concept; use nil UUID as the conventional branch_id sentinel.
    match RunsManager::default(LOCAL_WORKSPACE_ID, Uuid::nil()).await {
        Ok(runs_manager) => builder = builder.with_runs_manager(runs_manager),
        Err(e) => tracing::warn!(
            "local_context: failed to create runs manager: {}, continuing without it",
            e
        ),
    }

    builder = builder.try_with_intent_classifier().await;

    let workspace_manager = match builder.build().await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                "local_context: failed to build workspace manager: {}, continuing",
                e
            );
            return Ok(());
        }
    };

    match EnumIndexManager::init_from_config(workspace_manager.config_manager.clone()).await {
        Ok(_) => tracing::debug!("local_context: enum index initialized successfully"),
        Err(e) => tracing::debug!("local_context: enum index initialization skipped: {}", e),
    }

    let mut ctx = crate::agentic_wiring::OxyProjectContext::new(workspace_manager.clone());
    if let Some(uid) = user_id {
        ctx = ctx.with_subject(uid);
    }
    // Local mode is a single-user deployment where the seeded guest is
    // unconditionally the workspace Owner — there is no UI for switching
    // roles in local mode. Setting `Owner` here lets `airhouse_managed`
    // mints carry `admin` role for local queries.
    ctx = ctx.with_role(WorkspaceRole::Owner);
    if let Some(cache) = preagg_cache {
        ctx = ctx.with_preagg_cache(cache);
    }
    if let Some(secs) = preagg_renewal_threshold_secs {
        ctx = ctx.with_preagg_renewal_threshold_secs(secs);
    }
    let project_ctx = std::sync::Arc::new(ctx);
    let platform: std::sync::Arc<dyn agentic_pipeline::platform::PlatformContext> =
        project_ctx.clone();
    let bridges = crate::agentic_wiring::build_builder_bridges(project_ctx.clone());
    request.extensions_mut().insert(workspace_manager);
    request.extensions_mut().insert(platform);
    request.extensions_mut().insert(project_ctx);
    request.extensions_mut().insert(bridges);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::{Router, body::Body};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    /// The middleware must not 500 when no config.yml is resolvable — it
    /// must still run the handler with a fabricated workspace Model that
    /// has `path: None` and `status: Failed`.
    #[tokio::test]
    async fn tolerates_missing_config_and_inserts_model_with_no_path() {
        // Point CWD at a directory with no config.yml and no ancestors with one.
        // Tests run in the same process; no other test in this file depends on CWD.
        let tmp = tempfile::TempDir::new().expect("tempdir");
        std::env::set_current_dir(tmp.path()).expect("set cwd");

        let captured: Arc<Mutex<Option<WorkspaceModel>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let app_state = AppState {
            enterprise: false,
            internal: false,
            mode: crate::server::serve_mode::ServeMode::Local,
            observability: None,
            startup_cwd: std::path::PathBuf::new(),
            preagg_cache: None,
            preagg_renewal_threshold_secs: None,
        };

        let app = Router::new()
            .route(
                "/probe",
                get(move |request: Request<Body>| {
                    let cap = captured_clone.clone();
                    async move {
                        let ws = request
                            .extensions()
                            .get::<WorkspaceModel>()
                            .cloned()
                            .expect("workspace model in extensions");
                        *cap.lock().unwrap() = Some(ws);
                        axum::http::StatusCode::OK.into_response()
                    }
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                app_state,
                local_context_middleware,
            ));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/probe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot");
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let ws = captured.lock().unwrap().clone().expect("model captured");
        assert_eq!(ws.id, LOCAL_WORKSPACE_ID);
        assert!(
            ws.path.is_none(),
            "fabricated model must have path: None when no config.yml"
        );
        assert_eq!(ws.status, WorkspaceStatus::Failed);
    }
}
