//! A2A (Agent-to-Agent) Protocol Server CLI
//!
//! This module implements the `oxy a2a` command which starts a dedicated
//! A2A protocol server exposing configured Oxy agents for external agent communication.

use crate::cli::A2aArgs;
use crate::integrations::a2a::{
    agent_card::AgentCardService, handler::OxyA2aHandler, storage::OxyTaskStorage,
};
use a2a::server::{create_http_router, create_jsonrpc_router};
use axum::{
    Router,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
};
use migration::{Migrator, MigratorTrait};
use oxy::adapters::project::builder::ProjectBuilder;
use oxy::config::a2a_config::A2aConfig;
use oxy::database::client::establish_connection;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime};
use tokio::signal;
use tower_http::{compression::CompressionLayer, timeout::TimeoutLayer, trace::TraceLayer};
use tracing::{error, info};
use uuid::Uuid;

/// Start the A2A protocol server
///
/// This function:
/// 1. Validates A2A configuration
/// 2. Runs database migrations
/// 3. Creates handler instances per configured agent
/// 4. Mounts routers at `/a2a/agents/{name}/v1`
/// 5. Adds agent card endpoints per agent
/// 6. Starts the HTTP server
///
/// Configuration precedence (highest to lowest):
/// 1. Command-line arguments (--port, --host, --base-url, --log-level)
/// 2. Environment variables (OXY_A2A_PORT, OXY_PORT, OXY_A2A_HOST, OXY_HOST, OXY_A2A_BASE_URL, OXY_LOG_LEVEL)
/// 3. Configuration file values (config.yml)
/// 4. Built-in defaults (port: 8080, host: 0.0.0.0, base_url: http://localhost:8080, log_level: info)
pub async fn start_a2a_server(args: A2aArgs) -> Result<(), OxyError> {
    info!("Starting A2A server...");

    // Run database migrations
    run_database_migrations().await?;

    // Load configuration from current directory
    let project_path = std::env::current_dir()
        .map_err(|e| OxyError::RuntimeError(format!("Failed to get current directory: {}", e)))?;

    // Build project manager (which includes config manager)
    let project_manager = Arc::new(
        ProjectBuilder::new(Uuid::nil())
            .with_project_path(&project_path)
            .await?
            .with_secrets_manager(oxy::adapters::secrets::SecretsManager::from_environment()?)
            .build()
            .await?,
    );

    let config = project_manager.config_manager.get_config();

    // Check if A2A is configured
    let a2a_config = config
        .a2a
        .as_ref()
        .ok_or_else(|| OxyError::RuntimeError("A2A configuration not found in config.yml. Please add an 'a2a' section with agents to expose.".to_string()))?;

    if a2a_config.agents.is_empty() {
        return Err(OxyError::RuntimeError(
            "No agents configured in A2A section. Please add at least one agent to expose."
                .to_string(),
        ));
    }

    info!(
        "Found {} agent(s) configured for A2A",
        a2a_config.agents.len()
    );

    // Resolve final configuration values with precedence:
    // 1. Command-line arguments (highest priority)
    // 2. Environment variables
    // 3. Configuration file values (TODO: add to config schema)
    // 4. Built-in defaults (lowest priority)

    let port = args
        .port
        .or_else(|| {
            std::env::var("OXY_A2A_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .or_else(|| std::env::var("OXY_PORT").ok().and_then(|s| s.parse().ok()))
        .unwrap_or(8080);

    let host = args
        .host
        .or_else(|| std::env::var("OXY_A2A_HOST").ok())
        .or_else(|| std::env::var("OXY_HOST").ok())
        .unwrap_or_else(|| "0.0.0.0".to_string());

    let base_url = args
        .base_url
        .or_else(|| std::env::var("OXY_A2A_BASE_URL").ok())
        .unwrap_or_else(|| format!("http://{}:{}", host, port));

    // Create resolved args for passing to functions
    let resolved_args = ResolvedA2aArgs {
        port,
        host,
        base_url: base_url.clone(),
    };

    // Initialize server metrics
    let metrics = ServerMetrics::new();
    info!("Server metrics initialized");

    // Create the application with all routes
    let app = create_a2a_application(
        a2a_config.clone(),
        project_manager.clone(),
        base_url,
        metrics.clone(),
    )
    .await?;

    // Serve the application
    serve_a2a_application(app, resolved_args, a2a_config).await
}

/// Resolved A2A configuration arguments
struct ResolvedA2aArgs {
    port: u16,
    host: String,
    base_url: String,
}

/// Server metrics for monitoring
#[derive(Clone)]
struct ServerMetrics {
    requests_total: Arc<AtomicU64>,
    requests_failed: Arc<AtomicU64>,
    start_time: SystemTime,
}

impl ServerMetrics {
    fn new() -> Self {
        Self {
            requests_total: Arc::new(AtomicU64::new(0)),
            requests_failed: Arc::new(AtomicU64::new(0)),
            start_time: SystemTime::now(),
        }
    }

    fn increment_requests(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_failures(&self) {
        self.requests_failed.fetch_add(1, Ordering::Relaxed);
    }

    fn get_requests_total(&self) -> u64 {
        self.requests_total.load(Ordering::Relaxed)
    }

    fn get_requests_failed(&self) -> u64 {
        self.requests_failed.load(Ordering::Relaxed)
    }

    fn get_uptime_seconds(&self) -> u64 {
        self.start_time
            .elapsed()
            .unwrap_or(Duration::from_secs(0))
            .as_secs()
    }
}

/// Run database migrations
async fn run_database_migrations() -> Result<(), OxyError> {
    let db = establish_connection()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to connect to database: {}", e)))?;

    Migrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to run database migrations: {}", e)))
}

/// Create the A2A application with all routes
async fn create_a2a_application(
    a2a_config: A2aConfig,
    project_manager: Arc<oxy::adapters::project::manager::ProjectManager>,
    base_url: String,
    metrics: ServerMetrics,
) -> Result<Router, OxyError> {
    let mut router = Router::new();

    // Get database connection
    let db =
        Arc::new(establish_connection().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to connect to database: {}", e))
        })?);

    // Get config from project manager
    let config = Arc::new(project_manager.config_manager.get_config().clone());

    // Create agent card service
    let agent_card_service = Arc::new(AgentCardService::new(
        config.clone(),
        project_manager.clone(),
    ));

    // Iterate through configured agents and create routes
    for agent_config in &a2a_config.agents {
        let agent_name = &agent_config.name;
        info!("Setting up A2A routes for agent: {}", agent_name);

        // Create agent-scoped storage
        let storage = Arc::new(OxyTaskStorage::new_for_agent(
            agent_name.clone(),
            db.clone(),
        ));

        // Create agent-scoped handler
        let handler = Arc::new(OxyA2aHandler::new(
            agent_name.clone(),
            config.clone(),
            storage,
            db.clone(),
            project_manager.clone(),
            agent_card_service.clone(),
            base_url.clone(),
        ));

        // Get routers from a2a crate (agent-agnostic)
        let jsonrpc_router = create_jsonrpc_router(handler.clone());
        let http_router = create_http_router(handler.clone());

        // Mount at agent-specific path: /a2a/agents/{name}/v1
        let agent_router = jsonrpc_router.merge(http_router);
        router = router.nest(&format!("/a2a/agents/{}/v1", agent_name), agent_router);

        // Mount agent card endpoint: /a2a/agents/{name}/.well-known/agent-card.json
        let agent_card_service_clone = agent_card_service.clone();
        let agent_name_clone = agent_name.clone();
        let base_url_clone = base_url.clone();
        router = router.route(
            &format!("/a2a/agents/{}/.well-known/agent-card.json", agent_name),
            get({
                move || async move {
                    match agent_card_service_clone
                        .get_agent_card(&agent_name_clone, &base_url_clone)
                        .await
                    {
                        Ok(card) => (StatusCode::OK, Json(card)).into_response(),
                        Err(e) => {
                            error!("Failed to get agent card: {:?}", e);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({
                                    "error": "Failed to generate agent card"
                                })),
                            )
                                .into_response()
                        }
                    }
                }
            }),
        );
    }

    // Add discovery endpoint (optional, non-standard) - capture a2a_config in closure
    let a2a_config_for_discovery = a2a_config.clone();
    router = router.route(
        "/a2a/agents",
        get(move || list_agents_handler(a2a_config_for_discovery.clone())),
    );

    // Add health check endpoint with metrics
    let metrics_for_health = metrics.clone();
    let db_clone = db.clone();
    router = router.route(
        "/health",
        get(move || health_check_handler(metrics_for_health.clone(), db_clone.clone())),
    );

    // Add metrics endpoint
    let metrics_for_endpoint = metrics.clone();
    router = router.route(
        "/metrics",
        get(move || metrics_handler(metrics_for_endpoint.clone())),
    );

    // Add middleware layers
    let router = router
        .layer(TraceLayer::new_for_http())
        .layer({
            #[allow(deprecated)]
            TimeoutLayer::new(Duration::from_secs(30))
        })
        .layer(CompressionLayer::new());

    Ok(router)
}

/// List available agents (non-standard discovery endpoint)
async fn list_agents_handler(a2a_config: A2aConfig) -> Json<AgentListResponse> {
    let agents: Vec<AgentInfo> = a2a_config
        .agents
        .iter()
        .map(|agent| AgentInfo {
            name: agent.name.clone(),
            agent_card_url: format!("/a2a/agents/{}/.well-known/agent-card.json", agent.name),
            jsonrpc_endpoint: format!("/a2a/agents/{}/v1/jsonrpc", agent.name),
            http_endpoint: format!("/a2a/agents/{}/v1", agent.name),
        })
        .collect();

    Json(AgentListResponse { agents })
}

/// Health check endpoint with detailed status
async fn health_check_handler(
    metrics: ServerMetrics,
    db: Arc<sea_orm::DatabaseConnection>,
) -> impl IntoResponse {
    // Check database connectivity
    let db_healthy = db.ping().await.is_ok();

    let status = if db_healthy { "healthy" } else { "degraded" };
    let status_code = if db_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(serde_json::json!({
            "status": status,
            "service": "oxy-a2a",
            "uptime_seconds": metrics.get_uptime_seconds(),
            "database": if db_healthy { "connected" } else { "disconnected" },
            "version": env!("CARGO_PKG_VERSION"),
        })),
    )
}

/// Metrics endpoint
async fn metrics_handler(metrics: ServerMetrics) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "requests_total": metrics.get_requests_total(),
            "requests_failed": metrics.get_requests_failed(),
            "uptime_seconds": metrics.get_uptime_seconds(),
        })),
    )
}

#[derive(Serialize)]
struct AgentListResponse {
    agents: Vec<AgentInfo>,
}

#[derive(Serialize)]
struct AgentInfo {
    name: String,
    agent_card_url: String,
    jsonrpc_endpoint: String,
    http_endpoint: String,
}

/// Serve the A2A application
async fn serve_a2a_application(
    app: Router,
    args: ResolvedA2aArgs,
    a2a_config: &A2aConfig,
) -> Result<(), OxyError> {
    let addr = format!("{}:{}", args.host, args.port)
        .parse::<SocketAddr>()
        .map_err(|e| OxyError::RuntimeError(format!("Invalid host/port: {}", e)))?;

    // Print startup information
    println!("\n{}", "🚀 A2A Server Starting".success());
    println!("{}", format!("   Address: http://{}", addr).info());
    println!("{}", format!("   Base URL: {}", args.base_url).info());
    println!("\n{}", "📋 Exposed Agents:".success());
    for agent in &a2a_config.agents {
        println!(
            "{}",
            format!("   • {} (ref: {})", agent.name, agent.r#ref).info()
        );
        println!(
            "{}",
            format!(
                "     JSON-RPC: {}/a2a/agents/{}/v1/jsonrpc",
                args.base_url, agent.name
            )
            .info()
        );
        println!(
            "{}",
            format!("     HTTP: {}/a2a/agents/{}/v1", args.base_url, agent.name).info()
        );
        println!(
            "{}",
            format!(
                "     Agent Card: {}/a2a/agents/{}/.well-known/agent-card.json",
                args.base_url, agent.name
            )
            .info()
        );
    }
    println!("\n{}", "📊 Monitoring Endpoints:".success());
    println!(
        "{}",
        format!("   • Health: {}/health", args.base_url).info()
    );
    println!(
        "{}",
        format!("   • Metrics: {}/metrics", args.base_url).info()
    );
    println!(
        "{}",
        format!("   • Agent Discovery: {}/a2a/agents", args.base_url).info()
    );
    println!("\n{}", "Press Ctrl+C to stop the server".info());

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to bind to {}: {}", addr, e)))?;

    info!("A2A server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Server error: {}", e)))?;

    println!("\n{}", "👋 A2A Server stopped".info());
    Ok(())
}

/// Handle graceful shutdown on Ctrl+C
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, starting graceful shutdown");
}
