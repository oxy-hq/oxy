use crate::cli::StartArgs;
use crate::cli::commands::serve::start_server_and_web_app;
use oxy::database::docker;
use oxy::state_dir::get_state_dir;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;

/// Start the database and web server
pub async fn start_database_and_server(args: StartArgs) -> Result<(), OxyError> {
    println!(
        "{}",
        "=== Starting Oxygen with Docker PostgreSQL ===\n".text()
    );

    // 1. Check container runtime availability
    println!("{}", "🔍 Checking container runtime availability...".text());
    docker::check_docker_available().await?;
    println!("{}", "   ✓ Container runtime is available\n".success());

    // 2. Clean up before starting
    if args.clean {
        // --clean: remove containers and volumes (full reset)
        println!("{}", "🧹 Full cleanup (containers + volumes)...".text());
        docker::clean_all().await?;
        println!("{}", "   ✓ Full clean complete\n".success());

        // Also remove the workspaces directory so stale on-disk directories don't
        // conflict with the freshly-emptied database.
        let projects_root = get_state_dir().join("workspaces");
        if projects_root.exists() {
            println!("🗂️  {} workspaces directory…", "Removing".text());
            std::fs::remove_dir_all(&projects_root).map_err(|e| {
                OxyError::IOError(format!(
                    "Failed to remove workspaces directory '{}': {e}",
                    projects_root.display()
                ))
            })?;
            println!("{}", "   ✓ Workspaces directory removed\n".success());
        }
    } else {
        // Always cleanup existing containers for a fresh start
        println!("{}", "🧹 Cleaning up existing containers...".text());
        docker::cleanup_containers().await;
        println!("{}", "   ✓ Containers cleaned\n".success());
    }

    // 3. Start PostgreSQL container
    let db_url = start_postgres().await?;

    // 3b. If the observability backend is ClickHouse, start its container.
    // No extra container is needed for duckdb, postgres, or when the env var
    // is unset (observability disabled).
    if std::env::var("OXY_OBSERVABILITY_BACKEND").as_deref() == Ok("clickhouse") {
        start_clickhouse().await?;
    }

    // 4. Show helpful Docker commands
    print_docker_tips();

    // 5. Set environment variables for the server + the standalone worker.
    // Both processes need the same OXY_DATABASE_URL. We also force the
    // HTTP server into `--no-workers` mode so the standalone worker task
    // spawned below is the single drain on `agentic_task_queue` — same
    // operational shape as a cloud deploy where HTTP and worker are
    // separate replicas, just collapsed into one local process tree.
    //
    // Safety: the Tokio runtime is running but no task reads either env
    // var until the worker spawn / `start_server_and_web_app` below. No
    // data race can occur because the first readers run strictly after
    // these writes on the same task.
    unsafe {
        std::env::set_var("OXY_DATABASE_URL", &db_url);
        std::env::set_var("OXY_DISABLE_INPROCESS_WORKERS", "1");
    }

    // 6. Connection summary — copy-pasteable for psql / OXY_DATABASE_URL
    print_connection_summary(&db_url);

    // 7. Spawn the standalone worker as a background task. Mirrors the
    // production split (oxy serve --no-workers + oxy worker) without
    // making the operator juggle multiple terminals locally. Worker
    // shutdown rides on tokio runtime teardown when serve exits.
    println!("{}", "🛠  Starting Oxy worker (background task)...".text());
    let worker_handle = tokio::spawn(async {
        let worker_args = super::worker::WorkerArgs {
            // serve already ran migrations; skip them here to avoid the
            // worker racing the HTTP startup on the migrator.
            enterprise: false,
            skip_migrations: true,
            recovery_interval_secs: None,
            health_port: None,
        };
        if let Err(e) = super::worker::run_worker(worker_args).await {
            tracing::error!(error = ?e, "background worker exited with error");
        }
    });

    // 8. Start the web server (runs on host, not in Docker). Worker and
    // serve both watch SIGINT/SIGTERM independently. When the operator
    // Ctrl-C's, each drains on its own (worker waits up to 30s).
    println!("{}", "🚀 Starting Oxygen server...".text());
    let serve_result = start_server_and_web_app(args.serve).await;

    // 9. Wait for the worker to finish its own graceful shutdown so the
    // process doesn't exit mid-drain. Bound the wait so a stuck worker
    // can't keep `oxy start` hanging.
    let drain = tokio::time::Duration::from_secs(35);
    if tokio::time::timeout(drain, worker_handle).await.is_err() {
        tracing::warn!("background worker did not drain within {drain:?}; abandoning");
    }

    serve_result
}

/// Print the Postgres connection URL with username + password so the
/// operator can copy-paste it into psql / a `.env` / another tool.
fn print_connection_summary(db_url: &str) {
    println!("{}", "🔗 Database connection".text());
    println!("{}", "   OXY_DATABASE_URL:".tertiary());
    println!("   {}", db_url.text());
    println!();
}

/// Start the ClickHouse container for the observability backend and set the
/// matching `OXY_CLICKHOUSE_*` env vars so `ClickHouseObservabilityStorage::from_env()`
/// connects to it.
async fn start_clickhouse() -> Result<(), OxyError> {
    println!("{}", "🐳 Starting ClickHouse container...".text());
    println!("{}", "   Container: oxy-clickhouse".tertiary());
    println!(
        "{}",
        format!(
            "   Ports: {}:HTTP, {}:Native",
            docker::CLICKHOUSE_HTTP_PORT,
            docker::CLICKHOUSE_NATIVE_PORT
        )
        .tertiary()
    );
    println!("{}", "   Volume: oxy-clickhouse-data".tertiary());

    docker::start_clickhouse_container().await?;
    println!("{}", "   ✓ ClickHouse container started\n".success());

    println!("{}", "⏳ Waiting for ClickHouse to be ready...".text());
    docker::wait_for_clickhouse_ready(docker::CLICKHOUSE_READY_TIMEOUT_SECS).await?;
    println!("{}", "✓ ClickHouse ready".success());

    // Set env vars so the observability backend connects to the container we just started.
    // Safety: these vars are only read by `observability_boot::finalize` (called from
    // `start_server_and_web_app` below) and the ClickHouse backend it initializes. All
    // readers run strictly after this point on the same task, so no data race exists.
    unsafe {
        std::env::set_var(
            "OXY_CLICKHOUSE_URL",
            format!("http://localhost:{}", docker::CLICKHOUSE_HTTP_PORT),
        );
        std::env::set_var("OXY_CLICKHOUSE_USER", docker::CLICKHOUSE_USER);
        std::env::set_var("OXY_CLICKHOUSE_PASSWORD", docker::CLICKHOUSE_PASSWORD);
        std::env::set_var("OXY_CLICKHOUSE_DATABASE", docker::CLICKHOUSE_DATABASE);
    }

    println!(
        "{}",
        format!(
            "   Connection: http://localhost:{} (user={}, db={})\n",
            docker::CLICKHOUSE_HTTP_PORT,
            docker::CLICKHOUSE_USER,
            docker::CLICKHOUSE_DATABASE
        )
        .tertiary()
    );
    Ok(())
}

/// Start only PostgreSQL
async fn start_postgres() -> Result<String, OxyError> {
    println!("{}", "🐳 Starting PostgreSQL container...".text());
    println!("{}", "   Container: oxy-postgres".tertiary());
    println!("{}", "   Image: postgres:18-alpine".tertiary());
    println!("{}", "   Port: 15432:5432".tertiary());
    println!("{}", "   Volume: oxy-postgres-data".tertiary());

    let db_url = docker::start_postgres_container().await?;
    println!("{}", "   ✓ PostgreSQL container started\n".success());

    println!("{}", "⏳ Waiting for PostgreSQL to be ready...".text());
    docker::wait_for_postgres_ready(docker::POSTGRES_READY_TIMEOUT_SECS).await?;
    println!("{}", "✓ PostgreSQL ready".success());
    println!(
        "{}",
        "   Connection: postgresql://localhost:15432/oxy\n".tertiary()
    );

    Ok(db_url)
}

fn print_docker_tips() {
    println!("{}", "💡 Useful Docker Commands:".text());
    println!(
        "{}",
        "   View logs:        docker logs oxy-postgres".secondary()
    );
    println!(
        "{}",
        "   Follow logs:      docker logs -f oxy-postgres".secondary()
    );
    println!(
        "{}",
        "   Access psql:      docker exec -it oxy-postgres psql -U postgres -d oxy".secondary()
    );
    println!("{}", "   Check status:     oxy status".secondary());
    println!();
}
