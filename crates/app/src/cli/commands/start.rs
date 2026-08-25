use crate::cli::StartArgs;
use crate::cli::commands::serve::start_server_and_web_app;
use oxy::database::docker;
use oxy::state_dir::get_state_dir;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;

pub async fn start_database_and_server(
    args: StartArgs,
    extra_api_routes: axum::Router<crate::server::router::AppState>,
    extra_workspace_routes: axum::Router<crate::server::router::AppState>,
) -> Result<(), OxyError> {
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

    // 3b. Observability is ClickHouse-only; start its container when enabled.
    // When the env var is unset, observability is disabled and no container
    // is needed.
    if std::env::var("OXY_OBSERVABILITY_BACKEND").as_deref()
        == Ok(oxy_observability::backends::BACKEND_CLICKHOUSE)
    {
        start_clickhouse().await?;
    }

    // 4. Show helpful Docker commands
    print_docker_tips();

    // 5. `oxy start` runs a SINGLE all-in-one process that drives its own queue.
    // OXY_ROLE defaults to `all`, so the server's in-process worker + global
    // driver drain `agentic_task_queue` directly — scheduled + manual jobs, ELT,
    // and compiles all execute here.
    //
    // (A previous cut forced the server into `--no-workers` and spawned a
    // standalone `oxy worker` task to "mirror the cloud fleet" — but that worker
    // only runs the reaper; it does not yet drive runs, so NOTHING executed
    // background jobs locally and everything sat queued forever. Single-process
    // is the correct shape until the standalone worker can drive; the real fleet
    // split is exercised via the justfile's two-node recipe. Do NOT re-add
    // OXY_DISABLE_INPROCESS_WORKERS here without a worker that actually drives.)
    //
    // Safety: the Tokio runtime is running but no task reads OXY_DATABASE_URL
    // until `start_server_and_web_app` below, which runs strictly after this
    // write on the same task.
    unsafe {
        std::env::set_var("OXY_DATABASE_URL", &db_url);
    }

    // 6. Connection summary — copy-pasteable for psql / OXY_DATABASE_URL
    print_connection_summary(&db_url);

    // 7. Start the web server (runs on host, not in Docker). Its in-process
    // worker drains the queue; it drains on SIGINT/SIGTERM on its own.
    println!("{}", "🚀 Starting Oxygen server...".text());
    start_server_and_web_app(args.serve, extra_api_routes, extra_workspace_routes).await
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
