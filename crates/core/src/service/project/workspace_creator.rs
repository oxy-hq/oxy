use crate::api::workspace::{ProjectInfo, WorkspaceResponse};
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::github::{GitHubClient, GitOperations};
use crate::service::project::config_builder::ConfigBuilder;
use crate::service::project::database_operations::DatabaseOperations;
use crate::service::project::git_service::GitService;
use crate::service::project::models::{AgentConfig, CreateWorkspaceRequest};
use axum::{http::StatusCode, response::Json};
use chrono::{DateTime, FixedOffset, Utc};
use entity::{branches, projects, workspace_users, workspaces};
use sea_orm::{ActiveModelTrait, Set, TransactionTrait};
use std::fs;
use std::path::PathBuf;
use tracing::error;
use uuid::Uuid;

pub struct WorkspaceCreator;

impl WorkspaceCreator {
    pub async fn create_workspace(
        user_id: Uuid,
        req: CreateWorkspaceRequest,
    ) -> Result<Json<WorkspaceResponse>, StatusCode> {
        let db = establish_connection().await.map_err(|e| {
            error!("Database connection failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let workspace_id = Uuid::new_v4();

        let txn = db.begin().await.map_err(|e| {
            error!("Failed to begin transaction: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

        let workspace =
            Self::create_workspace_record(&txn, workspace_id, &req.workspace.name, now).await?;
        Self::create_workspace_user(&txn, workspace_id, user_id, now).await?;

        let project_info = match req.workspace.r#type.as_str() {
            "github" => {
                let github = req.github.unwrap();
                Self::create_project_with_git(
                    workspace_id,
                    github.namespace_id,
                    github.repo_id,
                    github.branch,
                )
                .await?
            }
            "new" => Self::create_new_project(&txn, workspace_id, user_id, &req, now).await?,
            _ => {
                error!("Invalid workspace type: {}", req.workspace.r#type);
                return Err(StatusCode::BAD_REQUEST);
            }
        };

        txn.commit().await.map_err(|e| {
            error!("Failed to commit transaction: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(Json(WorkspaceResponse {
            id: workspace.id.to_string(),
            name: workspace.name,
            role: "owner".to_string(),
            created_at: workspace.created_at.to_string(),
            updated_at: workspace.updated_at.to_string(),
            project: project_info,
        }))
    }

    async fn create_workspace_record(
        txn: &sea_orm::DatabaseTransaction,
        workspace_id: Uuid,
        name: &str,
        now: DateTime<FixedOffset>,
    ) -> std::result::Result<workspaces::Model, StatusCode> {
        let workspace = workspaces::ActiveModel {
            id: Set(workspace_id),
            name: Set(name.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        };

        workspace.insert(txn).await.map_err(|e| {
            error!("Failed to create workspace: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
    }

    async fn create_workspace_user(
        txn: &sea_orm::DatabaseTransaction,
        workspace_id: Uuid,
        user_id: Uuid,
        now: DateTime<FixedOffset>,
    ) -> std::result::Result<workspace_users::Model, StatusCode> {
        let ws_user = workspace_users::ActiveModel {
            id: Set(Uuid::new_v4()),
            workspace_id: Set(workspace_id),
            user_id: Set(user_id),
            role: Set("owner".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        };

        ws_user.insert(txn).await.map_err(|e| {
            error!("Failed to add user to workspace: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
    }

    async fn create_new_project(
        txn: &sea_orm::DatabaseTransaction,
        workspace_id: Uuid,
        user_id: Uuid,
        req: &CreateWorkspaceRequest,
        now: DateTime<FixedOffset>,
    ) -> std::result::Result<Option<ProjectInfo>, StatusCode> {
        let project_name = format!(
            "{}-project",
            req.workspace.name.to_lowercase().replace(' ', "-")
        );

        let branch_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();

        let project = Self::create_project_record(
            txn,
            project_id,
            &project_name,
            workspace_id,
            branch_id,
            now,
        )
        .await?;

        let branch = Self::create_branch_record(txn, project.id, branch_id, now).await?;
        let repo_path = Self::create_project_directory(project.id, branch.id)?;

        if let (Some(warehouses), Some(models)) = (&req.warehouses, &req.model) {
            let db = establish_connection().await.map_err(|e| {
                error!("Database connection failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            ConfigBuilder::create_project_config(
                project.id, user_id, warehouses, models, &repo_path, &db,
            )
            .await?;
        }

        if let Some(agent) = &req.agent {
            Self::create_project_agent(&repo_path, agent).await?;
        }

        Ok(Some(ProjectInfo {
            id: project.id.to_string(),
            name: project.name,
            workspace_id: project.workspace_id.to_string(),
            created_at: project.created_at.to_string(),
            updated_at: project.updated_at.to_string(),
        }))
    }

    async fn create_project_record(
        txn: &sea_orm::DatabaseTransaction,
        project_id: Uuid,
        project_name: &str,
        workspace_id: Uuid,
        branch_id: Uuid,
        now: DateTime<FixedOffset>,
    ) -> std::result::Result<projects::Model, StatusCode> {
        let project_model = projects::ActiveModel {
            id: Set(project_id),
            name: Set(project_name.to_string()),
            workspace_id: Set(workspace_id),
            project_repo_id: Set(None),
            active_branch_id: Set(branch_id),
            created_at: Set(now),
            updated_at: Set(now),
        };

        project_model.insert(txn).await.map_err(|e| {
            error!("Failed to create project: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
    }

    async fn create_branch_record(
        txn: &sea_orm::DatabaseTransaction,
        project_id: Uuid,
        branch_id: Uuid,
        now: DateTime<FixedOffset>,
    ) -> std::result::Result<branches::Model, StatusCode> {
        let branch_model = branches::ActiveModel {
            id: Set(branch_id),
            project_id: Set(project_id),
            name: Set("main".to_string()),
            revision: Set("".to_string()),
            sync_status: Set("synced".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        };

        branch_model.insert(txn).await.map_err(|e| {
            error!("Failed to create branch: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
    }

    fn create_project_directory(
        project_id: Uuid,
        branch_id: Uuid,
    ) -> std::result::Result<PathBuf, StatusCode> {
        let repo_path = GitOperations::get_repository_path(project_id, branch_id).map_err(|e| {
            error!("Failed to get repository path: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if !repo_path.exists() {
            fs::create_dir_all(&repo_path).map_err(|e| {
                error!("Failed to create project directory: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }

        Ok(repo_path)
    }

    async fn create_project_agent(
        repo_path: &std::path::Path,
        agent: &AgentConfig,
    ) -> std::result::Result<(), StatusCode> {
        use crate::config::model::{AgentToolsConfig, AgentType, ToolType};

        let agents_path = repo_path.join("agents");
        if !agents_path.exists() {
            fs::create_dir_all(&agents_path).map_err(|e| {
                error!("Failed to create agents directory: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }

        let agent_type = AgentType::Default(crate::config::model::DefaultAgent {
            system_instructions: agent.system_instructions.clone(),
            tools_config: Default::default(),
        });

        let mut agent_config = crate::config::model::AgentConfig {
            name: agent.name.clone(),
            model: agent.model.clone(),
            r#type: agent_type,
            context: None,
            tests: Vec::new(),
            description: agent.description.clone().unwrap_or_default(),
            public: agent.public.unwrap_or(true),
            retrieval: None,
            reasoning: None,
        };

        if let Some(tools) = &agent.tools {
            let mut agent_tools = Vec::new();

            for tool in tools {
                let tool_type = match tool.r#type.as_str() {
                    "execute_sql" => {
                        let execute_sql_config = crate::config::model::ExecuteSQLTool {
                            name: tool.name.clone(),
                            description: tool.description.clone(),
                            database: tool
                                .database
                                .clone()
                                .unwrap_or("default".to_owned())
                                .to_string(),
                            sql: None,
                            dry_run_limit: None,
                        };
                        ToolType::ExecuteSQL(execute_sql_config)
                    }
                    "visualize" => {
                        let visualize_config = crate::config::model::VisualizeTool {
                            name: tool.name.clone(),
                            description: tool.description.clone(),
                        };
                        ToolType::Visualize(visualize_config)
                    }
                    _ => {
                        tracing::warn!(
                            "Unsupported tool type: {}. Only execute_sql and visualize are supported.",
                            tool.r#type
                        );
                        continue;
                    }
                };

                agent_tools.push(tool_type);
            }

            let tools_config = AgentToolsConfig {
                tools: agent_tools,
                max_tool_calls: 10,
                max_tool_concurrency: 5,
            };

            if let AgentType::Default(ref mut default_agent) = agent_config.r#type {
                default_agent.tools_config = tools_config;
            }
        }

        let agent_filename = format!("{}.agent.yml", agent.name.to_lowercase().replace(' ', "_"));
        let agent_path = agents_path.join(agent_filename);

        let agent_yaml = serde_yaml::to_string(&agent_config).map_err(|e| {
            error!("Failed to serialize agent YAML: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        fs::write(&agent_path, agent_yaml).map_err(|e| {
            error!("Failed to write agent file: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        Ok(())
    }

    pub async fn create_project_with_git(
        workspace_id: Uuid,
        git_namespace_id: Uuid,
        repo_id: i64,
        branch_name: String,
    ) -> Result<Option<ProjectInfo>, OxyError> {
        tracing::info!(
            "Creating project using GitHub App installation {}",
            git_namespace_id
        );

        let token = GitService::load_token_from_git_namespace(git_namespace_id).await?;
        let client = GitHubClient::from_token(token.clone())?;
        let repo = client.get_repository(repo_id).await?;

        let active_branch_id = Uuid::new_v4();
        let project_repo_id = Uuid::new_v4();

        let (project, _project_repo) = Self::create_project_and_repo_records(
            workspace_id,
            &repo,
            active_branch_id,
            project_repo_id,
            git_namespace_id,
            repo_id,
        )
        .await?;

        let latest_commit =
            GitService::latest_commit_for_branch(&client, repo_id, &repo, &branch_name).await?;
        let branch =
            Self::create_initial_branch(project.id, active_branch_id, &branch_name, &latest_commit)
                .await?;

        tracing::info!(
            "Created project '{}' with GitHub repository '{}' using GitHub App, starting clone/pull",
            project.name,
            repo.full_name
        );

        let repo_path = GitService::ensure_repo_cloned_and_on_branch(
            &repo,
            &branch_name,
            project.id,
            active_branch_id,
            &token,
        )
        .await?;

        Self::finalize_project_creation(&project, &branch, &repo_path, &token).await
    }

    async fn create_project_and_repo_records(
        workspace_id: Uuid,
        repo: &crate::github::GitHubRepository,
        active_branch_id: Uuid,
        project_repo_id: Uuid,
        git_namespace_id: Uuid,
        repo_id: i64,
    ) -> Result<(projects::Model, entity::project_repos::Model), OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            let project_repo_model = entity::project_repos::ActiveModel {
                id: Set(project_repo_id),
                repo_id: Set(repo_id.to_string()),
                git_namespace_id: Set(git_namespace_id),
                created_at: Set(DatabaseOperations::now().into()),
                updated_at: Set(DatabaseOperations::now().into()),
            };

            let project_repo = project_repo_model.insert(&db).await.map_err(|e| {
                DatabaseOperations::wrap_db_error("Failed to create project repo", e)
            })?;

            let project_model = projects::ActiveModel {
                id: Set(Uuid::new_v4()),
                name: Set(repo.full_name.clone()),
                workspace_id: Set(workspace_id),
                project_repo_id: Set(Some(project_repo_id)),
                active_branch_id: Set(active_branch_id),
                created_at: Set(DatabaseOperations::now().into()),
                updated_at: Set(DatabaseOperations::now().into()),
            };

            let project = project_model
                .insert(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to create project", e))?;

            Ok((project, project_repo))
        })
        .await
    }

    async fn create_initial_branch(
        project_id: Uuid,
        branch_id: Uuid,
        branch_name: &str,
        latest_commit: &str,
    ) -> Result<branches::Model, OxyError> {
        DatabaseOperations::with_connection(|db| async move {
            let branch_model = branches::ActiveModel {
                id: Set(branch_id),
                project_id: Set(project_id),
                name: Set(branch_name.to_string()),
                revision: Set(latest_commit.to_string()),
                sync_status: Set(entity::branches::SyncStatus::Syncing.as_str().to_string()),
                created_at: Set(DatabaseOperations::now().into()),
                updated_at: Set(DatabaseOperations::now().into()),
            };

            branch_model
                .insert(&db)
                .await
                .map_err(|e| DatabaseOperations::wrap_db_error("Failed to create branch", e))
        })
        .await
    }

    async fn finalize_project_creation(
        project: &projects::Model,
        branch: &branches::Model,
        repo_path: &std::path::PathBuf,
        token: &str,
    ) -> Result<Option<ProjectInfo>, OxyError> {
        let res = async {
            if GitOperations::is_git_repository(repo_path).await {
                GitOperations::pull_repository(repo_path, Some(token)).await
            } else {
                Ok(())
            }
        }
        .await;

        DatabaseOperations::with_connection(|db| async move {
            let mut bm: branches::ActiveModel = branch.clone().into();
            match res {
                Ok(()) => {
                    bm.sync_status = Set(entity::branches::SyncStatus::Synced.as_str().to_string());
                    bm.updated_at = Set(DatabaseOperations::now().into());
                    let _updated_branch = bm.update(&db).await.map_err(|e| {
                        DatabaseOperations::wrap_db_error("Failed to update branch after clone", e)
                    })?;
                    tracing::info!(
                        "Successfully cloned/pulled repository for project '{}' branch '{}'",
                        project.name,
                        branch.name
                    );
                    Ok(Some(ProjectInfo {
                        id: project.id.to_string(),
                        name: project.name.clone(),
                        workspace_id: project.workspace_id.to_string(),
                        created_at: project.created_at.to_string(),
                        updated_at: project.updated_at.to_string(),
                    }))
                }
                Err(e) => {
                    bm.sync_status = Set(entity::branches::SyncStatus::Failed.as_str().to_string());
                    bm.updated_at = Set(DatabaseOperations::now().into());
                    if let Err(db_err) = bm.update(&db).await {
                        tracing::warn!(
                            "Failed to update branch status after clone failure: {}",
                            db_err
                        );
                    }
                    Err(e)
                }
            }
        })
        .await
    }
}
