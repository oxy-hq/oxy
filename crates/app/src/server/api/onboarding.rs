use axum::{Json, extract::Path, http::StatusCode};
use oxy::adapters::secrets::SecretsManager;
use oxy::adapters::workspace::{resolve_workspace_path, workspace_root_path};
use oxy::config::ConfigBuilder;
use oxy::github::{GitHubClient, default_git_client, github_token_for_namespace};
use oxy::service::retrieval::{ReindexInput, reindex};
use oxy::service::secret_manager::SecretManagerService;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_git::GitClient;
use oxy_project::{copy_demo_files_to, write_minimal_config_yml};
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use crate::server::api::middlewares::role_guards::OrgAdmin;
use crate::server::api::middlewares::workspace_context::{
    WorkspaceManagerExtractor, WorkspacePath,
};

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
    OrgAdmin(ctx): OrgAdmin,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    body: Option<Json<DemoSetupRequest>>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
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
    OrgAdmin(ctx): OrgAdmin,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    body: Option<Json<NewSetupRequest>>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
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
    OrgAdmin(ctx): OrgAdmin,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    axum::Json(req): axum::Json<GitHubSetupRequest>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
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
                    (
                        entity::workspaces::WorkspaceStatus::NotOxyProject,
                        Some(msg),
                    )
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

/// Response for `GET /{workspace_id}/onboarding/github-setup`.
///
/// Describes the setup work a GitHub-imported workspace still needs before the
/// user can start asking questions: which LLM API keys are referenced by the
/// repo's `config.yml` but don't yet have a secret set, and which warehouses
/// declare `*_var` references that don't yet resolve.
///
/// The shape mirrors how the frontend presents the prompts:
/// - one `secure_input` per `missing_llm_key_vars` entry
/// - one `credential_form` per `warehouses` entry (containing a field for each
///   `missing_vars` entry)
#[derive(Serialize, Default, Debug, Clone)]
pub struct GithubSetupResponse {
    /// `key_var` names from the repo's configured models (`openai`, `anthropic`,
    /// etc.) that do not yet have a workspace secret. Already-set keys are
    /// omitted so the user isn't asked to re-enter them.
    pub missing_llm_key_vars: Vec<GithubSetupKeyVar>,
    /// Warehouses declared in `config.yml` that still need at least one secret
    /// value before the connection can be tested. Warehouses whose `*_var`
    /// references all resolve are omitted.
    pub warehouses: Vec<GithubSetupWarehouse>,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct GithubSetupKeyVar {
    /// Env-var name to store the secret under (e.g. `ANTHROPIC_API_KEY`).
    pub var_name: String,
    /// User-facing vendor label derived from the model's `vendor` field — used
    /// to personalise the prompt ("Enter your Anthropic API key").
    pub vendor: String,
    /// One sample model name using this key, purely informational.
    pub sample_model_name: Option<String>,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct GithubSetupWarehouse {
    /// Warehouse `name` as declared in `config.yml`.
    pub name: String,
    /// Dialect string (`postgres`, `snowflake`, …) from `Database::dialect`.
    pub dialect: String,
    /// `*_var` fields on this warehouse that don't yet resolve to a secret.
    pub missing_vars: Vec<GithubSetupMissingVar>,
}

#[derive(Serialize, Default, Debug, Clone)]
pub struct GithubSetupMissingVar {
    /// Field this var refers to (`password`, `user`, `host`, `port`,
    /// `database`, `key_path`, `token`, `developer_token`).
    pub field: String,
    /// Env-var name declared in config.yml (e.g. `SNOWFLAKE_PASSWORD`).
    pub var_name: String,
    /// True when the corresponding plain value isn't also declared in the
    /// config. False means the config already has an inline value and the
    /// `*_var` is only used as a fallback — these entries can be treated as
    /// optional by the UI.
    pub required: bool,
}

/// GET /{workspace_id}/onboarding/github-setup — return the setup work needed
/// before a GitHub-imported workspace can be queried. See `GithubSetupResponse`.
pub async fn github_setup(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
) -> Result<Json<GithubSetupResponse>, (StatusCode, String)> {
    let secret_manager = SecretManagerService::new(workspace_id);
    let config_manager = &workspace_manager.config_manager;

    // ── LLM keys ──────────────────────────────────────────────────────────────
    // Deduplicate by var name — two models sharing a key_var only surface once.
    let mut seen_llm_keys = std::collections::HashSet::new();
    let mut missing_llm_key_vars: Vec<GithubSetupKeyVar> = Vec::new();
    for model in config_manager.models() {
        let Some(key_var) = model.key_var() else {
            continue;
        };
        if !seen_llm_keys.insert(key_var.to_string()) {
            continue;
        }
        if secret_manager.get_secret(key_var).await.is_some() {
            continue;
        }
        missing_llm_key_vars.push(GithubSetupKeyVar {
            var_name: key_var.to_string(),
            vendor: vendor_label_for_model(model),
            sample_model_name: Some(model.name().to_string()),
        });
    }

    // ── Warehouses ───────────────────────────────────────────────────────────
    let mut warehouses: Vec<GithubSetupWarehouse> = Vec::new();
    for database in config_manager.list_databases() {
        let vars = collect_warehouse_vars(database);
        let mut missing_vars: Vec<GithubSetupMissingVar> = Vec::new();
        for var in vars {
            if secret_manager.get_secret(&var.var_name).await.is_some() {
                continue;
            }
            missing_vars.push(var);
        }
        if !missing_vars.is_empty() {
            warehouses.push(GithubSetupWarehouse {
                name: database.name.clone(),
                dialect: database.dialect(),
                missing_vars,
            });
        }
    }

    Ok(Json(GithubSetupResponse {
        missing_llm_key_vars,
        warehouses,
    }))
}

fn vendor_label_for_model(model: &oxy::config::model::Model) -> String {
    use oxy::config::model::Model as M;
    match model {
        M::OpenAI { .. } => "OpenAI".to_string(),
        M::Anthropic { .. } => "Anthropic".to_string(),
        M::Google { .. } => "Google".to_string(),
        M::Ollama { .. } => "Ollama".to_string(),
    }
}

/// Enumerate the `*_var` credential fields declared on a warehouse, tagging
/// each with the config field it maps to and whether it's strictly required
/// (no inline plaintext fallback). The frontend uses this to build a
/// `credential_form` with one password-style input per entry.
fn collect_warehouse_vars(database: &oxy::config::model::Database) -> Vec<GithubSetupMissingVar> {
    use oxy::config::model::{DatabaseType, SnowflakeAuthType};

    let mut out: Vec<GithubSetupMissingVar> = Vec::new();
    let push_opt =
        |out: &mut Vec<GithubSetupMissingVar>, field: &str, var: &Option<String>, inline: bool| {
            if let Some(v) = var {
                out.push(GithubSetupMissingVar {
                    field: field.to_string(),
                    var_name: v.clone(),
                    required: !inline,
                });
            }
        };
    let push_req = |out: &mut Vec<GithubSetupMissingVar>, field: &str, var: &str| {
        out.push(GithubSetupMissingVar {
            field: field.to_string(),
            var_name: var.to_string(),
            required: true,
        });
    };

    match &database.database_type {
        DatabaseType::Postgres(p) => {
            push_opt(&mut out, "host", &p.host_var, p.host.is_some());
            push_opt(&mut out, "port", &p.port_var, p.port.is_some());
            push_opt(&mut out, "user", &p.user_var, p.user.is_some());
            push_opt(&mut out, "password", &p.password_var, p.password.is_some());
            push_opt(&mut out, "database", &p.database_var, p.database.is_some());
        }
        DatabaseType::Redshift(r) => {
            push_opt(&mut out, "host", &r.host_var, r.host.is_some());
            push_opt(&mut out, "port", &r.port_var, r.port.is_some());
            push_opt(&mut out, "user", &r.user_var, r.user.is_some());
            push_opt(&mut out, "password", &r.password_var, r.password.is_some());
            push_opt(&mut out, "database", &r.database_var, r.database.is_some());
        }
        DatabaseType::Mysql(m) => {
            push_opt(&mut out, "host", &m.host_var, m.host.is_some());
            push_opt(&mut out, "port", &m.port_var, m.port.is_some());
            push_opt(&mut out, "user", &m.user_var, m.user.is_some());
            push_opt(&mut out, "password", &m.password_var, m.password.is_some());
            push_opt(&mut out, "database", &m.database_var, m.database.is_some());
        }
        DatabaseType::ClickHouse(c) => {
            push_opt(&mut out, "host", &c.host_var, c.host.is_some());
            push_opt(&mut out, "user", &c.user_var, c.user.is_some());
            push_opt(&mut out, "password", &c.password_var, c.password.is_some());
            push_opt(&mut out, "database", &c.database_var, c.database.is_some());
        }
        DatabaseType::Snowflake(s) => {
            if let SnowflakeAuthType::PasswordVar { password_var } = &s.auth_type {
                push_req(&mut out, "password", password_var);
            }
        }
        DatabaseType::Bigquery(b) => {
            push_opt(&mut out, "key_path", &b.key_path_var, b.key_path.is_some());
        }
        DatabaseType::MotherDuck(md) => {
            push_req(&mut out, "token", &md.token_var);
        }
        DatabaseType::DOMO(d) => {
            push_req(&mut out, "developer_token", &d.developer_token_var);
        }
        DatabaseType::DuckDB(_) => {
            // No credentials.
        }
    }
    out
}

/// Manifest of onboarding side-effects to revert.
///
/// Each list is handled idempotently — missing entries are silently skipped so
/// the client can send a best-effort manifest derived from its local state.
#[derive(Deserialize, Default)]
pub struct OnboardingResetRequest {
    /// Secret names to delete (e.g. `ANTHROPIC_API_KEY`).
    #[serde(default)]
    pub secret_names: Vec<String>,
    /// Database names to remove from `config.yml`. For each database, the
    /// associated password secret (via `password_var`) is also deleted.
    #[serde(default)]
    pub database_names: Vec<String>,
    /// Model names to remove from `config.yml`. For each model, the associated
    /// API key secret (via `key_var`) is also deleted.
    #[serde(default)]
    pub model_names: Vec<String>,
    /// File paths (relative to the workspace root) to delete.
    #[serde(default)]
    pub file_paths: Vec<String>,
    /// Directory paths (relative to the workspace root) to recursively delete.
    /// Used for wiping generated trees such as `.databases/<warehouse>/`.
    #[serde(default)]
    pub directory_paths: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct OnboardingResetResponse {
    pub secrets_deleted: Vec<String>,
    pub databases_removed: Vec<String>,
    pub models_removed: Vec<String>,
    pub files_deleted: Vec<String>,
    pub directories_deleted: Vec<String>,
    /// Human-readable warnings for individual entries that could not be
    /// reverted — the overall request still returns 200 to stay idempotent.
    pub warnings: Vec<String>,
}

/// POST /workspaces/{id}/onboarding/reset — revert the server-side side effects
/// of a partial onboarding run (secrets, warehouse entries in `config.yml`, and
/// generated files). Intended to back the "Start over" UI.
pub async fn reset_onboarding(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    Json(request): Json<OnboardingResetRequest>,
) -> Result<Json<OnboardingResetResponse>, StatusCode> {
    let db = oxy::database::client::establish_connection()
        .await
        .map_err(|e| {
            error!("Failed to establish database connection for reset: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let secret_manager = SecretManagerService::new(workspace_id);
    let mut response = OnboardingResetResponse::default();

    // Collect password-var secrets to also delete, by looking up each database
    // in the current config before removing it.
    let mut password_secrets: Vec<String> = Vec::new();
    for db_name in &request.database_names {
        match workspace_manager.config_manager.resolve_database(db_name) {
            Ok(database) => {
                if let Some(password_var) = workspace_manager
                    .config_manager
                    .get_database_password_var(database)
                {
                    password_secrets.push(password_var);
                }
            }
            Err(_) => {
                // Missing from config.yml — nothing to look up; remove_database
                // below will record a warning.
            }
        }
    }

    // Collect key_var secrets for each model we're about to remove.
    let mut model_key_secrets: Vec<String> = Vec::new();
    for model_name in &request.model_names {
        match workspace_manager.config_manager.resolve_model(model_name) {
            Ok(model) => {
                if let Some(key_var) = model.key_var() {
                    model_key_secrets.push(key_var.to_string());
                }
            }
            Err(_) => {
                // Missing from config.yml — nothing to look up; remove_model
                // below will record a warning.
            }
        }
    }

    // Remove databases from config.yml (idempotent — "not found" is a warning,
    // not an error).
    for db_name in &request.database_names {
        match workspace_manager
            .config_manager
            .remove_database(db_name)
            .await
        {
            Ok(()) => response.databases_removed.push(db_name.clone()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("not found") {
                    response
                        .warnings
                        .push(format!("Database '{db_name}' was not in config.yml"));
                } else {
                    error!("Failed to remove database '{db_name}': {}", e);
                    response
                        .warnings
                        .push(format!("Failed to remove database '{db_name}': {msg}"));
                }
            }
        }
    }

    // Remove model entries from config.yml. Each call rewrites config.yml, so
    // this serialises after the database removals above to avoid overwrites.
    for model_name in &request.model_names {
        match workspace_manager
            .config_manager
            .remove_model(model_name)
            .await
        {
            Ok(()) => response.models_removed.push(model_name.clone()),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("not found") {
                    response
                        .warnings
                        .push(format!("Model '{model_name}' was not in config.yml"));
                } else {
                    error!("Failed to remove model '{model_name}': {}", e);
                    response
                        .warnings
                        .push(format!("Failed to remove model '{model_name}': {msg}"));
                }
            }
        }
    }

    // Delete secrets (the explicit list plus password_var / key_var derived above).
    let mut all_secret_names: Vec<String> = request.secret_names.clone();
    all_secret_names.extend(password_secrets);
    all_secret_names.extend(model_key_secrets);
    all_secret_names.sort();
    all_secret_names.dedup();

    for name in all_secret_names {
        match secret_manager.delete_secret(&db, &name).await {
            Ok(()) => response.secrets_deleted.push(name),
            Err(OxyError::SecretManager(msg)) if msg.to_lowercase().contains("not found") => {
                // Already absent — silently skip.
            }
            Err(e) => {
                error!("Failed to delete secret '{}': {}", name, e);
                response
                    .warnings
                    .push(format!("Failed to delete secret '{name}': {e}"));
            }
        }
    }

    // Delete files. Follows the same resolution pattern as `file::delete_file`.
    let workspace_root = workspace_manager
        .config_manager
        .workspace_path()
        .to_path_buf();

    for path in &request.file_paths {
        let resolved = match workspace_manager
            .config_manager
            .resolve_file(path.clone())
            .await
        {
            Ok(r) => r,
            Err(e) => {
                response
                    .warnings
                    .push(format!("Invalid file path '{path}': {e}"));
                continue;
            }
        };
        let file_path = workspace_root.join(&resolved);
        match tokio::fs::remove_file(&file_path).await {
            Ok(()) => response.files_deleted.push(path.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Already absent — silently skip.
            }
            Err(e) => {
                error!("Failed to delete file {:?}: {}", file_path, e);
                response
                    .warnings
                    .push(format!("Failed to delete file '{path}': {e}"));
            }
        }
    }

    // Recursively delete directories (e.g. `.databases/<warehouse>/` metadata).
    for path in &request.directory_paths {
        let resolved = match workspace_manager
            .config_manager
            .resolve_file(path.clone())
            .await
        {
            Ok(r) => r,
            Err(e) => {
                response
                    .warnings
                    .push(format!("Invalid directory path '{path}': {e}"));
                continue;
            }
        };
        let dir_path = workspace_root.join(&resolved);
        match tokio::fs::remove_dir_all(&dir_path).await {
            Ok(()) => response.directories_deleted.push(path.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Already absent — silently skip.
            }
            Err(e) => {
                error!("Failed to delete directory {:?}: {}", dir_path, e);
                response
                    .warnings
                    .push(format!("Failed to delete directory '{path}': {e}"));
            }
        }
    }

    info!(
        workspace_id = %workspace_id,
        secrets = response.secrets_deleted.len(),
        databases = response.databases_removed.len(),
        models = response.models_removed.len(),
        files = response.files_deleted.len(),
        directories = response.directories_deleted.len(),
        warnings = response.warnings.len(),
        "Onboarding reset completed"
    );

    Ok(Json(response))
}

// ── Warehouse file upload ──────────────────────────────────────────────────

/// Default subdirectory (relative to the workspace root) where uploaded DuckDB
/// data files are written when the client does not specify one.
const DEFAULT_UPLOAD_SUBDIR: &str = ".db";

/// Per-file byte cap for onboarding data uploads.
const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;

/// Per-request aggregate byte cap (across all files).
const MAX_TOTAL_BYTES: u64 = 500 * 1024 * 1024;

/// Body-limit constant for the router. Sized slightly above `MAX_TOTAL_BYTES`
/// to accommodate multipart framing overhead (boundaries, headers).
pub const MAX_UPLOAD_BODY_BYTES: usize = (MAX_TOTAL_BYTES as usize) + 1024 * 1024;

/// Hard cap on the number of files accepted in a single upload request.
const MAX_FILES_PER_REQUEST: usize = 25;

/// Supported file extensions, lowercased. Must mirror the DuckDB connector's
/// `collect_supported_files` so the connection test cannot find a file type
/// that the upload endpoint did not accept.
const SUPPORTED_EXTENSIONS: &[&str] = &["csv", "parquet"];

#[derive(Serialize)]
pub struct SkippedUpload {
    pub name: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct UploadWarehouseFilesResponse {
    /// The resolved subdir, relative to the workspace root (e.g. ".db").
    pub subdir: String,
    /// Paths of files successfully written, relative to the workspace root.
    pub files: Vec<String>,
    /// Files that were rejected (unsupported extension, oversize, etc.).
    pub skipped: Vec<SkippedUpload>,
}

/// POST /workspaces/{id}/onboarding/upload-warehouse-files — stream uploaded
/// CSV/Parquet files into `<workspace_root>/<subdir>/` (default `.db/`).
///
/// Backs the DuckDB file-upload onboarding step. The onboarding client is
/// expected to then submit a warehouse config with `file_search_path = subdir`,
/// which the connector will scan for the just-uploaded files.
pub async fn upload_warehouse_files(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    AuthenticatedUserExtractor(_user): AuthenticatedUserExtractor,
    mut multipart: axum::extract::Multipart,
) -> Result<(StatusCode, Json<UploadWarehouseFilesResponse>), (StatusCode, String)> {
    let workspace_root = workspace_manager
        .config_manager
        .workspace_path()
        .to_path_buf();

    let mut subdir: Option<std::path::PathBuf> = None;
    let mut files_out: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedUpload> = Vec::new();
    // Processed lazily on the first `file` field so callers can send `subdir`
    // either before or after the files in the form.
    let mut target_dir: Option<std::path::PathBuf> = None;
    let mut total_bytes: u64 = 0;
    let mut files_seen: usize = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to parse multipart body: {e}"),
        )
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "subdir" => {
                let raw = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read subdir field: {e}"),
                    )
                })?;
                subdir = parse_subdir(&raw)?;
            }
            "file" => {
                files_seen += 1;
                if files_seen > MAX_FILES_PER_REQUEST {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("Too many files in upload request (max {MAX_FILES_PER_REQUEST})"),
                    ));
                }
                // Resolve the target dir once, on the first file.
                if target_dir.is_none() {
                    let dir_rel = subdir
                        .clone()
                        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_UPLOAD_SUBDIR));
                    let dir_abs = workspace_root.join(&dir_rel);
                    tokio::fs::create_dir_all(&dir_abs).await.map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to create upload directory: {e}"),
                        )
                    })?;
                    target_dir = Some(dir_abs);
                }

                let raw_name = field.file_name().unwrap_or("").to_string();
                let sanitised = match sanitise_upload_filename(&raw_name) {
                    Ok(n) => n,
                    Err(reason) => {
                        skipped.push(SkippedUpload {
                            name: raw_name,
                            reason: reason.to_string(),
                        });
                        // Drain the field so the stream stays in sync.
                        drain_field(field).await?;
                        continue;
                    }
                };

                if !has_supported_extension(&sanitised) {
                    skipped.push(SkippedUpload {
                        name: sanitised,
                        reason: "unsupported_extension".to_string(),
                    });
                    drain_field(field).await?;
                    continue;
                }

                let dir_abs = target_dir.as_ref().expect("target_dir set above");
                let final_path = dir_abs.join(&sanitised);
                if tokio::fs::try_exists(&final_path).await.unwrap_or(false) {
                    return Err((
                        StatusCode::CONFLICT,
                        format!(
                            "A file named '{sanitised}' already exists in the upload directory. \
                            Rename or remove the existing file before re-uploading."
                        ),
                    ));
                }

                let tmp_path = dir_abs.join(format!("{sanitised}.upload-tmp"));
                let bytes_written =
                    stream_field_to_file(field, &tmp_path, MAX_FILE_BYTES, &mut total_bytes)
                        .await?;

                tracing::info!(
                    workspace_id = %workspace_id,
                    filename = %sanitised,
                    bytes = bytes_written,
                    "Uploaded warehouse file"
                );

                tokio::fs::rename(&tmp_path, &final_path)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to finalise upload '{sanitised}': {e}"),
                        )
                    })?;

                let rel = target_dir
                    .as_ref()
                    .and_then(|d| d.strip_prefix(&workspace_root).ok())
                    .map(|r| r.join(&sanitised))
                    .unwrap_or_else(|| std::path::PathBuf::from(&sanitised));
                files_out.push(rel.to_string_lossy().replace('\\', "/"));
            }
            _ => {
                // Ignore unknown fields, but drain them to keep the stream in sync.
                drain_field(field).await?;
            }
        }
    }

    if files_out.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No supported files uploaded. Upload at least one .csv or .parquet file.".to_string(),
        ));
    }

    let resolved_subdir = subdir
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_UPLOAD_SUBDIR))
        .to_string_lossy()
        .replace('\\', "/");

    Ok((
        StatusCode::CREATED,
        Json(UploadWarehouseFilesResponse {
            subdir: resolved_subdir,
            files: files_out,
            skipped,
        }),
    ))
}

/// Return the basename of `raw` after rejecting anything that could escape the
/// target directory (absolute paths, `..`, embedded separators, hidden files).
fn sanitise_upload_filename(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty_filename");
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("path_separator_in_filename");
    }
    if trimmed == "." || trimmed == ".." {
        return Err("invalid_filename");
    }
    if trimmed.starts_with('.') {
        return Err("hidden_filename");
    }
    if trimmed.contains('\0') {
        return Err("null_byte_in_filename");
    }
    Ok(trimmed.to_string())
}

fn has_supported_extension(name: &str) -> bool {
    match std::path::Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
    {
        Some(ext) => SUPPORTED_EXTENSIONS
            .iter()
            .any(|supported| supported.eq_ignore_ascii_case(ext)),
        None => false,
    }
}

async fn drain_field(
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<(), (StatusCode, String)> {
    while let Some(chunk) = field.chunk().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to read multipart chunk: {e}"),
        )
    })? {
        let _ = chunk;
    }
    Ok(())
}

/// Stream one `file` field to `tmp_path`, enforcing per-file and request-wide
/// byte caps. On any error (including size overflow) the partial file is
/// removed so we don't leak a half-written upload.
async fn stream_field_to_file(
    mut field: axum::extract::multipart::Field<'_>,
    tmp_path: &std::path::Path,
    max_file_bytes: u64,
    total_bytes: &mut u64,
) -> Result<u64, (StatusCode, String)> {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(tmp_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create upload temp file: {e}"),
        )
    })?;

    let mut written: u64 = 0;
    loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                let chunk_len = chunk.len() as u64;
                if written.saturating_add(chunk_len) > max_file_bytes {
                    let _ = tokio::fs::remove_file(tmp_path).await;
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!(
                            "File exceeds per-file size limit of {} bytes",
                            max_file_bytes
                        ),
                    ));
                }
                if total_bytes.saturating_add(chunk_len) > MAX_TOTAL_BYTES {
                    let _ = tokio::fs::remove_file(tmp_path).await;
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!(
                            "Upload exceeds aggregate size limit of {} bytes",
                            MAX_TOTAL_BYTES
                        ),
                    ));
                }
                file.write_all(&chunk).await.map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to write upload chunk: {e}"),
                    )
                })?;
                written += chunk_len;
                *total_bytes += chunk_len;
            }
            Ok(None) => break,
            Err(e) => {
                let _ = tokio::fs::remove_file(tmp_path).await;
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Failed to read upload stream: {e}"),
                ));
            }
        }
    }

    file.flush().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to flush upload: {e}"),
        )
    })?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitise_rejects_empty() {
        assert_eq!(sanitise_upload_filename(""), Err("empty_filename"));
        assert_eq!(sanitise_upload_filename("   "), Err("empty_filename"));
    }

    #[test]
    fn sanitise_rejects_separators() {
        assert_eq!(
            sanitise_upload_filename("foo/bar.csv"),
            Err("path_separator_in_filename")
        );
        assert_eq!(
            sanitise_upload_filename("foo\\bar.csv"),
            Err("path_separator_in_filename")
        );
    }

    #[test]
    fn sanitise_rejects_traversal() {
        // `.` / `..` are caught by the explicit invalid-filename guard, which
        // runs before the hidden-file check.
        assert_eq!(sanitise_upload_filename(".."), Err("invalid_filename"));
        assert_eq!(sanitise_upload_filename("."), Err("invalid_filename"));
    }

    #[test]
    fn sanitise_rejects_hidden() {
        assert_eq!(
            sanitise_upload_filename(".DS_Store"),
            Err("hidden_filename")
        );
        assert_eq!(
            sanitise_upload_filename(".env.parquet"),
            Err("hidden_filename")
        );
    }

    #[test]
    fn sanitise_rejects_null_byte() {
        assert_eq!(
            sanitise_upload_filename("foo\0.csv"),
            Err("null_byte_in_filename")
        );
    }

    #[test]
    fn sanitise_accepts_plain_filenames() {
        assert_eq!(
            sanitise_upload_filename("orders.csv"),
            Ok("orders.csv".to_string())
        );
        assert_eq!(
            sanitise_upload_filename("  Orders-2024.Parquet  "),
            Ok("Orders-2024.Parquet".to_string())
        );
    }

    #[test]
    fn supported_extension_accepts_csv_parquet_case_insensitive() {
        assert!(has_supported_extension("foo.csv"));
        assert!(has_supported_extension("foo.CSV"));
        assert!(has_supported_extension("foo.Parquet"));
        assert!(has_supported_extension("foo.PARQUET"));
    }

    #[test]
    fn supported_extension_rejects_others() {
        assert!(!has_supported_extension("foo"));
        assert!(!has_supported_extension("foo.txt"));
        assert!(!has_supported_extension("foo.json"));
        assert!(!has_supported_extension("notes.md"));
    }
}
