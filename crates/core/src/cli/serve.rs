use crate::cli::ServeArgs;
use crate::config::constants::DEFAULT_API_KEY_HEADER;
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::theme::StyledText;
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
    run_database_migrations().await?;

    let _available_port = find_available_port(args.host.clone(), args.port).await?;
    let app = create_web_application(args.cloud).await?;

    serve_application(app, args).await
}

async fn run_database_migrations() -> Result<(), OxyError> {
    let db = establish_connection()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to connect to database: {}", e)))?;

    Migrator::up(&db, None)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to run database migrations: {}", e)))
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

async fn create_web_application(cloud: bool) -> Result<Router, OxyError> {
    let api_router = crate::api::router::api_router(cloud)
        .await
        .map(|router| router.layer(create_trace_layer()))
        .map_err(|e| OxyError::RuntimeError(format!("Failed to create API router: {}", e)))?;
    let openapi_router = crate::api::router::openapi_router().await;
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

    Ok(Router::new()
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
        .layer(create_trace_layer()))
}

fn create_trace_layer()
-> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
        .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
        .on_request(trace::DefaultOnRequest::new().level(Level::INFO))
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

async fn serve_application(app: Router, args: ServeArgs) -> Result<(), OxyError> {
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
        " (HTTP/2 ONLY)"
    } else {
        " (HTTP/1.1+HTTP/2)"
    };
    println!(
        "{} {}{}",
        "Web app running at".text(),
        format!("{}://{}:{}", protocol, display_host, args.port).secondary(),
        protocol_info
    );

    let shutdown = create_shutdown_signal();

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
            let default_cert: &[u8] = include_bytes!("../../../../localhost+2.pem");
            let default_key: &[u8] = include_bytes!("../../../../localhost+2-key.pem");
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
        let result = axum_server::bind_rustls(socket_addr, config)
            .serve(app.into_make_service())
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(OxyError::RuntimeError(format!("Server error: {}", e))),
        }
    } else {
        let listener = tokio::net::TcpListener::bind(socket_addr)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to bind to address: {}", e)))?;
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
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
