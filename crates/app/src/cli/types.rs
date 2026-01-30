//! CLI argument types for Oxy commands

use clap::Parser;

#[derive(Parser, Debug)]
pub struct A2aArgs {
    /// Port number for the A2A server
    ///
    /// Specify which port to bind the A2A protocol server.
    /// Default is 8080 if not specified in configuration.
    #[clap(long)]
    pub port: Option<u16>,
    /// Host address to bind the A2A server
    ///
    /// Specify which host address to bind the A2A server.
    /// Default is 0.0.0.0 to listen on all interfaces.
    #[clap(long)]
    pub host: Option<String>,
    /// Base URL for constructing agent card endpoint URLs
    ///
    /// The base URL that external agents will use to reach this server.
    /// Used in agent cards to construct endpoint URLs.
    /// Example: https://api.example.com
    #[clap(long)]
    pub base_url: Option<String>,
}

/// Arguments for the `oxy serve` command (web server only, no Docker)
#[derive(Parser, Debug, Clone)]
pub struct ServeArgs {
    /// Port number for the web application server
    ///
    /// Specify which port to bind the Oxy web interface.
    /// Default is 3000 if not specified.
    #[clap(long, default_value_t = 3000)]
    pub port: u16,
    /// Host address to bind the web application server
    ///
    /// Specify which host address to bind the Oxy web interface.
    /// Default is 0.0.0.0 to listen on all interfaces.
    #[clap(long, default_value = "0.0.0.0")]
    pub host: String,
    /// Enable git-based project detection and onboarding
    ///
    /// When enabled, allows starting the server outside of an Oxy project
    /// directory and provides git-based onboarding functionality.
    #[clap(long, default_value_t = false)]
    pub readonly: bool,
    /// Force HTTP/2 only mode (disable HTTP/1.1)
    ///
    /// When enabled, the server will only accept HTTP/2 connections over TLS.
    /// HTTP/1.1 requests will be rejected. Default supports both protocols.
    #[clap(long, default_value_t = false)]
    pub http2_only: bool,
    /// TLS certificate file for HTTPS (local development)
    #[clap(long, default_value = "localhost+2.pem")]
    pub tls_cert: String,
    /// TLS private key file for HTTPS (local development)
    #[clap(long, default_value = "localhost+2-key.pem")]
    pub tls_key: String,

    #[clap(long, default_value_t = false)]
    pub cloud: bool,

    /// Enable enterprise features (ClickHouse observability, analytics)
    ///
    /// When enabled, requires ClickHouse environment variables to be set:
    /// - OXY_CLICKHOUSE_URL (required)
    /// - OXY_CLICKHOUSE_USER (optional, default: default)
    /// - OXY_CLICKHOUSE_PASSWORD (optional)
    /// - OXY_CLICKHOUSE_DATABASE (optional, default: otel)
    #[clap(long, default_value_t = false)]
    pub enterprise: bool,
}

/// Arguments for the `oxy start` command (Docker containers + web server)
#[derive(Parser, Debug)]
pub struct StartArgs {
    /// Server configuration options
    #[clap(flatten)]
    pub serve: ServeArgs,

    /// Enable enterprise features (ClickHouse, OTel, Cube.js containers)
    ///
    /// When enabled, starts additional containers for enterprise features:
    /// - ClickHouse for analytics
    /// - OpenTelemetry Collector for observability
    /// - Cube.js for semantic layer
    #[clap(long, default_value_t = false)]
    pub enterprise: bool,

    /// Clean start: remove existing Docker containers and volumes before starting
    ///
    /// When enabled, removes all Oxy-managed Docker containers and their
    /// associated volumes to start with a fresh state. This is useful for
    /// troubleshooting or resetting the local environment.
    #[clap(long, default_value_t = false)]
    pub clean: bool,
}
