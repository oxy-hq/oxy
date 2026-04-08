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
use oxy::adapters::runs::RunsManager;
use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::config::resolve_local_workspace_path;
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_project::LocalGitService;
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

#[derive(serde::Deserialize)]
pub struct WorkspacePath {
    pub workspace_id: Uuid,
}

#[derive(serde::Deserialize)]
pub struct BranchQuery {
    pub branch: Option<String>,
}

pub async fn workspace_middleware(
    State(app_state): State<AppState>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    Query(query): Query<BranchQuery>,
    _user: AuthenticatedUserExtractor,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let branch_id = Uuid::nil();

    // Resolve the workspace path:
    // - Non-nil UUID: look up workspace in DB by ID and use its stored path.
    // - Nil UUID (legacy / bootstrap): fall back to active_workspace_path or CWD discovery.
    let resolved_path: Option<std::path::PathBuf> = if workspace_id != Uuid::nil() {
        if let Ok(db) = establish_connection().await {
            use entity::prelude::Workspaces;
            if let Ok(Some(workspace_row)) = Workspaces::find_by_id(workspace_id).one(&db).await {
                let model = entity::workspaces::Model {
                    id: workspace_row.id,
                    name: workspace_row.name.clone(),
                    workspace_id: workspace_row.workspace_id,
                    project_repo_id: workspace_row.project_repo_id,
                    active_branch_id: workspace_row.active_branch_id,
                    created_at: workspace_row.created_at,
                    updated_at: workspace_row.updated_at,
                    path: workspace_row.path.clone(),
                    last_opened_at: workspace_row.last_opened_at,
                    created_by: workspace_row.created_by,
                };
                request.extensions_mut().insert(model);
                workspace_row.path.map(std::path::PathBuf::from)
            } else {
                tracing::warn!("Workspace {} not found in DB", workspace_id);
                None
            }
        } else {
            tracing::warn!(
                "Could not connect to DB to resolve workspace {}",
                workspace_id
            );
            None
        }
    } else {
        // Bootstrap / single-workspace mode: use active_workspace_path or CWD.
        let fake_workspace = entity::workspaces::Model {
            id: Uuid::nil(),
            name: "Oxy".to_string(),
            workspace_id: Uuid::nil(),
            project_repo_id: None,
            active_branch_id: Uuid::nil(),
            created_at: chrono::Utc::now().into(),
            updated_at: chrono::Utc::now().into(),
            path: None,
            last_opened_at: None,
            created_by: None,
        };
        request.extensions_mut().insert(fake_workspace);

        {
            let locked = app_state.active_workspace_path.read().await;
            locked.clone()
        }
        .or_else(|| resolve_local_workspace_path().ok())
    };

    match resolved_path {
        Some(workspace_path) => {
            // Use the worktree path for non-main branches when one exists,
            // so that workflow/app/agent execution reads from the correct branch.
            let effective_path = query
                .branch
                .as_deref()
                .filter(|b| !b.is_empty() && *b != "main")
                .and_then(|b| LocalGitService::get_worktree_path(&workspace_path, b))
                .unwrap_or(workspace_path);
            match WorkspaceBuilder::new(workspace_id)
                .with_workspace_path_and_fallback_config(&effective_path)
                .await
            {
                Ok(mut builder) => {
                    // Use DB-first with env fallback so DB secrets hot-reload
                    // and override env vars without a server restart.
                    let sm_result = SecretsManager::from_database_with_env_fallback(
                        SecretManagerService::new(workspace_id),
                    );
                    if let Ok(secrets_manager) = sm_result {
                        builder = builder.with_secrets_manager(secrets_manager);
                    } else {
                        tracing::warn!(
                            "Failed to create secrets manager for workspace {}, continuing without it",
                            workspace_id
                        );
                    }
                    match RunsManager::default(workspace_id, branch_id).await {
                        Ok(runs_manager) => {
                            builder = builder.with_runs_manager(runs_manager);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to create runs manager for workspace {}: {}, continuing without it",
                                workspace_id,
                                e
                            );
                        }
                    }
                    builder = builder.try_with_intent_classifier().await;
                    match builder.build().await {
                        Ok(workspace_manager) => {
                            match EnumIndexManager::init_from_config(
                                workspace_manager.config_manager.clone(),
                            )
                            .await
                            {
                                Ok(_) => {
                                    tracing::debug!(
                                        "Enum index initialized successfully for workspace {}",
                                        workspace_id
                                    );
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        "Enum index initialization skipped for workspace {}: {}",
                                        workspace_id,
                                        e
                                    );
                                }
                            }
                            request.extensions_mut().insert(workspace_manager);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to build workspace manager for workspace {}: {}, continuing without it",
                                workspace_id,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to set workspace path in workspace builder for workspace {}: {}, continuing without workspace manager",
                        workspace_id,
                        e
                    );
                }
            }
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
