use crate::adapters::project::builder::ProjectBuilder;
use crate::adapters::project::manager::ProjectManager;
use crate::adapters::runs::RunsManager;
use crate::adapters::secrets::SecretsManager;
use crate::auth::extractor::AuthenticatedUserExtractor;
use crate::db::client::establish_connection;
use crate::github::GitOperations;
use crate::service::retrieval::EnumIndexManager;
use crate::service::secret_manager::SecretManagerService;
use axum::extract::Query;
use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use entity::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::future::Future;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProjectManagerExtractor(pub ProjectManager);

impl<S> FromRequestParts<S> for ProjectManagerExtractor
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
            .get::<ProjectManager>()
            .cloned()
            .map(ProjectManagerExtractor)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);

        async move { result }
    }
}

#[derive(serde::Deserialize)]
pub struct ProjectPath {
    pub project_id: Uuid,
}

#[derive(serde::Deserialize)]
pub struct BranchQuery {
    pub branch: Option<String>,
}

const SKIP_PROJECT_MANAGER_ROUTES: &[&str] = &["/details", "/branches"];

fn should_skip_project_manager(uri_path: &str) -> bool {
    SKIP_PROJECT_MANAGER_ROUTES
        .iter()
        .any(|route| uri_path.starts_with(route))
}

pub async fn project_middleware(
    Path(ProjectPath { project_id }): Path<ProjectPath>,
    Query(query): Query<BranchQuery>,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    println!("Project ID from path: {}", project_id);
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("Failed to establish database connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let project = Projects::find_by_id(project_id)
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Database error when fetching project: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::warn!("Project {} not found", project_id);
            StatusCode::NOT_FOUND
        })?;

    let has_access = OrganizationUsers::find()
        .filter(entity::organization_users::Column::OrganizationId.eq(project.organization_id))
        .filter(entity::organization_users::Column::UserId.eq(user.id))
        .one(&db)
        .await
        .map_err(|e| {
            tracing::error!("Database error when checking organization access: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .is_some();

    if !has_access {
        tracing::warn!(
            "User {} does not have access to organization {} for project {}",
            user.id,
            project.organization_id,
            project_id
        );
        return Err(StatusCode::FORBIDDEN);
    }

    let active_branch_id = project.active_branch_id;
    let branch_id = if let Some(branch_name) = query.branch {
        if branch_name.trim().is_empty() {
            active_branch_id
        } else {
            let branch = Branches::find()
                .filter(entity::branches::Column::ProjectId.eq(project_id))
                .filter(entity::branches::Column::Name.eq(branch_name.clone()))
                .one(&db)
                .await
                .map_err(|e| {
                    tracing::error!("Database error when fetching branch: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                .ok_or_else(|| {
                    tracing::warn!("Branch {} not found", branch_name);
                    StatusCode::NOT_FOUND
                })?;
            branch.id
        }
    } else {
        active_branch_id
    };

    request.extensions_mut().insert(project);

    print!("Request URI path: {}", request.uri().path());
    let skip_project_manager = should_skip_project_manager(request.uri().path());

    if skip_project_manager {
        tracing::debug!(
            "Skipping project manager creation for route: {}",
            request.uri().path()
        );
        println!(
            "Skipping project manager creation for route: {}",
            request.uri().path()
        );
        return Ok(next.run(request).await);
    }

    match GitOperations::get_repository_path(project_id, branch_id) {
        Ok(project_path) => match ProjectBuilder::new()
            .with_project_path_and_fallback_config(&project_path)
            .await
        {
            Ok(mut builder) => {
                if let Ok(secrets_manager) =
                    SecretsManager::from_database(SecretManagerService::new(project_id))
                {
                    builder = builder.with_secrets_manager(secrets_manager);
                } else {
                    tracing::warn!(
                        "Failed to create secrets manager for project {}, continuing without it",
                        project_id
                    );
                }
                match RunsManager::default(project_id, branch_id).await {
                    Ok(runs_manager) => {
                        builder = builder.with_runs_manager(runs_manager);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create runs manager for project {}: {}, continuing without it",
                            project_id,
                            e
                        );
                    }
                }
                match builder.build().await {
                    Ok(project_manager) => {
                        match EnumIndexManager::init_from_config(
                            project_manager.config_manager.clone(),
                        )
                        .await
                        {
                            Ok(_) => {
                                tracing::debug!(
                                    "Enum index initialized successfully for project {}",
                                    project_id
                                );
                            }
                            Err(e) => {
                                tracing::debug!(
                                    "Enum index initialization skipped for project {}: {}",
                                    project_id,
                                    e
                                );
                            }
                        }
                        request.extensions_mut().insert(project_manager);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to build project manager for project {}: {}, continuing without it",
                            project_id,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to set project path in project builder for project {}: {}, continuing without project manager",
                    project_id,
                    e
                );
            }
        },
        Err(e) => {
            tracing::warn!(
                "Failed to get repository path for project {}: {}, continuing without project manager",
                project_id,
                e
            );
        }
    }

    Ok(next.run(request).await)
}
