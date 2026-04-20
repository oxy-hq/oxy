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

use crate::server::api::middlewares::workspace_context::EffectiveWorkspaceRole;
use crate::server::serve_mode::LOCAL_WORKSPACE_ID;
use crate::server::service::retrieval::EnumIndexManager;
use crate::server::service::secret_manager::SecretManagerService;
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
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = resolve_local_workspace_path().map_err(|e| {
        tracing::error!(
            "local mode: could not resolve workspace path (no config.yml found): {}",
            e
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let now = Utc::now().into();
    let workspace = WorkspaceModel {
        id: LOCAL_WORKSPACE_ID,
        name: "local".to_string(),
        git_namespace_id: None,
        git_remote_url: None,
        created_at: now,
        updated_at: now,
        path: Some(path.to_string_lossy().into_owned()),
        last_opened_at: None,
        created_by: None,
        org_id: None,
        status: WorkspaceStatus::Ready,
        error: None,
    };

    request.extensions_mut().insert(workspace.clone());
    request
        .extensions_mut()
        .insert(EffectiveWorkspaceRole(WorkspaceRole::Owner));

    attach_workspace_manager(&mut request, &workspace).await?;
    Ok(next.run(request).await)
}

/// Builds the `WorkspaceManager` and inserts it into request extensions.
/// Best-effort: failures are logged and the request continues without the
/// manager (matches the behavior of `workspace_context::try_attach_workspace_manager`).
async fn attach_workspace_manager(
    request: &mut Request<axum::body::Body>,
    workspace_row: &WorkspaceModel,
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

    request.extensions_mut().insert(workspace_manager);
    Ok(())
}
