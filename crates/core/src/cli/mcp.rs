use crate::mcp::OxyMcpServer;
use crate::theme::StyledText;
use rmcp::transport::SseServer;
use rmcp::transport::sse_server::SseServerConfig;
use rmcp::{ServiceExt, transport::stdio};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{Any, CorsLayer};
use tracing::{Instrument, error};

pub async fn start_mcp_stdio(project_path: PathBuf) -> anyhow::Result<()> {
    let service = OxyMcpServer::new(project_path)
        .await?
        .serve(stdio())
        .await
        .inspect_err(|e| {
            error!(error = ?e, "Error in MCP stdio server");
        })?;

    service.waiting().await?;
    Ok(())
}

pub async fn start_mcp_sse_server(
    mut port: u16,
    host: String,
    project_path: PathBuf,
) -> anyhow::Result<CancellationToken> {
    let original_port = port;
    let mut port_increment_count: u16 = 0;
    const MAX_PORT_INCREMENTS: u16 = 10;

    loop {
        match tokio::net::TcpListener::bind((host.as_str(), port)).await {
            Ok(_) => break,
            Err(e) => {
                if port <= 1024 && e.kind() == std::io::ErrorKind::PermissionDenied {
                    eprintln!(
                        "Permission denied binding to port {port}. Try running with sudo or use a port above 1024."
                    );
                    std::process::exit(1);
                }

                if port_increment_count >= MAX_PORT_INCREMENTS {
                    eprintln!(
                        "Failed to bind to any port after trying {} ports starting from {}. Error: {}",
                        port_increment_count + 1,
                        original_port,
                        e
                    );
                    std::process::exit(1);
                }

                println!("Port {port} for mcp is occupied. Trying next port...");
                port += 1;
                port_increment_count += 1;
            }
        }
    }

    let service = OxyMcpServer::new(project_path.clone()).await?;
    let bind = format!("{host}:{port}")
        .parse::<SocketAddr>()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], port)));
    let cors_layer = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let (sse_server, mut sse_router) = SseServer::new(SseServerConfig {
        bind,
        sse_path: "/sse".to_string(),
        post_path: "/message".to_string(),
        ct: CancellationToken::new(),
        sse_keep_alive: None,
    });

    sse_router = sse_router.layer(cors_layer);
    let ct = sse_server.with_service(move || service.to_owned());
    let serve_ct = ct.child_token();
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let server = axum::serve(listener, sse_router).with_graceful_shutdown(async move {
        serve_ct.cancelled().await;
        tracing::info!("sse server cancelled");
    });

    let display_host = if host == "0.0.0.0" {
        "localhost"
    } else {
        &host
    };
    println!(
        "{}",
        format!("MCP server running at http://{display_host}:{port}").secondary()
    );

    tokio::spawn(
        async move {
            if let Err(e) = server.await {
                tracing::error!(error = %e, "sse server shutdown with error");
            }
        }
        .instrument(tracing::info_span!("sse-server", bind_address = %bind)),
    );
    anyhow::Ok(ct)
}
