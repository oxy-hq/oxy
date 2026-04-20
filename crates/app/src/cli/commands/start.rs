use crate::cli::StartArgs;
use crate::cli::commands::serve::start_server_and_web_app;
use oxy::database::docker;
use oxy::state_dir::get_state_dir;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;

/// Start the database and web server
pub async fn start_database_and_server(args: StartArgs) -> Result<(), OxyError> {
    println!("{}", "=== Starting Oxy with Docker PostgreSQL ===\n".text());

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

    // 3b. If --enterprise is on AND the observability backend is ClickHouse,
    // start the ClickHouse container too. For the default (DuckDB/Postgres)
    // backends, no extra container is needed.
    if args.serve.enterprise
        && std::env::var("OXY_OBSERVABILITY_BACKEND").as_deref() == Ok("clickhouse")
    {
        start_clickhouse().await?;
    }

    // 4. Show helpful Docker commands
    print_docker_tips();

    // 5. Set environment variables for the server
    // Safety: This is safe because we're setting variables in single-threaded context
    // before the server starts, and they're only read by our own code
    unsafe {
        std::env::set_var("OXY_DATABASE_URL", &db_url);
    }

    // 6. Start the web server (runs on host, not in Docker)
    println!("{}", "🚀 Starting Oxy server...".text());
    start_server_and_web_app(args.serve).await?;

    // 7. Cleanup on exit (handled by graceful shutdown in serve.rs)
    Ok(())
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
    // Safety: single-threaded context before the server starts.
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
