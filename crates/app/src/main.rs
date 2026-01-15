use std::process::exit;

use dotenv::dotenv;
use human_panic::Metadata;
use human_panic::setup_panic;
use once_cell::sync::OnceCell;
use oxy::sentry_config;
use oxy::state_dir::get_state_dir;
use oxy::theme::StyledText;
use oxy_app::cli::commands::cli;
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
    let log_level = env::var("OXY_LOG_LEVEL")
        .as_deref()
        .unwrap_or("warn")
        .to_lowercase();
    // Default all crates to WARN level to reduce noise, then selectively enable INFO for critical components
    // This approach is more maintainable and ensures we don't miss any noisy dependencies
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(log_level)
            // Core Oxy components
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
            // Completely suppress deser_incomplete crate - it's too noisy with DEBUG logs
            .add_directive("deser_incomplete=off".parse().unwrap())
            .add_directive("deser_incomplete::options_impl=off".parse().unwrap())
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

    let (non_blocking, guard) = if log_to_stdout {
        tracing_appender::non_blocking(std::io::stdout())
    } else {
        let log_file_path = std::path::Path::new(&get_state_dir()).join("oxy.log");
        let file_appender = tracing_appender::rolling::never(
            log_file_path.parent().unwrap(),
            log_file_path.file_name().unwrap(),
        );
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
        setup_panic!(
            Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
                .authors("Robert Yi <robert@oxy.tech>") // temporarily using Robert email here, TODO: replace by support email
                .homepage("github.com/oxy-hq/oxy")
                .support(
                    "- For support, please email robert@oxy.tech or contact us directly via Github."
                )
        );
    }

    // Parse args early to check for log level override
    let args: Vec<String> = env::args().collect();

    // Log to stdout if `oxy serve` or `oxy a2a`
    let log_to_stdout = args
        .iter()
        .any(|a| a == "serve" || a == "start" || a == "a2a")
        || env::var("OXY_DEBUG")
            .as_deref()
            .unwrap_or("false")
            .eq_ignore_ascii_case("true");

    // Check if --enterprise flag is present (enables observability)
    let enterprise_enabled = args.iter().any(|a| a == "--enterprise");

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
            // Initialize tracing inside the Tokio runtime (OTLP exporter requires it)
            if enterprise_enabled {
                if let Err(e) = oxy::observability::init_telemetry() {
                    eprintln!(
                        "Failed to initialize OpenTelemetry: {e}. Falling back to local logging."
                    );
                    init_tracing_logging(log_to_stdout);
                }
            } else {
                init_tracing_logging(log_to_stdout);
            }

            // Register tool executors for workflow, agent, and semantic query tools
            // These registrations are critical - without them, workflow and agent tools won't work.
            // Fail fast if registration fails to prevent runtime errors.
            if let Err(e) = oxy_workflow::tool_executor::register_workflow_executors().await {
                tracing::error!(error = %e, "Failed to register workflow tool executors");
                eprintln!(
                    "{}",
                    format!("Failed to register workflow tool executors: {e}").error()
                );
                exit(1);
            }
            if let Err(e) = oxy_agent::tool_executor::register_agent_executor().await {
                tracing::error!(error = %e, "Failed to register agent tool executor");
                eprintln!(
                    "{}",
                    format!("Failed to register agent tool executor: {e}").error()
                );
                exit(1);
            }

            match cli().await {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(error = %e, "Application error");
                    sentry_config::capture_error_with_context(&*e, "CLI execution failed");
                    eprintln!("{}", format!("{e}").error());
                    exit(1)
                }
            };
        });
}
