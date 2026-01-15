use crate::cli::ServeArgs;
use crate::cli::commands::serve::start_server_and_web_app;
use oxy::database::docker;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;

/// Start the database and web server
pub async fn start_database_and_server(serve_args: ServeArgs) -> Result<(), OxyError> {
    println!("{}", "=== Starting Oxy with Docker PostgreSQL ===\n".text());

    // 1. Check Docker availability
    println!("{}", "🔍 Checking Docker availability...".text());
    docker::check_docker_available().await?;
    println!("{}", "   ✓ Docker is available\n".success());

    // 2. Start PostgreSQL container
    println!("{}", "🐳 Starting PostgreSQL container...".text());
    println!("{}", "   Container: oxy-postgres".tertiary());
    println!("{}", "   Image: postgres:18-alpine".tertiary());
    println!("{}", "   Port: 15432:5432".tertiary());
    println!("{}", "   Volume: oxy-postgres-data".tertiary());

    let db_url = docker::start_postgres_container().await?;
    tracing::info!("PostgreSQL container started with connection: {}", db_url);
    println!("{}", "   ✓ Container started\n".success());

    // 3. Wait for database to be ready
    println!("{}", "⏳ Waiting for database to be ready...".text());
    docker::wait_for_postgres_ready(docker::POSTGRES_READY_TIMEOUT_SECS).await?;

    println!("{}", "✓ PostgreSQL ready".success());
    println!(
        "{}",
        "   Connection: postgresql://localhost:15432/oxy\n".tertiary()
    );

    // 4. Show helpful Docker commands
    print_docker_tips();

    // 5. Set OXY_DATABASE_URL to point to Docker PostgreSQL
    // Safety: This is safe because we're setting a variable in single-threaded context
    // before the server starts, and it's only read by our own code
    unsafe {
        std::env::set_var("OXY_DATABASE_URL", &db_url);
    }

    // 6. Start the web server (runs on host, not in Docker)
    println!("{}", "🚀 Starting Oxy server...".text());
    start_server_and_web_app(serve_args).await?;

    // 7. Cleanup on exit (handled by graceful shutdown in serve.rs)
    Ok(())
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
