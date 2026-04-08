use axum::{Json, extract::State, http::StatusCode};
use oxy::adapters::secrets::SecretsManager;
use oxy::config::ConfigBuilder;
use oxy::github::GitHubClient;
use oxy::service::retrieval::{ReindexInput, reindex};
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_project::{GitService, LocalGitService, copy_demo_files_to};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{error, info};
use uuid::Uuid;

use crate::server::router::AppState;

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
/// - In single-workspace mode (`workspaces_root` is `None`): returns CWD.
/// - In multi-workspace mode: returns `workspaces_root/<workspace_id>`, creating it if needed.
///   Using the workspace UUID as the directory name guarantees uniqueness without any
///   name-collision logic.
fn resolve_project_dir(
    app_state: &AppState,
    workspace_id: Uuid,
) -> Result<std::path::PathBuf, (StatusCode, String)> {
    match &app_state.workspaces_root {
        None => std::env::current_dir().map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get current directory: {e}"),
            )
        }),
        Some(root) => {
            let dir = root.join(workspace_id.to_string());
            std::fs::create_dir_all(&dir).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to create workspace directory '{dir:?}': {e}"),
                )
            })?;
            Ok(dir)
        }
    }
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
        workspace_id: Set(Uuid::nil()),
        project_repo_id: Set(None),
        active_branch_id: Set(Uuid::nil()),
        created_at: Set(chrono::Utc::now().into()),
        updated_at: Set(chrono::Utc::now().into()),
        path: Set(Some(path_str.clone())),
        last_opened_at: Set(None),
        created_by: Set(created_by),
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

/// POST /onboarding/demo — copy embedded demo workspace files and trigger background reindex.
pub async fn setup_demo(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    State(app_state): State<AppState>,
    body: Option<Json<DemoSetupRequest>>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
    let req = body.map(|b| b.0).unwrap_or_default();
    let workspace_id = Uuid::new_v4();
    let project_dir = resolve_project_dir(&app_state, workspace_id).map_err(|(status, msg)| {
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
    let workspace_id =
        match register_project(&project_dir, &display_name, workspace_id, Some(user.id)).await {
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

/// POST /onboarding/new — write a minimal config.yml to the workspace directory if none exists.
pub async fn setup_new(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    State(app_state): State<AppState>,
    body: Option<Json<NewSetupRequest>>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
    let req = body.map(|b| b.0).unwrap_or_default();
    let workspace_id = Uuid::new_v4();
    let project_dir = resolve_project_dir(&app_state, workspace_id).map_err(|(status, msg)| {
        error!("{}", msg);
        (status, msg)
    })?;

    let config_path = project_dir.join("config.yml");
    if !config_path.exists() {
        let minimal_config = "# Oxy workspace configuration\n# Add your databases and agents here.\n\ndatabases: []\nmodels: []\n";
        if let Err(e) = std::fs::write(&config_path, minimal_config) {
            error!("Failed to write config.yml: {}", e);
            let _ = std::fs::remove_dir_all(&project_dir);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to write config.yml: {e}"),
            ));
        }
        info!("Created minimal config.yml at {:?}", config_path);
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
    let workspace_id =
        match register_project(&project_dir, &display_name, workspace_id, Some(user.id)).await {
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

/// POST /onboarding/github — register a GitHub repository as a workspace and clone it in the
/// background. The workspace appears in the list immediately; the clone runs asynchronously so
/// large repositories don't hit the global request timeout.
pub async fn setup_github(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    State(app_state): State<AppState>,
    axum::Json(req): axum::Json<GitHubSetupRequest>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
    let token = GitService::load_token_from_git_namespace(req.namespace_id)
        .await
        .map_err(|e| {
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
    let repo_dir = resolve_project_dir(&app_state, workspace_id).map_err(|(status, msg)| {
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
    let workspace_id =
        match register_project(&oxy_project_dir, project_name, workspace_id, Some(user.id)).await {
            Ok(id) => id,
            Err((status, msg)) => {
                error!("{}", msg);
                let _ = std::fs::remove_dir_all(&repo_dir);
                return Err((status, msg));
            }
        };

    // Mark this workspace as cloning so the frontend can show a pending indicator.
    {
        let mut cloning = app_state.cloning_workspaces.lock().unwrap();
        cloning.insert(workspace_id);
    }

    // Clone the full repository in the background — large repositories can take
    // longer than the global request timeout.
    let clone_url = repo.clone_url.clone();
    let branch = req.branch.clone();
    let cloning_projects = app_state.cloning_workspaces.clone();
    let errored_projects = app_state.errored_workspaces.clone();
    let oxy_project_dir_clone = oxy_project_dir.clone();
    tokio::spawn(async move {
        info!(
            "Cloning repository '{}' branch '{}' into {:?}",
            clone_url, branch, repo_dir
        );
        let clone_result =
            LocalGitService::clone_or_init(&repo_dir, Some(&clone_url), &branch, Some(&token))
                .await;
        match clone_result {
            Ok(()) => {
                info!("Repository cloned successfully into {:?}", repo_dir);
                // Verify this is an Oxy project by checking for config.yml.
                if !oxy_project_dir_clone.join("config.yml").exists() {
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
                    errored_projects.lock().unwrap().insert(workspace_id, msg);
                }
            }
            Err(e) => {
                error!(
                    "Background clone failed for workspace {}: {}",
                    workspace_id, e
                );
                // Clean up: remove the empty directory and the stale DB record so
                // the workspace does not appear as a broken entry in /workspaces.
                if let Err(rm_err) = std::fs::remove_dir_all(&repo_dir) {
                    error!("Failed to remove repo_dir {:?}: {}", repo_dir, rm_err);
                }
                if let Ok(db) = oxy::database::client::establish_connection().await {
                    use entity::prelude::Workspaces;
                    use sea_orm::{EntityTrait, ModelTrait};
                    if let Ok(Some(record)) = Workspaces::find_by_id(workspace_id).one(&db).await
                        && let Err(del_err) = record.delete(&db).await
                    {
                        error!(
                            "Failed to delete workspace {} from DB after failed clone: {}",
                            workspace_id, del_err
                        );
                    }
                }
            }
        }
        // Remove from cloning set regardless of success/failure.
        cloning_projects.lock().unwrap().remove(&workspace_id);
    });

    Ok(Json(OnboardingResult {
        workspace_type: "github".to_string(),
        workspace_id,
    }))
}

/// Request body for URL-based GitHub import (local / single-workspace mode).
#[derive(Deserialize)]
pub struct GitHubUrlSetupRequest {
    /// Full git clone URL — HTTPS or SSH (e.g. `https://github.com/acme/repo.git`).
    pub git_url: String,
    /// Branch to clone. Defaults to `"main"` when omitted.
    pub branch: Option<String>,
    /// Local workspace directory name. Derived from the URL when omitted.
    pub name: Option<String>,
    /// Optional subdirectory inside the repository to use as the Oxy workspace root.
    pub subdir: Option<String>,
    /// Personal access token. Omit on the first attempt so host credentials are tried first.
    pub token: Option<String>,
}

/// Check whether a remote git URL is accessible, without performing a full clone.
///
/// Runs `git ls-remote --exit-code --heads <url>` with `GIT_TERMINAL_PROMPT=0`
/// so that git never hangs waiting for interactive credentials.
///
/// When a `token` is provided it is passed via `-c http.extraHeader` so that
/// it never appears in the process argument list (visible via `ps aux`).
///
/// Returns `Ok(())` when the URL is reachable, or an error whose message
/// contains `"auth"` when authentication was the problem.
async fn check_remote_access(url: &str, token: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0");

    if let Some(t) = token {
        // Pass credentials via http.extraHeader — never embeds token in URL or
        // process args, so it does not appear in `ps aux` output.
        cmd.args(["-c", &format!("http.extraHeader=Authorization: Bearer {t}")]);
    }

    cmd.args(["ls-remote", "--exit-code", "--heads", url]);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run git: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    let is_auth_err = stderr.contains("authentication failed")
        || stderr.contains("could not read username")
        || stderr.contains("invalid username or password")
        || stderr.contains("repository not found")
        || stderr.contains("not found")
        || stderr.contains("403")
        || stderr.contains("401");

    if is_auth_err {
        Err("auth".to_owned())
    } else {
        // Strip any credential-like patterns from stderr before surfacing to caller.
        let raw = String::from_utf8_lossy(&output.stderr);
        let sanitized = sanitize_git_stderr(&raw);
        Err(sanitized)
    }
}

/// Remove credential-bearing URL fragments (e.g. `https://token@host`) from git
/// stderr output so they are never forwarded to the caller or written to logs.
fn sanitize_git_stderr(stderr: &str) -> String {
    // Replace `https://<anything>@` with `https://***@` in error messages.
    let re = regex::Regex::new(r"https://[^@\s]+@").expect("static regex is valid");
    re.replace_all(stderr.trim(), "https://***@").into_owned()
}

/// Derive a workspace directory name from a git URL (strips `.git` suffix, takes last path segment).
fn name_from_git_url(url: &str) -> &str {
    url.trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .unwrap_or("workspace")
}

/// POST /onboarding/github-url — import a repository by URL, using host credentials first.
///
/// Call without `token` on the first attempt. If the response is `401 Unauthorized`
/// the caller should prompt for a personal access token and retry with it set.
pub async fn setup_github_url(
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    State(app_state): State<AppState>,
    axum::Json(req): axum::Json<GitHubUrlSetupRequest>,
) -> Result<Json<OnboardingResult>, (StatusCode, String)> {
    // Fast auth check — avoids a full clone just to discover bad credentials.
    match check_remote_access(&req.git_url, req.token.as_deref()).await {
        Ok(()) => {}
        Err(e) if e == "auth" => {
            info!(
                "Remote access check failed with auth error for {}",
                req.git_url
            );
            return Err((
                StatusCode::UNAUTHORIZED,
                "Authentication failed".to_string(),
            ));
        }
        Err(e) => {
            error!("Remote access check failed for {}: {}", req.git_url, e);
            return Err((StatusCode::BAD_REQUEST, e));
        }
    }

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

    let workspace_name = req
        .name
        .as_deref()
        .unwrap_or_else(|| name_from_git_url(&req.git_url));

    let workspace_id = Uuid::new_v4();
    let repo_dir = resolve_project_dir(&app_state, workspace_id).map_err(|(s, msg)| {
        error!("{}", msg);
        (s, msg)
    })?;

    let oxy_dir = match &subdir {
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

    match register_project(&oxy_dir, workspace_name, workspace_id, Some(user.id)).await {
        Ok(_) => {}
        Err((status, msg)) => {
            error!("{}", msg);
            let _ = std::fs::remove_dir_all(&repo_dir);
            return Err((status, msg));
        }
    };

    {
        let mut cloning = app_state.cloning_workspaces.lock().unwrap();
        cloning.insert(workspace_id);
    }

    let git_url = req.git_url.clone();
    let branch = req.branch.clone().unwrap_or_else(|| "main".to_string());
    let token_owned = req.token.clone();
    let cloning_set = app_state.cloning_workspaces.clone();
    let errored_set = app_state.errored_workspaces.clone();
    let oxy_dir_clone = oxy_dir.clone();

    tokio::spawn(async move {
        info!(
            "Cloning '{}' branch '{}' into {:?}",
            git_url, branch, repo_dir
        );
        match LocalGitService::clone_or_init(
            &repo_dir,
            Some(&git_url),
            &branch,
            token_owned.as_deref(),
        )
        .await
        {
            Ok(()) => {
                info!("Cloned workspace {} successfully", workspace_id);
                // Verify this is an Oxy project by checking for config.yml.
                if !oxy_dir_clone.join("config.yml").exists() {
                    let msg = format!(
                        "Repository '{}' does not appear to be an Oxy project — no config.yml found{}.",
                        git_url,
                        if oxy_dir_clone != repo_dir {
                            format!(" in subdirectory '{}'", oxy_dir_clone.display())
                        } else {
                            String::new()
                        }
                    );
                    error!("{}", msg);
                    errored_set.lock().unwrap().insert(workspace_id, msg);
                }
            }
            Err(e) => {
                error!("Clone failed for workspace {}: {}", workspace_id, e);
                if let Err(rm) = std::fs::remove_dir_all(&repo_dir) {
                    error!("Failed to remove {:?}: {}", repo_dir, rm);
                }
                if let Ok(db) = oxy::database::client::establish_connection().await {
                    use entity::prelude::Workspaces;
                    use sea_orm::{EntityTrait, ModelTrait};
                    if let Ok(Some(rec)) = Workspaces::find_by_id(workspace_id).one(&db).await {
                        let _ = rec.delete(&db).await;
                    }
                }
            }
        }
        cloning_set.lock().unwrap().remove(&workspace_id);
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

#[derive(serde::Deserialize)]
pub struct ReadinessQuery {
    pub workspace_id: Option<uuid::Uuid>,
}

/// GET /onboarding/readiness — check which LLM API keys are needed by the workspace's config.yml.
///
/// Accepts an optional `workspace_id` query parameter. When provided, the endpoint loads the
/// workspace's `config.yml`, collects the `key_var` names from all configured models, and
/// checks only those variables — so keys that are not used by any model are not surfaced.
/// When no workspace_id is given (or the config cannot be read), the response contains empty
/// lists (the caller should treat this as "no LLM keys required by the current config").
pub async fn onboarding_readiness(
    _user: AuthenticatedUserExtractor,
    axum::extract::Query(query): axum::extract::Query<ReadinessQuery>,
) -> Json<ReadinessResponse> {
    let key_vars = match query.workspace_id {
        Some(workspace_id) => load_key_vars_from_workspace(workspace_id).await,
        None => vec![],
    };

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
    use entity::prelude::Workspaces;
    use oxy::database::client::establish_connection;
    use sea_orm::EntityTrait;

    let path = async {
        let db = establish_connection().await?;
        let ws = Workspaces::find_by_id(workspace_id).one(&db).await?;
        Ok::<_, anyhow::Error>(ws.and_then(|w| w.path))
    }
    .await
    .unwrap_or(None);

    let Some(path_str) = path else {
        return vec![];
    };

    let builder = match ConfigBuilder::new().with_workspace_path(std::path::Path::new(&path_str)) {
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
