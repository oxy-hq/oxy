use axum::{Json, http::StatusCode};
use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::{resolve_workspace_path, workspace_root_path};
use oxy::config::ConfigBuilder;
use oxy::github::{GitHubClient, default_git_client, github_token_for_namespace};
use oxy::service::retrieval::{ReindexInput, reindex};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_git::GitClient;
use oxy_project::{copy_demo_files_to, write_minimal_config_yml};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::server::api::middlewares::org_context::{OrgContext, OrgContextExtractor};

fn require_org_admin(ctx: &OrgContext) -> Result<(), (StatusCode, String)> {
    use entity::org_members::OrgRole;
    match ctx.membership.role {
        OrgRole::Owner | OrgRole::Admin => Ok(()),
        OrgRole::Member => Err((
            StatusCode::FORBIDDEN,
            "Only org owners and admins can create workspaces".to_string(),
        )),
    }
}

/// Result returned by all three onboarding endpoints.
#[derive(Serialize)]
pub struct OnboardingResult {
    pub workspace_type: String,
    /// The UUID of the newly created workspace. The caller is responsible for
    /// activating it if desired (no auto-activation on the backend).
    pub workspace_id: Uuid,
}

/// Validate and normalise a user-supplied subdirectory path.
///
/// Returns `Err` if the path is absolute or contains any `..` / `.` components
/// (path-traversal protection). Returns `Ok(None)` for empty/whitespace input.
fn parse_subdir(raw: &str) -> Result<Option<std::path::PathBuf>, (StatusCode, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = std::path::Path::new(trimmed);
    if path.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Subdirectory must be a relative path".to_string(),
        ));
    }
    for component in path.components() {
        match component {
            std::path::Component::Normal(_) => {}
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Invalid subdirectory path: '{trimmed}'"),
                ));
            }
        }
    }
    Ok(Some(path.to_path_buf()))
}

/// Resolve the target workspace directory for an onboarding operation.
///
/// Returns `<state_dir>/workspaces/<workspace_id>`, creating it if needed.
/// Using the workspace UUID as the directory name guarantees uniqueness
/// without any name-collision logic.
fn resolve_project_dir(workspace_id: Uuid) -> Result<std::path::PathBuf, (StatusCode, String)> {
    let dir = workspace_root_path(workspace_id);
    std::fs::create_dir_all(&dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create workspace directory '{dir:?}': {e}"),
        )
    })?;
    Ok(dir)
}

/// Find a unique workspace display name by appending " 2", " 3", … when the base name is taken.
///
/// Uses a single `LIKE '{base}%'` query to fetch all matching names, then finds the first
/// gap in-process — avoiding up to 99 sequential round trips.
async fn unique_display_name(base: &str) -> Result<String, (StatusCode, String)> {
    use entity::{prelude::Workspaces, workspaces};
    use oxy::database::client::establish_connection;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    use std::collections::HashSet;

    let db = establish_connection().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database connection failed: {e}"),
        )
    })?;

    let taken: HashSet<String> = Workspaces::find()
        .filter(workspaces::Column::Name.like(format!("{base}%")))
        .all(&db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query workspaces: {e}"),
            )
        })?
        .into_iter()
        .map(|w| w.name)
        .collect();

    std::iter::once(base.to_string())
        .chain((2u32..=99).map(|i| format!("{base} {i}")))
        .find(|candidate| !taken.contains(candidate))
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Could not find a unique name for '{base}'"),
            )
        })
}

/// Register the workspace in the DB. Returns the workspace's UUID.
/// Does NOT activate the workspace — the caller decides when (and whether) to activate.
async fn register_project(
    project_dir: &std::path::Path,
    name: &str,
    workspace_id: Uuid,
    created_by: Option<Uuid>,
    org_id: Option<Uuid>,
    status: entity::workspaces::WorkspaceStatus,
    git_namespace_id: Option<Uuid>,
    git_remote_url: Option<String>,
) -> Result<Uuid, (StatusCode, String)> {
    use entity::workspaces;
    use oxy::database::client::establish_connection;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let path_str = project_dir.to_string_lossy().to_string();

    let db = establish_connection().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database connection failed: {e}"),
        )
    })?;

    use entity::prelude::Workspaces;

    // Return the existing workspace if the same path is already registered (idempotent).
    let existing = Workspaces::find()
        .filter(workspaces::Column::Path.eq(path_str.clone()))
        .one(&db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query workspaces: {e}"),
            )
        })?;

    if let Some(existing) = existing {
        return Ok(existing.id);
    }

    // Reject duplicate names — each workspace must have a unique display name.
    let name_taken = Workspaces::find()
        .filter(workspaces::Column::Name.eq(name))
        .one(&db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query workspaces: {e}"),
            )
        })?
        .is_some();

    if name_taken {
        return Err((
            StatusCode::CONFLICT,
            format!("A workspace named '{name}' already exists. Please choose a different name."),
        ));
    }

    let new_workspace = workspaces::ActiveModel {
        id: Set(workspace_id),
        name: Set(name.to_string()),
        git_namespace_id: Set(git_namespace_id),
        git_remote_url: Set(git_remote_url),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        path: Set(Some(path_str.clone())),
        last_opened_at: Set(None),
        created_by: Set(created_by),
        org_id: Set(org_id),
        status: Set(status),
        error: Set(None),
    };
    new_workspace.insert(&db).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to register workspace '{}' in DB: {e}", name),
        )
    })?;
    tracing::info!("Registered workspace '{}' at '{}'", name, path_str);
    Ok(workspace_id)
}

/// Update the clone status and error message for a workspace row.
async fn update_workspace_status(
    workspace_id: Uuid,
    status: entity::workspaces::WorkspaceStatus,
    error: Option<String>,
) -> Result<(), String> {
    use entity::workspaces;
    use oxy::database::client::establish_connection;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    let db = establish_connection()
        .await
        .map_err(|e| format!("Database connection failed: {e}"))?;

    let existing = workspaces::Entity::find_by_id(workspace_id)
        .one(&db)
        .await
        .map_err(|e| format!("Failed to load workspace {workspace_id}: {e}"))?
        .ok_or_else(|| format!("Workspace {workspace_id} no longer exists"))?;

    let mut active: workspaces::ActiveModel = existing.into();
    active.status = Set(status);
    active.error = Set(error);
    active.updated_at = Set(chrono::Utc::now().into());
    active
        .update(&db)
        .await
        .map_err(|e| format!("Failed to update workspace {workspace_id}: {e}"))?;
    Ok(())
}

#[derive(Deserialize, Default)]
pub struct DemoSetupRequest {
    /// Project name (slug) — used as directory name inside the projects root.
    /// Ignored in single-project mode (when PROJECT_DIR was provided to oxy serve).
    pub name: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct NewSetupRequest {
    /// Project name (slug) — used as directory name inside the projects root.
    /// Ignored in single-project mode (when PROJECT_DIR was provided to oxy serve).
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct GitHubSetupRequest {
    pub namespace_id: Uuid,
    pub repo_id: i64,
    pub branch: String,
    /// Project name (slug) — used as directory name inside the projects root.
    /// Ignored in single-project mode.
    pub name: Option<String>,
    /// Optional subdirectory inside the repository to use as the Oxy project root.
    /// For example, `"analytics"` or `"data/oxy"` for a monorepo layout.
    /// The full repository is still cloned; only the registered project path changes.
    pub subdir: Option<String>,
}

/// POST /orgs/{org_id}/onboarding/demo — copy embedded demo workspace files and trigger background reindex.
pub async fn setup_demo(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    OrgContextExtractor(ctx): OrgContextExtractor,
    body: Option<Json<DemoSetupRequest>>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
    require_org_admin(&ctx)?;
    let req = body.map(|b| b.0).unwrap_or_default();

    let workspace_id = Uuid::new_v4();
    let project_dir = resolve_project_dir(workspace_id).map_err(|(status, msg)| {
        error!("{}", msg);
        (status, msg)
    })?;

    if let Err(e) = copy_demo_files_to(&project_dir).await {
        error!("Failed to copy demo workspace files: {:?}", e);
        let _ = std::fs::remove_dir_all(&project_dir);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to copy demo workspace files: {e}"),
        ));
    }

    info!("Demo workspace files copied to {:?}", project_dir);

    let display_name = match req.name.as_deref() {
        Some(n) => n.to_string(),
        None => unique_display_name("Demo workspace")
            .await
            .map_err(|(s, m)| {
                error!("{}", m);
                (s, m)
            })?,
    };
    let workspace_id = match register_project(
        &project_dir,
        &display_name,
        workspace_id,
        Some(user.id),
        Some(ctx.org.id),
        entity::workspaces::WorkspaceStatus::Ready,
        None,
        None,
    )
    .await
    {
        Ok(id) => id,
        Err((status, msg)) => {
            error!("{}", msg);
            let _ = std::fs::remove_dir_all(&project_dir);
            return Err((status, msg));
        }
    };

    // Background reindex — best-effort, does not block the response
    let dir_clone = project_dir.clone();
    tokio::spawn(async move {
        let result = async {
            let config = ConfigBuilder::new()
                .with_workspace_path(&dir_clone)?
                .build_with_fallback_config()
                .await?;

            let secrets_manager = SecretsManager::from_environment()?;

            reindex(ReindexInput {
                config,
                secrets_manager,
                drop_all_tables: true,
            })
            .await
        };

        if let Err(e) = result.await {
            tracing::warn!("Background reindex after demo setup failed: {}", e);
        }
    });

    Ok(Json(OnboardingResult {
        workspace_type: "demo".to_string(),
        workspace_id,
    }))
}

/// POST /orgs/{org_id}/onboarding/new — write a minimal config.yml to the workspace directory if none exists.
pub async fn setup_new(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    OrgContextExtractor(ctx): OrgContextExtractor,
    body: Option<Json<NewSetupRequest>>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
    require_org_admin(&ctx)?;
    let req = body.map(|b| b.0).unwrap_or_default();

    let workspace_id = Uuid::new_v4();
    let project_dir = resolve_project_dir(workspace_id).map_err(|(status, msg)| {
        error!("{}", msg);
        (status, msg)
    })?;

    if !project_dir.join("config.yml").exists()
        && let Err(e) = write_minimal_config_yml(&project_dir).await
    {
        error!("Failed to write config.yml: {}", e);
        let _ = std::fs::remove_dir_all(&project_dir);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write config.yml: {e}"),
        ));
    }

    let display_name = match req.name.as_deref() {
        Some(n) => n.to_string(),
        None => unique_display_name("New workspace")
            .await
            .map_err(|(s, m)| {
                error!("{}", m);
                (s, m)
            })?,
    };
    let workspace_id = match register_project(
        &project_dir,
        &display_name,
        workspace_id,
        Some(user.id),
        Some(ctx.org.id),
        entity::workspaces::WorkspaceStatus::Ready,
        None,
        None,
    )
    .await
    {
        Ok(id) => id,
        Err((status, msg)) => {
            error!("{}", msg);
            let _ = std::fs::remove_dir_all(&project_dir);
            return Err((status, msg));
        }
    };

    Ok(Json(OnboardingResult {
        workspace_type: "new".to_string(),
        workspace_id,
    }))
}

/// POST /orgs/{org_id}/onboarding/github — register a GitHub repository as a workspace and clone
/// it in the background. The workspace appears in the list immediately; the clone runs
/// asynchronously so large repositories don't hit the global request timeout.
pub async fn setup_github(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    OrgContextExtractor(ctx): OrgContextExtractor,
    axum::Json(req): axum::Json<GitHubSetupRequest>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
    require_org_admin(&ctx)?;

    // Verify the namespace belongs to the caller's org — unconditional now that the org is
    // always available from the path. Closes the cross-org namespace bypass (security #2).
    let ns = {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        let db = oxy::database::client::establish_connection()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
        let ns = entity::git_namespaces::Entity::find()
            .filter(entity::git_namespaces::Column::Id.eq(req.namespace_id))
            .one(&db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?
            .ok_or((StatusCode::NOT_FOUND, "Namespace not found".to_string()))?;
        if ns.org_id != Some(ctx.org.id) {
            return Err((
                StatusCode::FORBIDDEN,
                "Namespace does not belong to this org".to_string(),
            ));
        }
        ns
    };

    let token = github_token_for_namespace(&ns).await.map_err(|e| {
        let msg = format!(
            "Failed to load token from git namespace {}: {}",
            req.namespace_id, e
        );
        error!("{}", msg);
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    })?;

    let client = GitHubClient::from_token(token.clone()).map_err(|e| {
        let msg = format!("Failed to create GitHub client: {}", e);
        error!("{}", msg);
        (StatusCode::INTERNAL_SERVER_ERROR, msg)
    })?;

    let repo = client.get_repository(req.repo_id).await.map_err(|e| {
        let msg = format!("Failed to get GitHub repository {}: {}", req.repo_id, e);
        error!("{}", msg);
        (StatusCode::NOT_FOUND, msg)
    })?;

    // Validate the subdirectory before touching the filesystem.
    let subdir = req
        .subdir
        .as_deref()
        .map(parse_subdir)
        .transpose()
        .map_err(|(status, msg)| {
            error!("{}", msg);
            (status, msg)
        })?
        .flatten();

    // Use the caller-supplied name, or fall back to the repository's short name so
    // each imported repo gets its own directory by default.
    let project_name = req.name.as_deref().unwrap_or(&repo.name);
    let workspace_id = Uuid::new_v4();
    // repo_dir is where the full git repository will be cloned.
    let repo_dir = resolve_project_dir(workspace_id).map_err(|(status, msg)| {
        error!("{}", msg);
        (status, msg)
    })?;

    // If a subdirectory was specified, the Oxy workspace root lives inside the repo.
    // Create it eagerly so register_project can write the DB record now, before
    // the background clone finishes.
    let oxy_project_dir = match &subdir {
        Some(sub) => {
            let dir = repo_dir.join(sub);
            std::fs::create_dir_all(&dir).map_err(|e| {
                let msg = format!("Failed to create subdir {:?}: {}", dir, e);
                error!("{}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            })?;
            dir
        }
        None => repo_dir.clone(),
    };

    // Register the workspace in the DB now so it appears in /workspaces immediately.
    let workspace_id = match register_project(
        &oxy_project_dir,
        project_name,
        workspace_id,
        Some(user.id),
        Some(ctx.org.id),
        entity::workspaces::WorkspaceStatus::Cloning,
        Some(req.namespace_id),
        Some(repo.clone_url.clone()),
    )
    .await
    {
        Ok(id) => id,
        Err((status, msg)) => {
            error!("{}", msg);
            let _ = std::fs::remove_dir_all(&repo_dir);
            return Err((status, msg));
        }
    };

    // Clone the full repository in the background — large repositories can take
    // longer than the global request timeout.
    let clone_url = repo.clone_url.clone();
    let branch = req.branch.clone();
    let oxy_project_dir_clone = oxy_project_dir.clone();
    tokio::spawn(async move {
        info!(
            "Cloning repository '{}' branch '{}' into {:?}",
            clone_url, branch, repo_dir
        );
        let clone_result = default_git_client()
            .clone_or_init(&repo_dir, Some(&clone_url), &branch, Some(&token))
            .await;
        let (new_status, new_error) = match clone_result {
            Ok(()) => {
                info!("Repository cloned successfully into {:?}", repo_dir);
                if oxy_project_dir_clone.join("config.yml").exists() {
                    (entity::workspaces::WorkspaceStatus::Ready, None)
                } else {
                    let msg = format!(
                        "Repository '{}' does not appear to be an Oxy project — no config.yml found{}.",
                        clone_url,
                        if oxy_project_dir_clone != repo_dir {
                            format!(" in subdirectory '{}'", oxy_project_dir_clone.display())
                        } else {
                            String::new()
                        }
                    );
                    error!("{}", msg);
                    (entity::workspaces::WorkspaceStatus::Failed, Some(msg))
                }
            }
            Err(e) => {
                let msg = format!("Background clone failed: {e}");
                error!(
                    "Background clone failed for workspace {}: {}",
                    workspace_id, e
                );
                (entity::workspaces::WorkspaceStatus::Failed, Some(msg))
            }
        };

        if let Err(e) = update_workspace_status(workspace_id, new_status, new_error).await {
            error!(
                "Failed to persist clone status for workspace {}: {}",
                workspace_id, e
            );
        }
    });

    Ok(Json(OnboardingResult {
        workspace_type: "github".to_string(),
        workspace_id,
    }))
}

/// Response from the onboarding readiness check.
#[derive(Serialize)]
pub struct ReadinessResponse {
    /// True if at least one LLM API key is set in the environment.
    pub has_llm_key: bool,
    /// Names of LLM API keys that are present in the environment.
    pub llm_keys_present: Vec<String>,
    /// Names of LLM API keys that are absent from the environment.
    pub llm_keys_missing: Vec<String>,
}

/// GET /{workspace_id}/onboarding-readiness — check which LLM API keys are needed by
/// the workspace's config.yml. Runs behind workspace_middleware so access to the
/// workspace is already verified.
pub async fn onboarding_readiness(
    axum::extract::Path(crate::server::api::middlewares::workspace_context::WorkspacePath {
        workspace_id,
    }): axum::extract::Path<crate::server::api::middlewares::workspace_context::WorkspacePath>,
) -> Json<ReadinessResponse> {
    // Route is under /{workspace_id}/ behind workspace_middleware, which has
    // already enforced org membership. No separate access check needed.
    let key_vars = load_key_vars_from_workspace(workspace_id).await;

    let mut llm_keys_present = Vec::new();
    let mut llm_keys_missing = Vec::new();

    for key in &key_vars {
        if std::env::var(key).map(|v| !v.is_empty()).unwrap_or(false) {
            llm_keys_present.push(key.clone());
        } else {
            llm_keys_missing.push(key.clone());
        }
    }

    let has_llm_key = !llm_keys_present.is_empty();

    Json(ReadinessResponse {
        has_llm_key,
        llm_keys_present,
        llm_keys_missing,
    })
}

/// Resolve the unique `key_var` names from the models configured in a workspace's `config.yml`.
async fn load_key_vars_from_workspace(workspace_id: uuid::Uuid) -> Vec<String> {
    let workspace_path = match resolve_workspace_path(workspace_id).await {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let builder = match ConfigBuilder::new().with_workspace_path(&workspace_path) {
        Ok(b) => b,
        Err(_) => return vec![],
    };
    let config = match builder.build_with_fallback_config().await {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    // Collect unique non-None key_var values from all configured models.
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for m in config.models() {
        if let Some(key) = m.key_var()
            && seen.insert(key.to_string())
        {
            result.push(key.to_string());
        }
    }
    result
}
