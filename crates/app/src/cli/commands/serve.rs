use crate::cli::ServeArgs;
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
    storage::{ClickHouseConfig, ClickHouseStorage},
    theme::StyledText,
};
use oxy_project::LocalGitService;
use oxy_shared::errors::OxyError;
use std::net::SocketAddr;
use tokio::signal;
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
    // In local mode the CWD is already the workspace directory — log it for clarity.
    if args.local
        && let Ok(cwd) = std::env::current_dir()
    {
        tracing::info!("Local mode: serving workspace from {}", cwd.display());
    }

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

    // Validate enterprise mode requirements
    if args.enterprise && !ClickHouseConfig::is_configured() {
        return Err(OxyError::RuntimeError(
            "--enterprise flag requires ClickHouse configuration.\n\n\
            Required environment variables:\n\
            - OXY_CLICKHOUSE_URL (e.g., http://localhost:8123)\n\n\
            Optional environment variables:\n\
            - OXY_CLICKHOUSE_USER (default: default)\n\
            - OXY_CLICKHOUSE_PASSWORD\n\
            - OXY_CLICKHOUSE_DATABASE (default: otel)\n\n\
            Options:\n\
            1. Use 'oxy start --enterprise' to automatically start ClickHouse with Docker\n\
            2. Set the environment variables to point to your ClickHouse instance"
                .to_string(),
        ));
    }

    println!("serve: running database migrations");
    run_database_migrations(args.enterprise).await?;
    println!("serve: migrations done, finding available port");

    // Initialize local git when running in multi-workspace / cloud mode.
    // Skipped in local mode: files are already on disk and git is optional.
    if !args.local
        && let Ok(workspace_root) = resolve_local_workspace_path()
    {
        let repo_url = std::env::var("GIT_REPOSITORY_URL").ok();
        let branch = std::env::var("GIT_BRANCH").unwrap_or_else(|_| "main".to_string());
        let token = LocalGitService::get_remote_token().await;
        if let Err(e) = LocalGitService::clone_or_init(
            &workspace_root,
            repo_url.as_deref(),
            &branch,
            token.as_deref(),
        )
        .await
        {
            tracing::warn!("Failed to initialize local git repository: {}", e);
        }
    }

    // Warn when authentication is enabled but OXY_OWNER is not set.
    // In that case every authenticated user is treated as admin, which is
    // correct for single-user installs but almost certainly unintentional
    // for multi-user deployments.
    let auth_configured = std::env::var("GOOGLE_CLIENT_ID").is_ok()
        || std::env::var("OKTA_CLIENT_ID").is_ok()
        || std::env::var("MAGIC_LINK_SECRET").is_ok();
    let oxy_owner = std::env::var("OXY_OWNER").unwrap_or_default();
    if auth_configured && oxy_owner.trim().is_empty() {
        tracing::warn!(
            "Authentication is enabled but OXY_OWNER is not set — \
             every authenticated user will be treated as an admin. \
             Set OXY_OWNER to the email address of the instance owner \
             to restrict admin access."
        );
    }

    // Resolve the workspaces root directory:
    // - Single-workspace mode (--local flag): no workspaces root — server manages exactly one workspace.
    // - Default multi-workspace mode: $OXY_STATE_DIR/workspaces/
    let workspaces_root: Option<std::path::PathBuf> = if args.local {
        None
    } else {
        let state = get_state_dir();
        // Migrate legacy "projects" directory to "workspaces" if it exists and the
        // new name does not yet exist.  This is a one-time, best-effort rename so
        // that existing deployments continue to see their workspaces after upgrade.
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
        Some(root)
    };

    // Scan the workspaces root and register any discovered workspaces in the DB.
    // Best-effort: a failure is logged but does not prevent the server from starting.
    let first_workspace_path: Option<std::path::PathBuf> = if let Some(ref root) = workspaces_root {
        match scan_and_register_projects(root).await {
            Ok(first) => first,
            Err(e) => {
                tracing::warn!("Workspaces root scan failed: {}", e);
                None
            }
        }
    } else {
        // Single-workspace mode: use CWD resolution
        resolve_local_workspace_path().ok()
    };

    let active_workspace_path = std::sync::Arc::new(tokio::sync::RwLock::new(first_workspace_path));
    // Shared across both the public and internal routers so either can update clone/error status.
    let cloning_workspaces = std::sync::Arc::new(std::sync::Mutex::new(
        std::collections::HashSet::<uuid::Uuid>::new(),
    ));
    let errored_workspaces =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
            uuid::Uuid,
            String,
        >::new()));

    let _available_port = find_available_port(args.host.clone(), args.port).await?;
    let app = create_web_application(
        args.enterprise,
        args.readonly,
        workspaces_root.clone(),
        active_workspace_path.clone(),
        cloning_workspaces.clone(),
        errored_workspaces.clone(),
    )
    .await?;

    let internal_app = if args.internal_port > 0 {
        Some(
            create_internal_application(
                args.enterprise,
                args.readonly,
                workspaces_root,
                active_workspace_path,
                cloning_workspaces,
                errored_workspaces,
            )
            .await?,
        )
    } else {
        println!("serve: internal port disabled (internal_port=0)");
        None
    };

    println!("serve: starting application");
    serve_application(app, internal_app, args).await
}

/// Scan `workspaces_root` for subdirectories containing `config.yml` and upsert
/// corresponding records in the `workspaces` table.  Existing DB records whose
/// `path` no longer exists on disk are left untouched (they may have been moved).
/// Returns the path of the most recently opened workspace (by `last_opened_at`),
/// or any discovered workspace if none have been opened yet.
async fn scan_and_register_projects(
    workspaces_root: &std::path::Path,
) -> Result<Option<std::path::PathBuf>, OxyError> {
    use entity::prelude::Workspaces;
    use entity::workspaces;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};

    let db = establish_connection()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("DB connection failed: {}", e)))?;

    let entries = std::fs::read_dir(workspaces_root).map_err(|e| {
        OxyError::RuntimeError(format!(
            "Cannot read workspaces root '{}': {}",
            workspaces_root.display(),
            e
        ))
    })?;

    let workspaces_root_str = workspaces_root.to_string_lossy().to_string();

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() || !dir.join("config.yml").exists() {
            continue;
        }

        let path_str = dir.to_string_lossy().to_string();
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "workspace".to_string());

        // Check if a workspace with this path already exists
        let existing = Workspaces::find()
            .filter(workspaces::Column::Path.eq(path_str.clone()))
            .one(&db)
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("DB query failed for path '{}': {}", path_str, e))
            })?;

        if existing.is_none() {
            // Insert a new workspace record
            let new_workspace = workspaces::ActiveModel {
                id: Set(uuid::Uuid::new_v4()),
                name: Set(name.clone()),
                workspace_id: Set(uuid::Uuid::nil()),
                project_repo_id: Set(None),
                active_branch_id: Set(uuid::Uuid::nil()),
                created_at: Set(chrono::Utc::now().into()),
                updated_at: Set(chrono::Utc::now().into()),
                path: Set(Some(path_str.clone())),
                last_opened_at: Set(None),
                created_by: Set(None),
            };
            new_workspace.insert(&db).await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to insert workspace '{}': {}", path_str, e))
            })?;
            tracing::info!("Registered workspace '{}' at '{}'", name, path_str);
        }
    }

    // Return the most recently opened workspace whose path lives under workspaces_root.
    // Falls back to the first workspace found (arbitrary) if none have been opened yet.
    let most_recent = Workspaces::find()
        // Use "root/%" (with a literal slash before the wildcard) so only paths
        // that are actual children of workspaces_root match.  Without the slash,
        // "root%" would also match sibling directories like "root-extra/…".
        .filter(
            workspaces::Column::Path
                .like(format!("{}/%", workspaces_root_str.trim_end_matches('/'))),
        )
        .order_by_desc(workspaces::Column::LastOpenedAt)
        .one(&db)
        .await
        .map_err(|e| {
            OxyError::RuntimeError(format!("DB query for active workspace failed: {}", e))
        })?;

    Ok(most_recent
        .and_then(|p| p.path)
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists()))
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

    // Run ClickHouse migrations only when enterprise mode is enabled
    if enterprise {
        run_clickhouse_migrations().await?;
    }

    Ok(())
}

async fn run_clickhouse_migrations() -> Result<(), OxyError> {
    tracing::info!("Running ClickHouse migrations for enterprise mode...");

    let storage = ClickHouseStorage::from_env();

    // Run ClickHouse migrations
    storage
        .run_migrations()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("ClickHouse migrations failed: {e}")))?;

    tracing::info!("ClickHouse migrations completed successfully");

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
    enterprise: bool,
    readonly: bool,
    workspaces_root: Option<std::path::PathBuf>,
    active_workspace_path: std::sync::Arc<tokio::sync::RwLock<Option<std::path::PathBuf>>>,
    cloning_workspaces: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>>,
    errored_workspaces: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<uuid::Uuid, String>>,
    >,
) -> Result<Router, OxyError> {
    let api_router = crate::server::router::api_router(
        enterprise,
        readonly,
        workspaces_root,
        active_workspace_path,
        cloning_workspaces,
        errored_workspaces,
    )
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
    readonly: bool,
    workspaces_root: Option<std::path::PathBuf>,
    active_workspace_path: std::sync::Arc<tokio::sync::RwLock<Option<std::path::PathBuf>>>,
    cloning_workspaces: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>>,
    errored_workspaces: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<uuid::Uuid, String>>,
    >,
) -> Result<Router, OxyError> {
    let internal_router = crate::server::router::internal_api_router(
        enterprise,
        readonly,
        workspaces_root,
        active_workspace_path,
        cloning_workspaces,
        errored_workspaces,
    )
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Failed to create internal API router: {}", e)))?;

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

    let _shutdown = create_shutdown_signal();

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
        tokio::spawn(async move {
            create_shutdown_signal().await;
            tracing::info!("Shutdown signal received, stopping server...");
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

        let shutdown = create_shutdown_signal();

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

    // Cleanup Docker containers (stop and remove all oxy-managed containers)
    docker::cleanup_containers().await;
}
