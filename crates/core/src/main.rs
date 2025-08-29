use std::process::exit;

use oxy::cli::cli;
mod theme;
use dotenv::dotenv;
use human_panic::Metadata;
use human_panic::setup_panic;
use once_cell::sync::OnceCell;
use oxy::db::client;
use oxy::sentry_config;
use oxy::theme::StyledText;
use std::env;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

static LOG_GUARD: OnceCell<tracing_appender::non_blocking::WorkerGuard> = OnceCell::new();

#[derive(Debug, Clone)]
enum LogFormat {
    Pretty, // Human-readable format for development
    Json,
    CloudRun, // Google Cloud Run optimized format
    Compact,  // Compact format for other cloud platforms
}

impl LogFormat {
    fn detect() -> Self {
        // Check environment variables to determine the platform
        if env::var("K_SERVICE").is_ok() || env::var("CLOUD_RUN_JOB").is_ok() {
            LogFormat::CloudRun
        // AWS environments - use JSON for better CloudWatch integration
        } else if env::var("AWS_LAMBDA_FUNCTION_NAME").is_ok()
            || env::var("AWS_EXECUTION_ENV").is_ok()
            || env::var("AWS_COGNITO_USER_POOL_ID").is_ok()
        {
            LogFormat::Json
        } else if env::var("VERCEL").is_ok() {
            LogFormat::Json
        } else if cfg!(debug_assertions) {
            LogFormat::Pretty
        } else {
            LogFormat::Compact
        }
    }
}

fn init_tracing_logging(log_to_stdout: bool) {
    // Default all crates to WARN level to reduce noise, then selectively enable INFO for critical components
    // This approach is more maintainable and ensures we don't miss any noisy dependencies
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn")
            // Core Oxy components
            .add_directive("oxy=info".parse().unwrap())
            .add_directive("tower_http=info".parse().unwrap())
            // Only enable trace-level HTTP logging in debug builds or when explicitly requested
            .add_directive(if cfg!(debug_assertions) {
                "tower_http::trace=info".parse().unwrap()
            } else {
                "tower_http::trace=warn".parse().unwrap()
            })
            // Database-related logging - SQLx can be very verbose
            .add_directive("sqlx=warn".parse().unwrap()) // Reduce SQLx query logging noise
            .add_directive("sea_orm=info".parse().unwrap()) // Keep SeaORM at info level if used
            .add_directive("deser_incomplete=warn".parse().unwrap()) // Keep deser_incomplete quiet
    });
    // Allow override via environment variable
    // If not set, auto-detects based on environment (Cloud Run, AWS Lambda, etc.)
    let log_format = env::var("OXY_LOG_FORMAT")
        .ok()
        .and_then(|f| match f.to_lowercase().as_str() {
            "pretty" => Some(LogFormat::Pretty),
            "json" => Some(LogFormat::Json),
            "cloudrun" => Some(LogFormat::CloudRun),
            "compact" => Some(LogFormat::Compact),
            _ => None,
        })
        .unwrap_or_else(LogFormat::detect);

    let log_file_path = std::path::Path::new(&client::get_state_dir()).join("oxy.log");
    let file_appender = tracing_appender::rolling::never(
        log_file_path.parent().unwrap(),
        log_file_path.file_name().unwrap(),
    );
    let (non_blocking, guard) = if log_to_stdout {
        tracing_appender::non_blocking(std::io::stdout())
    } else {
        tracing_appender::non_blocking(file_appender)
    };
    LOG_GUARD.set(guard).ok();

    match log_format {
        LogFormat::Pretty => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(sentry::integrations::tracing::layer())
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_level(true)
                        .with_writer(non_blocking)
                        .pretty(),
                )
                .init();
        }
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(sentry::integrations::tracing::layer())
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_level(true)
                        .with_writer(non_blocking)
                        .json(),
                )
                .init();
        }
        LogFormat::CloudRun => {
            // the cloud run web ui log browser is optimized for compact logs
            tracing_subscriber::registry()
                .with(env_filter)
                .with(sentry::integrations::tracing::layer())
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_level(true)
                        .with_writer(non_blocking)
                        .with_ansi(false)
                        .without_time()
                        .compact(),
                )
                .init();
        }
        LogFormat::Compact => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(sentry::integrations::tracing::layer())
                .with(
                    fmt::layer()
                        .with_target(false)
                        .with_level(true)
                        .with_writer(non_blocking)
                        .compact(),
                )
                .init();
        }
    }
}

fn main() {
    dotenv().ok();
    let _sentry_guard = sentry_config::init_sentry();
    if _sentry_guard.is_none() {
        setup_panic!(Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
            .authors("Robert Yi <robert@oxy.tech>") // temporarily using Robert email here, TODO: replace by support email
            .homepage("github.com/oxy-hq/oxy")
            .support("- For support, please email robert@oxy.tech or contact us directly via Discord or Github.")
        );
    }

    // Log to stdout if `oxy serve`
    let args: Vec<String> = env::args().collect();
    let log_to_stdout = args.iter().any(|a| a == "serve");
    init_tracing_logging(log_to_stdout);

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // DO NOT USE #[tokio::main]
    // https://docs.sentry.io/platforms/rust/
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            match cli().await {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(error = %e, "Application error");
                    sentry_config::capture_error_with_context(e.as_ref(), "CLI execution failed");
                    eprintln!("{}", format!("{e}").error());
                    exit(1)
                }
            };
        });
}
