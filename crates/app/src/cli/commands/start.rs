use crate::cli::StartArgs;
use crate::cli::commands::serve::start_server_and_web_app;
use oxy::database::docker;
use oxy::state_dir::get_state_dir;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;

/// Start the database and web server
pub async fn start_database_and_server(args: StartArgs) -> Result<(), OxyError> {
    let enterprise = args.serve.enterprise;

    if enterprise {
        println!(
            "{}",
            "=== Starting Oxy Enterprise with Docker PostgreSQL + ClickHouse + OTel ===\n".text()
        );
    } else {
        println!("{}", "=== Starting Oxy with Docker PostgreSQL ===\n".text());
    }

    // 1. Check container runtime availability
    println!("{}", "🔍 Checking container runtime availability...".text());
    docker::check_docker_available().await?;
    println!("{}", "   ✓ Container runtime is available\n".success());

    // 2. Clean up before starting
    if args.clean {
        // --clean: remove containers, volumes, and network (full reset)
        println!(
            "{}",
            "🧹 Full cleanup (containers + volumes + network)...".text()
        );
        docker::clean_all(enterprise).await?;
        println!("{}", "   ✓ Full clean complete\n".success());

        // Also remove the workspaces directory so stale on-disk directories don't
        // conflict with the freshly-emptied database.
        // In local mode there is no managed workspaces root, so skip this step.
        if !args.serve.local {
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
        }
    } else {
        // Always cleanup existing containers for a fresh start
        println!("{}", "🧹 Cleaning up existing containers...".text());
        docker::cleanup_containers().await;
        println!("{}", "   ✓ Containers cleaned\n".success());
    }

    // 3. Start containers (PostgreSQL + ClickHouse in parallel if enterprise)
    let db_url = if enterprise {
        start_all_containers().await?
    } else {
        start_postgres().await?
    };

    // 4. Show helpful Docker commands
    print_docker_tips(enterprise);

    // 5. Set environment variables for the server
    // Safety: This is safe because we're setting variables in single-threaded context
    // before the server starts, and they're only read by our own code
    unsafe {
        std::env::set_var("OXY_DATABASE_URL", &db_url);
    }

    if enterprise {
        unsafe {
            std::env::set_var("OXY_CLICKHOUSE_URL", "http://localhost:8123");
            std::env::set_var("OXY_CLICKHOUSE_USER", "default");
            std::env::set_var("OXY_CLICKHOUSE_PASSWORD", "default");
            std::env::set_var("OXY_CLICKHOUSE_DATABASE", "otel");
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317");
        }
    }

    // 6. Start the web server (runs on host, not in Docker)
    println!("{}", "🚀 Starting Oxy server...".text());
    start_server_and_web_app(args.serve).await?;

    // 7. Cleanup on exit (handled by graceful shutdown in serve.rs)
    Ok(())
}

/// Start only PostgreSQL (non-enterprise mode)
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

/// Start PostgreSQL, ClickHouse, and OTel Collector (enterprise mode)
/// PostgreSQL and ClickHouse start in parallel; OTel starts after ClickHouse is ready.
async fn start_all_containers() -> Result<String, OxyError> {
    println!("{}", "🐳 Starting containers in parallel...".text());
    println!(
        "{}",
        "   PostgreSQL:  oxy-postgres (postgres:18-alpine)".tertiary()
    );
    println!(
        "{}",
        "   ClickHouse:  oxy-clickhouse (clickhouse/clickhouse-server:25.12.5.44)".tertiary()
    );

    // Start PostgreSQL and ClickHouse in parallel
    let (pg_result, ch_result) = tokio::join!(
        docker::start_postgres_container(),
        docker::start_clickhouse_container(),
    );

    // Handle partial failures: if one started but the other failed, stop the successful one
    let db_url = match (&pg_result, &ch_result) {
        (Ok(_), Err(_)) => {
            eprintln!(
                "{}",
                "   ClickHouse failed to start, stopping PostgreSQL...".error()
            );
            let _ = docker::stop_postgres_container().await;
            ch_result?;
            unreachable!()
        }
        (Err(_), Ok(_)) => {
            eprintln!(
                "{}",
                "   PostgreSQL failed to start, stopping ClickHouse...".error()
            );
            let _ = docker::stop_clickhouse_container().await;
            pg_result?;
            unreachable!()
        }
        (Err(_), Err(_)) => {
            // Both failed, return the PostgreSQL error as primary
            pg_result?;
            unreachable!()
        }
        (Ok(url), Ok(_)) => url.clone(),
    };

    println!(
        "{}",
        "   ✓ PostgreSQL and ClickHouse containers started\n".success()
    );

    // Wait for both to be ready in parallel
    println!(
        "{}",
        "⏳ Waiting for PostgreSQL and ClickHouse to be ready...".text()
    );
    let (pg_ready, ch_ready) = tokio::join!(
        docker::wait_for_postgres_ready(docker::POSTGRES_READY_TIMEOUT_SECS),
        docker::wait_for_clickhouse_ready(docker::CLICKHOUSE_READY_TIMEOUT_SECS),
    );

    // If either readiness check fails, stop both containers
    if pg_ready.is_err() || ch_ready.is_err() {
        eprintln!(
            "{}",
            "   Readiness check failed, stopping containers...".error()
        );
        let _ = docker::stop_enterprise_containers().await;
        let _ = docker::stop_postgres_container().await;
        pg_ready?;
        ch_ready?;
    }

    println!("{}", "✓ PostgreSQL ready".success());
    println!(
        "{}",
        "   Connection: postgresql://localhost:15432/oxy".tertiary()
    );
    println!("{}", "✓ ClickHouse ready".success());
    println!(
        "{}",
        "   HTTP: http://localhost:8123, Native: localhost:9000\n".tertiary()
    );

    // Start OTel Collector (depends on ClickHouse being ready)
    println!("{}", "🐳 Starting OTel Collector container...".text());
    println!("{}", "   Container: oxy-otel-collector".tertiary());
    println!(
        "{}",
        "   Image: otel/opentelemetry-collector-contrib:0.144.0".tertiary()
    );
    println!("{}", "   Ports: 4317 (gRPC), 4318 (HTTP)".tertiary());

    if let Err(e) = docker::start_otel_collector_container().await {
        eprintln!(
            "{}",
            "   OTel Collector failed, stopping all containers...".error()
        );
        let _ = docker::stop_enterprise_containers().await;
        let _ = docker::stop_postgres_container().await;
        return Err(e);
    }
    println!("{}", "   ✓ OTel Collector container started\n".success());

    Ok(db_url)
}

fn print_docker_tips(enterprise: bool) {
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
    if enterprise {
        println!(
            "{}",
            "   ClickHouse logs:  docker logs oxy-clickhouse".secondary()
        );
        println!(
            "{}",
            "   OTel logs:        docker logs oxy-otel-collector".secondary()
        );
        println!(
            "{}",
            "   ClickHouse CLI:   docker exec -it oxy-clickhouse clickhouse-client".secondary()
        );
    }
    println!("{}", "   Check status:     oxy status".secondary());
    println!();
}
