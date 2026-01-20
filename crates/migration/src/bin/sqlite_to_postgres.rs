use anyhow::{Context, Result};
use clap::Parser;
use migration::MigratorTrait;
use sea_orm::{Database, DatabaseConnection, EntityTrait, QueryOrder, Set};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about = "Migrate data from SQLite to PostgreSQL", long_about = None)]
struct Args {
    /// SQLite database URL (e.g., sqlite:///path/to/db.sqlite)
    /// Defaults to the standard Oxy SQLite location: ~/.local/share/oxy/db.sqlite
    #[arg(long, env = "SQLITE_URL")]
    from: Option<String>,

    /// PostgreSQL database URL (e.g., postgresql://postgres:postgres@localhost:15432/oxy)
    /// REQUIRED: Start PostgreSQL first using 'oxy start' or Docker
    #[arg(long, env = "POSTGRES_URL")]
    to: String,

    /// Dry run - don't actually insert data
    #[arg(long, default_value = "false")]
    dry_run: bool,
}

/// Get the state directory for Oxy data
/// Duplicated from oxy::state_dir to avoid circular dependency
fn get_state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("OXY_STATE_DIR") {
        return PathBuf::from(dir);
    }

    let homedir = home::home_dir().unwrap_or_else(|| {
        eprintln!("Error: Could not determine home directory.");
        std::process::exit(1);
    });
    homedir.join(".local/share/oxy")
}

fn get_default_sqlite_path() -> String {
    let state_dir = get_state_dir();
    let db_path = state_dir.join("db.sqlite");
    format!("sqlite://{}", db_path.to_string_lossy())
}

/// Wait for a database connection to be ready with retries
async fn wait_for_connection(
    connection_url: &str,
    max_attempts: u32,
) -> Result<DatabaseConnection> {
    let mut attempt = 1;
    let mut delay = Duration::from_millis(100);

    loop {
        match Database::connect(connection_url).await {
            Ok(conn) => {
                info!("Successfully connected to database");
                return Ok(conn);
            }
            Err(_e) if attempt < max_attempts => {
                info!(
                    "Connection attempt {}/{} failed, retrying in {:?}...",
                    attempt, max_attempts, delay
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
                delay = std::cmp::min(delay * 2, Duration::from_secs(5));
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Determine source SQLite database
    let from = args.from.unwrap_or_else(|| {
        let default = get_default_sqlite_path();
        info!("No --from specified, using default SQLite location");
        default
    });

    let to = args.to;

    info!("Starting migration from SQLite to PostgreSQL");
    info!("Source: {}", from);
    info!("Target: {}", to);
    info!("");
    info!("NOTE: Make sure PostgreSQL is running before starting migration.");
    info!("      You can start it with: oxy start");

    if args.dry_run {
        warn!("DRY RUN MODE - no data will be written");
    }

    // Connect to both databases with retries
    info!("Connecting to SQLite database...");
    let sqlite = wait_for_connection(&from, 3)
        .await
        .context("Failed to connect to SQLite database")?;

    info!("Connecting to PostgreSQL database...");
    let postgres = wait_for_connection(&to, 20)
        .await
        .context("Failed to connect to PostgreSQL database")?;

    // Run migrations on PostgreSQL first
    info!("Running migrations on PostgreSQL...");
    migration::Migrator::up(&postgres, None)
        .await
        .context("Failed to run migrations on PostgreSQL")?;

    // Migrate data for each entity in dependency order
    // Order matters due to foreign key constraints!

    info!("=== Starting data migration ===");

    // 1. Base entities (no dependencies)
    migrate_users(&sqlite, &postgres, args.dry_run).await?;
    migrate_workspaces(&sqlite, &postgres, args.dry_run).await?;
    migrate_git_namespaces(&sqlite, &postgres, args.dry_run).await?;

    // 2. Entities that depend on workspaces and users
    migrate_workspace_users(&sqlite, &postgres, args.dry_run).await?;

    // 3. Entities that depend on git_namespaces
    migrate_project_repos(&sqlite, &postgres, args.dry_run).await?;

    // 4. Entities that depend on workspaces and project_repos
    migrate_projects(&sqlite, &postgres, args.dry_run).await?;

    // 5. Entities that depend on projects
    migrate_branches(&sqlite, &postgres, args.dry_run).await?;
    migrate_threads(&sqlite, &postgres, args.dry_run).await?;
    migrate_secrets(&sqlite, &postgres, args.dry_run).await?;
    migrate_api_keys(&sqlite, &postgres, args.dry_run).await?;

    // 6. Entities that depend on threads
    migrate_messages(&sqlite, &postgres, args.dry_run).await?;
    migrate_artifacts(&sqlite, &postgres, args.dry_run).await?;
    migrate_logs(&sqlite, &postgres, args.dry_run).await?;

    // 7. Entities that depend on messages and projects
    migrate_runs(&sqlite, &postgres, args.dry_run).await?;

    // 8. Entities that depend on runs
    migrate_checkpoints(&sqlite, &postgres, args.dry_run).await?;

    // 9. Other entities
    migrate_settings(&sqlite, &postgres, args.dry_run).await?;
    migrate_tasks(&sqlite, &postgres, args.dry_run).await?;

    info!("=== Migration completed successfully! ===");

    Ok(())
}

async fn migrate_users(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::users;

    info!("Migrating users...");

    let records = users::Entity::find()
        .order_by_asc(users::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch users from SQLite")?;

    info!("Found {} user records", records.len());

    if !dry_run {
        for record in &records {
            let active_model: users::ActiveModel = entity::users::ActiveModel {
                id: Set(record.id),
                email: Set(record.email.clone()),
                name: Set(record.name.clone()),
                picture: Set(record.picture.clone()),
                password_hash: Set(record.password_hash.clone()),
                email_verified: Set(record.email_verified),
                email_verification_token: Set(record.email_verification_token.clone()),
                role: Set(record.role.clone()),
                status: Set(record.status.clone()),
                created_at: Set(record.created_at),
                last_login_at: Set(record.last_login_at),
            };
            users::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert user into PostgreSQL")?;
        }
        info!("✓ Migrated {} users", records.len());
    }

    Ok(())
}

async fn migrate_workspaces(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::workspaces;

    info!("Migrating workspaces...");

    let records = workspaces::Entity::find()
        .order_by_asc(workspaces::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch workspaces from SQLite")?;

    info!("Found {} workspace records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = workspaces::ActiveModel {
                id: Set(record.id),
                name: Set(record.name.clone()),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            workspaces::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert workspace into PostgreSQL")?;
        }
        info!("✓ Migrated {} workspaces", records.len());
    }

    Ok(())
}

async fn migrate_git_namespaces(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::git_namespaces;

    info!("Migrating git_namespaces...");

    let records = git_namespaces::Entity::find()
        .all(sqlite)
        .await
        .context("Failed to fetch git_namespaces from SQLite")?;

    info!("Found {} git_namespace records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = git_namespaces::ActiveModel {
                id: Set(record.id),
                installation_id: Set(record.installation_id),
                name: Set(record.name.clone()),
                owner_type: Set(record.owner_type.clone()),
                provider: Set(record.provider.clone()),
                slug: Set(record.slug.clone()),
                user_id: Set(record.user_id),
                oauth_token: Set(record.oauth_token.clone()),
            };
            git_namespaces::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert git_namespace into PostgreSQL")?;
        }
        info!("✓ Migrated {} git_namespaces", records.len());
    }

    Ok(())
}

async fn migrate_workspace_users(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::workspace_users;

    info!("Migrating workspace_users...");

    let records = workspace_users::Entity::find()
        .order_by_asc(workspace_users::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch workspace_users from SQLite")?;

    info!("Found {} workspace_user records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = workspace_users::ActiveModel {
                id: Set(record.id),
                workspace_id: Set(record.workspace_id),
                user_id: Set(record.user_id),
                role: Set(record.role.clone()),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            workspace_users::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert workspace_user into PostgreSQL")?;
        }
        info!("✓ Migrated {} workspace_users", records.len());
    }

    Ok(())
}

async fn migrate_project_repos(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::project_repos;

    info!("Migrating project_repos...");

    let records = project_repos::Entity::find()
        .order_by_asc(project_repos::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch project_repos from SQLite")?;

    info!("Found {} project_repo records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = project_repos::ActiveModel {
                id: Set(record.id),
                repo_id: Set(record.repo_id.clone()),
                git_namespace_id: Set(record.git_namespace_id),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            project_repos::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert project_repo into PostgreSQL")?;
        }
        info!("✓ Migrated {} project_repos", records.len());
    }

    Ok(())
}

async fn migrate_projects(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::projects;

    info!("Migrating projects...");

    let records = projects::Entity::find()
        .order_by_asc(projects::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch projects from SQLite")?;

    info!("Found {} project records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = projects::ActiveModel {
                id: Set(record.id),
                name: Set(record.name.clone()),
                workspace_id: Set(record.workspace_id),
                project_repo_id: Set(record.project_repo_id),
                active_branch_id: Set(record.active_branch_id),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            projects::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert project into PostgreSQL")?;
        }
        info!("✓ Migrated {} projects", records.len());
    }

    Ok(())
}

async fn migrate_branches(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::branches;

    info!("Migrating branches...");

    let records = branches::Entity::find()
        .order_by_asc(branches::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch branches from SQLite")?;

    info!("Found {} branch records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = branches::ActiveModel {
                id: Set(record.id),
                project_id: Set(record.project_id),
                name: Set(record.name.clone()),
                revision: Set(record.revision.clone()),
                sync_status: Set(record.sync_status.clone()),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            branches::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert branch into PostgreSQL")?;
        }
        info!("✓ Migrated {} branches", records.len());
    }

    Ok(())
}

async fn migrate_threads(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::threads;

    info!("Migrating threads...");

    let records = threads::Entity::find()
        .order_by_asc(threads::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch threads from SQLite")?;

    info!("Found {} thread records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = threads::ActiveModel {
                id: Set(record.id),
                title: Set(record.title.clone()),
                input: Set(record.input.clone()),
                output: Set(record.output.clone()),
                source: Set(record.source.clone()),
                created_at: Set(record.created_at),
                references: Set(record.references.clone()),
                source_type: Set(record.source_type.clone()),
                user_id: Set(record.user_id),
                project_id: Set(record.project_id),
                is_processing: Set(record.is_processing),
            };
            threads::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert thread into PostgreSQL")?;
        }
        info!("✓ Migrated {} threads", records.len());
    }

    Ok(())
}

async fn migrate_secrets(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::secrets;

    info!("Migrating secrets...");

    let records = secrets::Entity::find()
        .order_by_asc(secrets::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch secrets from SQLite")?;

    info!("Found {} secret records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = secrets::ActiveModel {
                id: Set(record.id),
                name: Set(record.name.clone()),
                encrypted_value: Set(record.encrypted_value.clone()),
                description: Set(record.description.clone()),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
                created_by: Set(record.created_by),
                is_active: Set(record.is_active),
                project_id: Set(record.project_id),
            };
            secrets::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert secret into PostgreSQL")?;
        }
        info!("✓ Migrated {} secrets", records.len());
    }

    Ok(())
}

async fn migrate_api_keys(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::api_keys;

    info!("Migrating api_keys...");

    let records = api_keys::Entity::find()
        .order_by_asc(api_keys::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch api_keys from SQLite")?;

    info!("Found {} api_key records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = api_keys::ActiveModel {
                id: Set(record.id),
                user_id: Set(record.user_id),
                key_hash: Set(record.key_hash.clone()),
                name: Set(record.name.clone()),
                expires_at: Set(record.expires_at),
                last_used_at: Set(record.last_used_at),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
                is_active: Set(record.is_active),
                project_id: Set(record.project_id),
            };
            api_keys::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert api_key into PostgreSQL")?;
        }
        info!("✓ Migrated {} api_keys", records.len());
    }

    Ok(())
}

async fn migrate_messages(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::messages;

    info!("Migrating messages...");

    let records = messages::Entity::find()
        .order_by_asc(messages::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch messages from SQLite")?;

    info!("Found {} message records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = messages::ActiveModel {
                id: Set(record.id),
                content: Set(record.content.clone()),
                is_human: Set(record.is_human),
                thread_id: Set(record.thread_id),
                created_at: Set(record.created_at),
                input_tokens: Set(record.input_tokens),
                output_tokens: Set(record.output_tokens),
            };
            messages::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert message into PostgreSQL")?;
        }
        info!("✓ Migrated {} messages", records.len());
    }

    Ok(())
}

async fn migrate_artifacts(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::artifacts;

    info!("Migrating artifacts...");

    let records = artifacts::Entity::find()
        .all(sqlite)
        .await
        .context("Failed to fetch artifacts from SQLite")?;

    info!("Found {} artifact records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = artifacts::ActiveModel {
                id: Set(record.id),
                content: Set(record.content.clone()),
                kind: Set(record.kind.clone()),
                message_id: Set(record.message_id),
                thread_id: Set(record.thread_id),
                created_at: Set(record.created_at),
            };
            artifacts::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert artifact into PostgreSQL")?;
        }
        info!("✓ Migrated {} artifacts", records.len());
    }

    Ok(())
}

async fn migrate_logs(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::logs;

    info!("Migrating logs...");

    let records = logs::Entity::find()
        .order_by_asc(logs::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch logs from SQLite")?;

    info!("Found {} log records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = logs::ActiveModel {
                id: Set(record.id),
                user_id: Set(record.user_id),
                prompts: Set(record.prompts.clone()),
                thread_id: Set(record.thread_id),
                log: Set(record.log.clone()),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            logs::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert log into PostgreSQL")?;
        }
        info!("✓ Migrated {} logs", records.len());
    }

    Ok(())
}

async fn migrate_runs(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::runs;

    info!("Migrating runs...");

    let records = runs::Entity::find()
        .order_by_asc(runs::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch runs from SQLite")?;

    info!("Found {} run records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = runs::ActiveModel {
                id: Set(record.id),
                source_id: Set(record.source_id.clone()),
                run_index: Set(record.run_index),
                root_source_id: Set(record.root_source_id.clone()),
                root_run_index: Set(record.root_run_index),
                root_replay_ref: Set(record.root_replay_ref.clone()),
                metadata: Set(record.metadata.clone()),
                children: Set(record.children.clone()),
                blocks: Set(record.blocks.clone()),
                variables: Set(record.variables.clone()),
                output: Set(record.output.clone()),
                error: Set(record.error.clone()),
                project_id: Set(record.project_id),
                branch_id: Set(record.branch_id),
                lookup_id: Set(record.lookup_id.clone()),
                user_id: Set(record.user_id),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            runs::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert run into PostgreSQL")?;
        }
        info!("✓ Migrated {} runs", records.len());
    }

    Ok(())
}

async fn migrate_checkpoints(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::checkpoints;

    info!("Migrating checkpoints...");

    let records = checkpoints::Entity::find()
        .order_by_asc(checkpoints::Column::CreatedAt)
        .all(sqlite)
        .await
        .context("Failed to fetch checkpoints from SQLite")?;

    info!("Found {} checkpoint records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = checkpoints::ActiveModel {
                id: Set(record.id),
                run_id: Set(record.run_id),
                replay_id: Set(record.replay_id.clone()),
                checkpoint_hash: Set(record.checkpoint_hash.clone()),
                output: Set(record.output.clone()),
                events: Set(record.events.clone()),
                child_run_info: Set(record.child_run_info.clone()),
                loop_values: Set(record.loop_values.clone()),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            checkpoints::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert checkpoint into PostgreSQL")?;
        }
        info!("✓ Migrated {} checkpoints", records.len());
    }

    Ok(())
}

async fn migrate_settings(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::settings;

    info!("Migrating settings...");

    let records = settings::Entity::find()
        .all(sqlite)
        .await
        .context("Failed to fetch settings from SQLite")?;

    info!("Found {} setting records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = settings::ActiveModel {
                id: Set(record.id),
                github_token: Set(record.github_token.clone()),
                selected_repo_id: Set(record.selected_repo_id),
                revision: Set(record.revision.clone()),
                sync_status: Set(record.sync_status.clone()),
                onboarded: Set(record.onboarded),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
            };
            settings::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert setting into PostgreSQL")?;
        }
        info!("✓ Migrated {} settings", records.len());
    }

    Ok(())
}

async fn migrate_tasks(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<()> {
    use entity::tasks;

    info!("Migrating tasks...");

    let records = tasks::Entity::find()
        .all(sqlite)
        .await
        .context("Failed to fetch tasks from SQLite")?;

    info!("Found {} task records", records.len());

    if !dry_run {
        for record in &records {
            let active_model = tasks::ActiveModel {
                id: Set(record.id),
                title: Set(record.title.clone()),
                question: Set(record.question.clone()),
                answer: Set(record.answer.clone()),
                file_path: Set(record.file_path.clone()),
                created_at: Set(record.created_at),
            };
            tasks::Entity::insert(active_model)
                .exec(postgres)
                .await
                .context("Failed to insert task into PostgreSQL")?;
        }
        info!("✓ Migrated {} tasks", records.len());
    }

    Ok(())
}
