use migration::MigratorTrait;
use oxy::state_dir::get_state_dir;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use sea_orm::{Database, DatabaseConnection, EntityTrait, QueryOrder, Set};
use std::time::Duration;

#[derive(clap::Parser, Debug)]
pub struct MigrateSqliteArgs {
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

fn get_default_sqlite_path() -> String {
    let state_dir = get_state_dir();
    let db_path = state_dir.join("db.sqlite");
    format!("sqlite://{}", db_path.to_string_lossy())
}

/// Wait for a database connection to be ready with retries
async fn wait_for_connection(
    connection_url: &str,
    max_attempts: u32,
) -> Result<DatabaseConnection, OxyError> {
    let mut attempt = 1;
    let mut delay = Duration::from_millis(100);

    loop {
        match Database::connect(connection_url).await {
            Ok(conn) => {
                println!("{}", "  ✓ Successfully connected to database".success());
                return Ok(conn);
            }
            Err(_e) if attempt < max_attempts => {
                println!(
                    "{}",
                    format!(
                        "  Connection attempt {}/{} failed, retrying in {:?}...",
                        attempt, max_attempts, delay
                    )
                    .tertiary()
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
                delay = std::cmp::min(delay * 2, Duration::from_secs(5));
            }
            Err(e) => {
                return Err(OxyError::Database(format!(
                    "Failed to connect to database: {}",
                    e
                )));
            }
        }
    }
}

pub async fn run_migration(args: MigrateSqliteArgs) -> Result<(), OxyError> {
    // Determine source SQLite database
    let from = args.from.unwrap_or_else(|| {
        let default = get_default_sqlite_path();
        println!(
            "{}",
            "No --from specified, using default SQLite location".tertiary()
        );
        default
    });

    let to = args.to;

    println!(
        "{}",
        "=== Starting migration from SQLite to PostgreSQL ===\n".primary()
    );
    println!("{}", format!("Source: {}", from).text());
    println!("{}", format!("Target: {}", to).text());
    println!();
    println!(
        "{}",
        "NOTE: Make sure PostgreSQL is running before starting migration.".tertiary()
    );
    println!("{}", "      You can start it with: oxy start".tertiary());
    println!();

    if args.dry_run {
        println!("{}", "DRY RUN MODE - no data will be written".warning());
        println!();
    }

    // Connect to both databases with retries
    println!("{}", "Connecting to SQLite database...".text());
    let sqlite = wait_for_connection(&from, 3).await?;

    println!("{}", "Connecting to PostgreSQL database...".text());
    let postgres = wait_for_connection(&to, 20).await?;

    // Run migrations on PostgreSQL first
    println!("{}", "Running migrations on PostgreSQL...".text());
    migration::Migrator::up(&postgres, None)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to run migrations on PostgreSQL: {}", e))
        })?;
    println!("{}", "  ✓ Migrations completed\n".success());

    // Migrate data for each entity in dependency order
    // Order matters due to foreign key constraints!

    println!("{}", "=== Starting data migration ===\n".primary());

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

    println!();
    println!("{}", "=== Migration completed successfully! ===".success());
    println!();

    Ok(())
}

async fn migrate_users(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::users;

    println!("{}", format!("Migrating {}...", "users").text());

    let records = users::Entity::find()
        .order_by_asc(users::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch users from SQLite: {}", e)))?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to insert user into PostgreSQL: {}", e))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} users", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_workspaces(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::workspaces;

    println!("{}", format!("Migrating {}...", "workspaces").text());

    let records = workspaces::Entity::find()
        .order_by_asc(workspaces::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch workspaces from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert workspace into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} workspaces", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_git_namespaces(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::git_namespaces;

    println!("{}", format!("Migrating {}...", "git_namespaces").text());

    let records = git_namespaces::Entity::find()
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch git_namespaces from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert git_namespace into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} git_namespaces", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_workspace_users(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::workspace_users;

    println!("{}", format!("Migrating {}...", "workspace_users").text());

    let records = workspace_users::Entity::find()
        .order_by_asc(workspace_users::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to fetch workspace_users from SQLite: {}",
                e
            ))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert workspace_user into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} workspace_users", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_project_repos(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::project_repos;

    println!("{}", format!("Migrating {}...", "project_repos").text());

    let records = project_repos::Entity::find()
        .order_by_asc(project_repos::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch project_repos from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert project_repo into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} project_repos", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_projects(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::projects;

    println!("{}", format!("Migrating {}...", "projects").text());

    let records = projects::Entity::find()
        .order_by_asc(projects::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch projects from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert project into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} projects", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_branches(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::branches;

    println!("{}", format!("Migrating {}...", "branches").text());

    let records = branches::Entity::find()
        .order_by_asc(branches::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch branches from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert branch into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} branches", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_threads(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::threads;

    println!("{}", format!("Migrating {}...", "threads").text());

    let records = threads::Entity::find()
        .order_by_asc(threads::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch threads from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                sandbox_info: Set(None),
            };
            threads::Entity::insert(active_model)
                .exec(postgres)
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert thread into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} threads", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_secrets(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::secrets;

    println!("{}", format!("Migrating {}...", "secrets").text());

    let records = secrets::Entity::find()
        .order_by_asc(secrets::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch secrets from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert secret into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} secrets", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_api_keys(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::api_keys;

    println!("{}", format!("Migrating {}...", "api_keys").text());

    let records = api_keys::Entity::find()
        .order_by_asc(api_keys::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch api_keys from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert api_key into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} api_keys", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_messages(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::messages;

    println!("{}", format!("Migrating {}...", "messages").text());

    let records = messages::Entity::find()
        .order_by_asc(messages::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch messages from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert message into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} messages", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_artifacts(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::artifacts;

    println!("{}", format!("Migrating {}...", "artifacts").text());

    let records = artifacts::Entity::find().all(sqlite).await.map_err(|e| {
        OxyError::RuntimeError(format!("Failed to fetch artifacts from SQLite: {}", e))
    })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert artifact into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} artifacts", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_logs(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::logs;

    println!("{}", format!("Migrating {}...", "logs").text());

    let records = logs::Entity::find()
        .order_by_asc(logs::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch logs from SQLite: {}", e)))?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to insert log into PostgreSQL: {}", e))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} logs", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_runs(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::runs;

    println!("{}", format!("Migrating {}...", "runs").text());

    let records = runs::Entity::find()
        .order_by_asc(runs::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch runs from SQLite: {}", e)))?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                lookup_id: Set(record.lookup_id),
                created_at: Set(record.created_at),
                updated_at: Set(record.updated_at),
                user_id: Set(None), // Migrated runs have no user association
            };
            runs::Entity::insert(active_model)
                .exec(postgres)
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to insert run into PostgreSQL: {}", e))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} runs", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_checkpoints(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::checkpoints;

    println!("{}", format!("Migrating {}...", "checkpoints").text());

    let records = checkpoints::Entity::find()
        .order_by_asc(checkpoints::Column::CreatedAt)
        .all(sqlite)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to fetch checkpoints from SQLite: {}", e))
        })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert checkpoint into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} checkpoints", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_settings(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::settings;

    println!("{}", format!("Migrating {}...", "settings").text());

    let records = settings::Entity::find().all(sqlite).await.map_err(|e| {
        OxyError::RuntimeError(format!("Failed to fetch settings from SQLite: {}", e))
    })?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to insert setting into PostgreSQL: {}",
                        e
                    ))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} settings", records.len()).success()
        );
    }

    Ok(())
}

async fn migrate_tasks(
    sqlite: &DatabaseConnection,
    postgres: &DatabaseConnection,
    dry_run: bool,
) -> Result<(), OxyError> {
    use entity::tasks;

    println!("{}", format!("Migrating {}...", "tasks").text());

    let records = tasks::Entity::find()
        .all(sqlite)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to fetch tasks from SQLite: {}", e)))?;

    println!(
        "{}",
        format!("  Found {} records", records.len()).tertiary()
    );

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
                .map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to insert task into PostgreSQL: {}", e))
                })?;
        }
        println!(
            "{}",
            format!("  ✓ Migrated {} tasks", records.len()).success()
        );
    }

    Ok(())
}
