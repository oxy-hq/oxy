use crate::cli::ServeArgs;
use crate::server::serve_mode::ServeMode;
use agentic_pipeline::{AnalyticsMigrator, WorkflowMigrator};
use agentic_runtime::migration::RuntimeMigrator;
use axum::handler::Handler;
use axum::http::HeaderValue;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get_service,
};
use include_dir::{Dir, include_dir};
use migration::{Migrator, MigratorTrait};
use oxy::{
    config::{constants::DEFAULT_API_KEY_HEADER, resolve_local_workspace_path},
    database::{client::establish_connection, docker},
    state_dir::get_state_dir,
    theme::StyledText,
};
use oxy_shared::errors::OxyError;
use std::net::SocketAddr;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tower::service_fn;
use tower_http::trace::{self, TraceLayer};
use tower_serve_static::ServeDir;
use tracing::Level;
use utoipa::openapi::security::{
    ApiKey as OApiKey, ApiKeyValue, SecurityRequirement, SecurityScheme,
};
use utoipa::openapi::server::Server;
use utoipa_swagger_ui::SwaggerUi;

#[cfg(target_os = "windows")]
static DIST: Dir = include_dir!("D:\\a\\oxy\\oxy\\crates\\core\\dist");
#[cfg(not(target_os = "windows"))]
static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/dist");
const ASSETS_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

pub async fn start_server_and_web_app(args: ServeArgs) -> Result<(), OxyError> {
    // Require OXY_DATABASE_URL to be set
    if std::env::var("OXY_DATABASE_URL").is_err() {
        return Err(OxyError::RuntimeError(
            "OXY_DATABASE_URL environment variable is required.\n\n\
            Options:\n\
            1. Use 'oxy start' to automatically start PostgreSQL with Docker\n\
            2. Set OXY_DATABASE_URL to your PostgreSQL connection string:\n\
               export OXY_DATABASE_URL=postgresql://user:password@localhost:5432/oxy"
                .to_string(),
        ));
    }

    println!("serve: running database migrations");
    run_database_migrations(args.enterprise).await?;
    println!("serve: migrations done, finding available port");

    // Now that OXY_DATABASE_URL is set (either externally for `oxy serve` or
    // by `oxy start` after booting Postgres), resolve the observability
    // backend and spawn the bridge that drains the span channel into it.
    // No-op when OXY_OBSERVABILITY_BACKEND is unset — the layer was never
    // installed and there's no receiver to drain.
    crate::observability_boot::finalize().await;

    // Detect whether any cloud auth provider is configured — used only to
    // surface an informational log when `--local` is requested with providers
    // present (the providers are ignored in local mode).
    let auth_configured = std::env::var("GOOGLE_CLIENT_ID").is_ok()
        || std::env::var("OKTA_CLIENT_ID").is_ok()
        || std::env::var("MAGIC_LINK_SECRET").is_ok();

    // Ensure `$OXY_STATE_DIR/workspaces/` exists, migrating the legacy
    // "projects" directory on first boot. The canonical on-disk layout is
    // owned by `oxy::adapters::workspace::workspace_root_path`.
    {
        let state = get_state_dir();
        let legacy = state.join("projects");
        let root = state.join("workspaces");
        if legacy.exists()
            && !root.exists()
            && let Err(e) = std::fs::rename(&legacy, &root)
        {
            tracing::warn!(
                "Could not migrate workspaces directory {:?} → {:?}: {}",
                legacy,
                root,
                e
            );
        }
        std::fs::create_dir_all(&root).ok();
    }

    // Retrieve the global observability storage (if initialized) for the API handlers.
    let observability = oxy_observability::global::get_global().cloned();

    let _available_port = find_available_port(args.host.clone(), args.port).await?;
    let mode = if args.local {
        if auth_configured {
            tracing::info!(
                "--local: ignoring configured auth providers — all requests will run as the local guest user"
            );
        }
        match resolve_local_workspace_path() {
            Ok(path) => {
                tracing::info!("Local mode: workspace resolved to {}", path.display());
            }
            Err(e) => {
                tracing::info!(
                    "local mode: no workspace found ({}), waiting for setup via web UI",
                    e
                );
            }
        }
        ServeMode::Local
    } else {
        ServeMode::Cloud
    };
    let shutdown_token = CancellationToken::new();
    let startup_cwd = std::env::current_dir().map_err(|e| {
        OxyError::RuntimeError(format!("Failed to resolve startup working directory: {e}"))
    })?;
    let app = create_web_application(
        mode,
        args.enterprise,
        observability.clone(),
        startup_cwd.clone(),
    )
    .await?;

    let internal_app = if args.internal_port > 0 {
        Some(create_internal_application(args.enterprise, observability).await?)
    } else {
        println!("serve: internal port disabled (internal_port=0)");
        None
    };

    println!("serve: starting application");
    serve_application(app, internal_app, args, shutdown_token).await
}

async fn run_database_migrations(enterprise: bool) -> Result<(), OxyError> {
    println!("migrations: establishing database connection (this builds the connection pool)");
    let db = establish_connection()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to connect to database: {}", e)))?;
    println!("migrations: database connection established, running SeaORM migrations");

    // Run SeaORM migrations for PostgreSQL
    Migrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to run database migrations: {}", e)))?;
    println!("migrations: SeaORM migrations complete");

    // Run orchestrator runtime migrations (separate tracking table).
    RuntimeMigrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("runtime migrations failed: {}", e)))?;
    println!("migrations: runtime migrations complete");

    // Run analytics domain extension migrations (separate tracking table).
    AnalyticsMigrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("analytics migrations failed: {}", e)))?;
    println!("migrations: analytics migrations complete");

    // Run workflow state migrations (separate tracking table).
    WorkflowMigrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("workflow migrations failed: {}", e)))?;
    println!("migrations: workflow migrations complete");

    // Observability schema (DuckDB / Postgres / ClickHouse) is initialized by
    // the backend itself during `*Storage::open()` in `main.rs`, so no separate
    // migration step is needed here.

    Ok(())
}

async fn find_available_port(host: String, port: u16) -> Result<u16, OxyError> {
    let original_web_port = port;
    let mut chosen_port = port;
    let mut port_attempts = 0u16;
    const MAX_PORT_ATTEMPTS: u16 = 100;

    loop {
        let trial = format!("{host}:{chosen_port}");
        match trial.parse::<SocketAddr>() {
            Ok(addr) => {
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(listener) => {
                        // Successfully bound to the port: close listener and use this port
                        drop(listener);
                        break;
                    }
                    Err(e) => {
                        if chosen_port <= 1024 && e.kind() == std::io::ErrorKind::PermissionDenied {
                            eprintln!(
                                "Permission denied binding to port {chosen_port}. Try running with sudo or use a port above 1024."
                            );
                            std::process::exit(1);
                        }
                        port_attempts += 1;
                        if port_attempts > MAX_PORT_ATTEMPTS {
                            eprintln!(
                                "Failed to bind to any port after trying {} ports starting from {}. Error: {}",
                                port_attempts, original_web_port, e
                            );
                            std::process::exit(1);
                        }
                        println!("Port {chosen_port} is occupied. Trying next port...");
                        chosen_port += 1;
                    }
                }
            }
            Err(_) => {
                // If parse fails, fall back to binding to unspecified address
                break;
            }
        }
    }
    Ok(chosen_port)
}

async fn create_web_application(
    mode: ServeMode,
    enterprise: bool,
    observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    startup_cwd: std::path::PathBuf,
) -> Result<Router, OxyError> {
    let api_router =
        crate::server::router::api_router(mode, enterprise, observability, startup_cwd)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create API router: {}", e)))?;
    let openapi_router = crate::server::router::openapi_router().await;
    println!("create_web_application: openapi_router done, assembling final router");
    let mut openapi_doc = openapi_router.into_openapi().clone();

    openapi_doc.info.title = "oxy-api-docs".to_string();
    openapi_doc.info.description = Some("oxy api docs".to_string());
    openapi_doc.info.contact = None;
    openapi_doc.info.license = None;

    let name = "ApiKey".to_string();
    let mut components = openapi_doc.components.take().unwrap_or_default();
    components.security_schemes.insert(
        name.clone(),
        SecurityScheme::ApiKey(OApiKey::Header(ApiKeyValue::new(
            DEFAULT_API_KEY_HEADER.to_string(),
        ))),
    );
    openapi_doc.components = Some(components);
    openapi_doc.security = Some(vec![SecurityRequirement::new(name, Vec::<String>::new())]);
    openapi_doc.servers = Some(vec![Server::new("/api")]);
    let static_service = service_fn(handle_static_files);

    let router = Router::new()
        .nest("/api", api_router)
        .merge(
            SwaggerUi::new("/apidoc")
                .url("/apidoc/openapi.json", openapi_doc)
                .config(
                    utoipa_swagger_ui::Config::new(["/apidoc/openapi.json"])
                        .persist_authorization(true)
                        .deep_linking(true)
                        .display_request_duration(true)
                        .try_it_out_enabled(true),
                ),
        )
        .fallback_service(static_service)
        .layer(create_trace_layer());
    Ok(router)
}

async fn create_internal_application(
    enterprise: bool,
    observability: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
) -> Result<Router, OxyError> {
    let internal_router = crate::server::router::internal_api_router(enterprise, observability)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create internal API router: {}", e))
        })?;

    let static_service = service_fn(handle_static_files);

    Ok(Router::new()
        .nest("/api", internal_router)
        .fallback_service(static_service)
        .layer(create_trace_layer()))
}

fn create_trace_layer()
-> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
        .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
        .on_request(trace::DefaultOnRequest::new().level(Level::DEBUG))
        .on_response(
            trace::DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(tower_http::LatencyUnit::Millis),
        )
        .on_failure(trace::DefaultOnFailure::new().level(Level::ERROR))
}

async fn handle_static_files(
    req: Request<Body>,
) -> Result<axum::response::Response, std::convert::Infallible> {
    let uri = req.uri().clone();
    let mut response = get_service(ServeDir::new(&DIST))
        .call(req, None::<()>)
        .await;

    if uri.path().starts_with("/assets/") {
        response.headers_mut().insert(
            "Cache-Control",
            HeaderValue::from_static(ASSETS_CACHE_CONTROL),
        );
    }

    if response.status() == StatusCode::NOT_FOUND {
        let index_request = Request::builder()
            .uri("/index.html")
            .body(Body::empty())
            .unwrap();
        let response = get_service(ServeDir::new(&DIST))
            .call(index_request, None::<()>)
            .await;

        return Ok(response);
    }

    Ok(response)
}

async fn serve_application(
    app: Router,
    internal_app: Option<Router>,
    args: ServeArgs,
    shutdown_token: CancellationToken,
) -> Result<(), OxyError> {
    let socket_addr = format!("{}:{}", args.host, args.port)
        .parse()
        .or_else(|_| Ok(SocketAddr::from(([0, 0, 0, 0], args.port))))
        .map_err(|e: std::net::AddrParseError| {
            OxyError::RuntimeError(format!("Invalid address: {}", e))
        })?;

    let display_host = if args.host == "0.0.0.0" {
        "localhost"
    } else {
        &args.host
    };

    let protocol = if args.http2_only { "https" } else { "http" };
    let protocol_info = if args.http2_only {
        " (HTTP/2 only)"
    } else {
        " (HTTP/1.1+HTTP/2)"
    };
    println!(
        "{} {}{}",
        "Web app running at".text(),
        format!("{}://{}:{}", protocol, display_host, args.port).secondary(),
        protocol_info
    );

    // Start internal server if enabled
    if let Some(internal_app) = internal_app {
        let internal_addr: SocketAddr = format!("{}:{}", args.internal_host, args.internal_port)
            .parse()
            .map_err(|e: std::net::AddrParseError| {
                OxyError::RuntimeError(format!("Invalid internal address: {}", e))
            })?;

        let internal_display_host = if args.internal_host == "0.0.0.0" {
            "localhost"
        } else {
            &args.internal_host
        };
        println!(
            "{} {}",
            "Internal API running at".text(),
            format!("http://{}:{}", internal_display_host, args.internal_port).secondary(),
        );

        let internal_listener =
            tokio::net::TcpListener::bind(internal_addr)
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to bind internal server to {}: {}",
                        internal_addr, e
                    ))
                })?;

        tokio::spawn(async move {
            if let Err(e) = axum::serve(internal_listener, internal_app).await {
                tracing::error!("Internal server error: {}", e);
            }
        });
    }

    if args.http2_only {
        // If TLS cert/key files exist, use HTTPS+HTTP/2
        let cert_exists = std::path::Path::new(&args.tls_cert).exists();
        let key_exists = std::path::Path::new(&args.tls_key).exists();
        let config = if cert_exists && key_exists {
            tracing::info!("Using provided TLS cert/key files for HTTPS (TLS) and HTTP/2");
            match axum_server::tls_rustls::RustlsConfig::from_pem_file(
                &args.tls_cert,
                &args.tls_key,
            )
            .await
            {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("Failed to load TLS cert/key: {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            tracing::warn!("No TLS cert/key files found, using bundled default cert/key.");
            let default_cert: &[u8] = include_bytes!("../../../../../localhost+2.pem");
            let default_key: &[u8] = include_bytes!("../../../../../localhost+2-key.pem");
            match axum_server::tls_rustls::RustlsConfig::from_pem(
                default_cert.to_vec(),
                default_key.to_vec(),
            )
            .await
            {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("Failed to load bundled TLS cert/key: {}", e);
                    std::process::exit(1);
                }
            }
        };

        // Create handle for graceful shutdown with axum_server
        let handle = axum_server::Handle::new();

        // Spawn shutdown signal handler
        let shutdown_handle = handle.clone();
        let token = shutdown_token;
        tokio::spawn(async move {
            create_shutdown_signal().await;
            tracing::info!("Shutdown signal received, stopping server...");
            token.cancel();
            shutdown_handle.shutdown();
        });

        axum_server::bind_rustls(socket_addr, config)
            .handle(handle)
            .serve(app.into_make_service())
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Server error: {}", e)))
    } else {
        let listener = tokio::net::TcpListener::bind(socket_addr)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to bind to address: {}", e)))?;

        let shutdown = async move {
            create_shutdown_signal().await;
            shutdown_token.cancel();
        };

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Server error: {}", e)))
    }
}

async fn create_shutdown_signal() {
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
        _ = ctrl_c => {
            tracing::info!("Received shutdown signal, cleaning up...");
        },
        _ = terminate => {
            tracing::info!("Received termination signal, cleaning up...");
        },
    }

    // If the user presses Ctrl+C again while graceful shutdown is in progress,
    // force-exit immediately instead of showing ^C and hanging.
    tokio::spawn(async {
        signal::ctrl_c()
            .await
            .expect("failed to install second Ctrl+C handler");
        tracing::warn!("Received second shutdown signal, forcing exit");
        std::process::exit(1);
    });

    // Cleanup Docker containers (stop and remove all oxy-managed containers)
    docker::cleanup_containers().await;
}
