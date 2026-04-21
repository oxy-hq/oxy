use crate::server::router::AppState;
use crate::server::service::retrieval::EnumIndexManager;
use crate::server::service::secret_manager::SecretManagerService;
use axum::extract::{FromRequestParts, Path};
use axum::extract::{Query, State};
use axum::http::request::Parts;
use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use entity::workspace_members::WorkspaceRole;
use oxy::adapters::runs::RunsManager;
use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::effective_workspace_path;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::EntityTrait;
use std::future::Future;
use uuid::Uuid;

#[derive(Clone)]
pub struct WorkspaceManagerExtractor(pub WorkspaceManager);

pub struct WorkspaceManagerMissing;

impl IntoResponse for WorkspaceManagerMissing {
    fn into_response(self) -> Response {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "error": "Workspace configuration is not available. Check that the workspace path is accessible and config.yml is valid."
            })),
        )
            .into_response()
    }
}

impl<S> FromRequestParts<S> for WorkspaceManagerExtractor
where
    S: Send + Sync,
{
    type Rejection = WorkspaceManagerMissing;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<WorkspaceManager>()
            .cloned()
            .map(WorkspaceManagerExtractor)
            .ok_or(WorkspaceManagerMissing);

        async move { result }
    }
}

/// The resolved workspace role for the current user.
#[derive(Clone, Debug)]
pub struct EffectiveWorkspaceRole(pub WorkspaceRole);

impl<S> FromRequestParts<S> for EffectiveWorkspaceRole
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<EffectiveWorkspaceRole>()
            .cloned()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);

        async move { result }
    }
}

/// The caller's org membership, inserted by workspace_middleware when the workspace belongs to an org.
#[derive(Clone)]
pub struct OrgMembershipExtractor(pub entity::org_members::Model);

impl<S> FromRequestParts<S> for OrgMembershipExtractor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<entity::org_members::Model>()
            .cloned()
            .map(OrgMembershipExtractor)
            .ok_or(StatusCode::FORBIDDEN);

        async move { result }
    }
}

#[derive(serde::Deserialize)]
pub struct WorkspacePath {
    pub workspace_id: Uuid,
}

#[derive(serde::Deserialize)]
pub struct BranchQuery {
    pub branch: Option<String>,
}

pub async fn workspace_middleware(
    State(_app_state): State<AppState>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Query(query): Query<BranchQuery>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if workspace_id == Uuid::nil() {
        tracing::warn!("Nil-UUID workspace path is not allowed");
        return Err(StatusCode::NOT_FOUND);
    }

    let branch_id = Uuid::nil();

    match authorize_workspace(workspace_id, user.id, &mut request).await? {
        Some(workspace_row) => {
            try_attach_workspace_manager(
                &workspace_row,
                query.branch.as_deref(),
                workspace_id,
                branch_id,
                &mut request,
            )
            .await?;
        }
        None => {
            tracing::warn!(
                "No workspace path available for workspace {}, continuing without workspace manager",
                workspace_id
            );
        }
    }

    Ok(next.run(request).await)
}

/// Looks up the workspace, authorizes the caller, and inserts request extensions
/// (workspace row, effective role, org membership). Returns the workspace row
/// only when it has a configured path — i.e. when builder construction should follow.
///
/// `Ok(None)`: workspace has no configured path (builder construction skipped).
/// Fatal: DB unreachable (SERVICE_UNAVAILABLE), workspace not found (NOT_FOUND),
/// workspace with no `org_id` (FORBIDDEN), caller not in org (FORBIDDEN),
/// query errors (INTERNAL_SERVER_ERROR).
async fn authorize_workspace(
    workspace_id: Uuid,
    user_id: Uuid,
    request: &mut Request<axum::body::Body>,
) -> Result<Option<entity::workspaces::Model>, StatusCode> {
    use entity::prelude::Workspaces;

    let db = establish_connection().await.map_err(|e| {
        tracing::error!(
            "Could not connect to DB to resolve workspace {}: {}",
            workspace_id,
            e
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let workspace_row = Workspaces::find_by_id(workspace_id)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query workspace {}: {}", workspace_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("Workspace {} not found in DB", workspace_id);
            StatusCode::NOT_FOUND
        })?;

    // Every workspace must belong to an org.
    let org_id = workspace_row.org_id.ok_or_else(|| {
        tracing::warn!("Workspace {} has no org_id — access denied", workspace_id);
        StatusCode::FORBIDDEN
    })?;

    request.extensions_mut().insert(workspace_row.clone());

    let (org_membership, effective_role) =
        resolve_effective_role(&db, workspace_id, org_id, user_id).await?;

    request
        .extensions_mut()
        .insert(EffectiveWorkspaceRole(effective_role));
    request.extensions_mut().insert(org_membership);

    if workspace_row.path.is_none() {
        tracing::warn!(
            "Workspace {} has no path configured — continuing without workspace manager",
            workspace_id
        );
        return Ok(None);
    }

    Ok(Some(workspace_row))
}

async fn resolve_effective_role(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<(entity::org_members::Model, WorkspaceRole), StatusCode> {
    use entity::org_members::Column as OrgMemberCol;
    use entity::prelude::{OrgMembers, WorkspaceMembers};
    use entity::workspace_members::Column as WsMemberCol;
    use sea_orm::{ColumnTrait, QueryFilter};

    let org_membership = OrgMembers::find()
        .filter(OrgMemberCol::OrgId.eq(org_id))
        .filter(OrgMemberCol::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to query org membership (org={}, user={}, workspace={}): {}",
                org_id,
                user_id,
                workspace_id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!(
                "User {} denied access to workspace {} (not a member of org {})",
                user_id,
                workspace_id,
                org_id
            );
            StatusCode::FORBIDDEN
        })?;

    let ws_override = WorkspaceMembers::find()
        .filter(WsMemberCol::WorkspaceId.eq(workspace_id))
        .filter(WsMemberCol::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to query workspace member override (workspace={}, user={}): {}",
                workspace_id,
                user_id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let org_derived_role = match org_membership.role {
        entity::org_members::OrgRole::Owner => WorkspaceRole::Owner,
        entity::org_members::OrgRole::Admin => WorkspaceRole::Admin,
        entity::org_members::OrgRole::Member => WorkspaceRole::Member,
    };

    // Workspace-member override can only elevate, never downgrade below org-derived role.
    let effective_role = match ws_override {
        Some(ws_member) => std::cmp::max(org_derived_role, ws_member.role),
        None => org_derived_role,
    };

    Ok((org_membership, effective_role))
}

/// Best-effort: builds the `WorkspaceManager` (with secrets, runs, intent classifier)
/// and inserts it into request extensions. The only fatal outcome is an invalid
/// branch query parameter, which yields BAD_REQUEST.
async fn try_attach_workspace_manager(
    workspace_row: &entity::workspaces::Model,
    branch_name: Option<&str>,
    workspace_id: Uuid,
    branch_id: Uuid,
    request: &mut Request<axum::body::Body>,
) -> Result<(), StatusCode> {
    // Branch name is validated inside `effective_workspace_path`. The helper
    // rejects ".." / leading "-" / non-allowed chars via OxyError::RuntimeError —
    // we map that to 400 before the string reaches any shell-out downstream.
    let effective_path = effective_workspace_path(workspace_row, branch_name)
        .await
        .map_err(|e| {
            tracing::warn!(
                "Invalid branch or missing path for workspace {}: {}",
                workspace_id,
                e
            );
            StatusCode::BAD_REQUEST
        })?;

    let mut builder = match WorkspaceBuilder::new(workspace_id)
        .with_workspace_path_and_fallback_config(&effective_path)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                "Failed to set workspace path in workspace builder for workspace {}: {}, continuing without workspace manager",
                workspace_id,
                e
            );
            return Ok(());
        }
    };

    // DB-first with env fallback so DB secrets hot-reload and override env vars
    // without a server restart.
    match SecretsManager::from_database_with_env_fallback(SecretManagerService::new(workspace_id)) {
        Ok(secrets_manager) => builder = builder.with_secrets_manager(secrets_manager),
        Err(_) => tracing::warn!(
            "Failed to create secrets manager for workspace {}, continuing without it",
            workspace_id
        ),
    }

    match RunsManager::default(workspace_id, branch_id).await {
        Ok(runs_manager) => builder = builder.with_runs_manager(runs_manager),
        Err(e) => tracing::warn!(
            "Failed to create runs manager for workspace {}: {}, continuing without it",
            workspace_id,
            e
        ),
    }

    builder = builder.try_with_intent_classifier().await;

    let workspace_manager = match builder.build().await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                "Failed to build workspace manager for workspace {}: {}, continuing without it",
                workspace_id,
                e
            );
            return Ok(());
        }
    };

    match EnumIndexManager::init_from_config(workspace_manager.config_manager.clone()).await {
        Ok(_) => tracing::debug!(
            "Enum index initialized successfully for workspace {}",
            workspace_id
        ),
        Err(e) => tracing::debug!(
            "Enum index initialization skipped for workspace {}: {}",
            workspace_id,
            e
        ),
    }

    let project_ctx = std::sync::Arc::new(crate::agentic_wiring::OxyProjectContext::new(
        workspace_manager.clone(),
    ));
    let platform: std::sync::Arc<dyn agentic_pipeline::platform::PlatformContext> =
        project_ctx.clone();
    let bridges = crate::agentic_wiring::build_builder_bridges(project_ctx);
    request.extensions_mut().insert(workspace_manager);
    request.extensions_mut().insert(platform);
    request.extensions_mut().insert(bridges);
    Ok(())
}
