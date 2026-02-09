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
    Local, // Human-readable format with colors for local development
    Cloud, // Plain text format for Kubernetes/cloud (no ANSI, compact)
}

impl LogFormat {
    fn detect() -> Self {
        // Check if running in Kubernetes
        if env::var("KUBERNETES_SERVICE_HOST").is_ok() || env::var("KUBERNETES_PORT").is_ok() {
            LogFormat::Cloud
        } else {
            LogFormat::Local
        }
    }

    fn is_cloud() -> bool {
        matches!(Self::detect(), LogFormat::Cloud)
    }
}

fn init_tracing_logging(log_to_stdout: bool) {
    let log_format = LogFormat::detect();
    let is_cloud = LogFormat::is_cloud();

    // Default to WARN level for minimal output in both cloud and local
    // Use OXY_LOG_LEVEL=info or OXY_LOG_LEVEL=debug for verbose output
    let default_level = "warn";
    let log_level = env::var("OXY_LOG_LEVEL")
        .as_deref()
        .unwrap_or(default_level)
        .to_lowercase();

    // In cloud environments, significantly reduce logging noise
    // - Turn off HTTP request/response logs (we'll use TRACE level which won't be logged)
    // - Turn off SQL query logs (they're extremely verbose)
    // - Only show WARN+ for most crates
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if is_cloud {
            EnvFilter::new(&log_level)
                .add_directive("tower_http=warn".parse().unwrap()) // Only log HTTP errors
                .add_directive("sqlx=warn".parse().unwrap()) // Only log SQL errors
                .add_directive("sea_orm=warn".parse().unwrap())
                .add_directive("deser_incomplete=off".parse().unwrap())
        } else {
            // Local: more verbose for development
            EnvFilter::new(&log_level)
                .add_directive("tower_http::trace=info".parse().unwrap())
                .add_directive("sqlx=warn".parse().unwrap())
                .add_directive("sea_orm=warn".parse().unwrap())
                .add_directive("deser_incomplete=off".parse().unwrap())
        }
    });

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
        LogFormat::Local => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(sentry::integrations::tracing::layer())
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_level(true)
                        .with_writer(non_blocking)
                        .with_ansi(true),
                )
                .init();
        }
        LogFormat::Cloud => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(sentry::integrations::tracing::layer())
                .with(
                    fmt::layer()
                        .with_target(false)
                        .with_level(true)
                        .with_writer(non_blocking)
                        .with_ansi(false) // No colors in cloud
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
