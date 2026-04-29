use std::process::exit;

use dotenv::dotenv;
use human_panic::Metadata;
use human_panic::setup_panic;
use once_cell::sync::OnceCell;
use oxy::sentry_config;
use oxy::state_dir::get_state_dir;
use oxy::theme::StyledText;
use oxy_app::cli::commands::cli;
use oxy_app::observability_boot;
use std::env;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

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
}

fn init_tracing_logging(log_to_stdout: bool, observability_enabled: bool) {
    let log_format = LogFormat::detect();
    let is_cloud = matches!(log_format, LogFormat::Cloud);

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

    // Always log to file
    let log_file_path = std::path::Path::new(&get_state_dir()).join("oxy.log");
    let file_appender = tracing_appender::rolling::never(
        log_file_path.parent().unwrap(),
        log_file_path.file_name().unwrap(),
    );
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    LOG_GUARD.set(guard).ok();

    // Build the `SpanCollectorLayer` up front so it can be composed with the
    // same subscriber as Sentry + file appender + fmt. The store isn't ready
    // yet (for `oxy start`, Postgres hasn't been booted), so we stash the
    // receiver and let `serve.rs` wire the bridge once the DB URL is set.
    // Spans emitted during startup buffer in the unbounded channel and flush
    // as soon as the bridge spawns.
    let obs_collector = if observability_enabled {
        let (layer, receiver) = oxy_observability::build_layer_and_receiver();
        observability_boot::stash_receiver(receiver);
        Some(layer)
    } else {
        None
    };

    // Filters are applied per-layer so that the observability layer captures
    // agent/workflow spans independently of OXY_LOG_LEVEL. A global
    // `.with(env_filter)` would drop info-level spans before they reached
    // any layer — the legacy OTel pipeline masked this, but the custom
    // SpanCollectorLayer must be kept isolated from console verbosity.
    match log_format {
        LogFormat::Local => {
            let stdout_layer = log_to_stdout.then(|| {
                fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_writer(std::io::stdout)
                    .with_ansi(true)
                    .with_filter(env_filter.clone())
            });
            let obs_layer =
                obs_collector.map(|l| l.with_filter(oxy_observability::observability_filter()));
            tracing_subscriber::registry()
                .with(
                    // Sentry only needs warn+ to populate breadcrumbs and
                    // capture error events. Filtering here avoids shipping
                    // the full info-level span firehose to its span store.
                    sentry::integrations::tracing::layer()
                        .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
                )
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_level(true)
                        .with_writer(file_writer)
                        .with_ansi(false)
                        .with_filter(env_filter),
                )
                .with(stdout_layer)
                .with(obs_layer)
                .init();
        }
        LogFormat::Cloud => {
            let stdout_layer = log_to_stdout.then(|| {
                fmt::layer()
                    .with_target(false)
                    .with_level(true)
                    .with_writer(std::io::stdout)
                    .with_ansi(false)
                    .compact()
                    .with_filter(env_filter.clone())
            });
            let obs_layer =
                obs_collector.map(|l| l.with_filter(oxy_observability::observability_filter()));
            tracing_subscriber::registry()
                .with(
                    // Sentry only needs warn+ to populate breadcrumbs and
                    // capture error events. Filtering here avoids shipping
                    // the full info-level span firehose to its span store.
                    sentry::integrations::tracing::layer()
                        .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
                )
                .with(
                    fmt::layer()
                        .with_target(false)
                        .with_level(true)
                        .with_writer(file_writer)
                        .with_ansi(false)
                        .compact()
                        .with_filter(env_filter),
                )
                .with(stdout_layer)
                .with(obs_layer)
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

    // Log to stdout only when OXY_DEBUG=true, otherwise always log to file
    let log_to_stdout = env::var("OXY_DEBUG")
        .as_deref()
        .unwrap_or("false")
        .eq_ignore_ascii_case("true");

    // Check if --enterprise flag is present (gates the observability UI/routes)
    let enterprise_enabled = args.iter().any(|a| a == "--enterprise");
    let local_mode = args.iter().any(|a| a == "--local");

    // In `--local` mode, default the observability backend to DuckDB. Local
    // installs are single-instance by definition, the state dir is already
    // writable, and the alternative (making the operator set the env var for
    // every local run) is pointless friction. For non-local runs the backend
    // stays opt-in — we don't want to pick a backend for a multi-pod cluster
    // without the operator asking.
    //
    // Safety: we're still single-threaded at this point (before `block_on`
    // spins up the Tokio runtime), so setting the env var is safe.
    if local_mode && env::var_os("OXY_OBSERVABILITY_BACKEND").is_none() {
        unsafe {
            env::set_var("OXY_OBSERVABILITY_BACKEND", "duckdb");
        }
    }

    // Observability is only enabled when the user explicitly picks a backend
    // (or is running --local, which implies duckdb). With `--enterprise` but
    // no backend, we warn and run with observability disabled — no data is
    // recorded and the UI surfaces a "not configured" banner.
    let observability_backend = env::var("OXY_OBSERVABILITY_BACKEND").ok();
    let observability_enabled = observability_backend.is_some();
    if enterprise_enabled && !observability_enabled {
        eprintln!(
            "{}",
            "Observability disabled: OXY_OBSERVABILITY_BACKEND is not set. \
             Set it to duckdb, postgres, or clickhouse to record traces."
                .text()
        );
    }

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
            // Install tracing with Sentry + file appender + (if enterprise)
            // the SpanCollectorLayer. The observability *store* isn't wired
            // yet — the `oxy start` path boots Postgres and only then is
            // `OXY_DATABASE_URL` set. `observability_boot::finalize()` is
            // called from `serve.rs` once the DB is ready to resolve the
            // backend and spawn the bridge task.
            init_tracing_logging(log_to_stdout, observability_enabled);

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
            if let Err(e) = oxy_airform::tool::register_dbt_executor().await {
                tracing::error!(error = %e, "Failed to register dbt tool executor");
                eprintln!(
                    "{}",
                    format!("Failed to register dbt tool executor: {e}").error()
                );
                exit(1);
            }

            let exit_code = match cli().await {
                Ok(_) => 0,
                Err(e) => {
                    tracing::error!(error = %e, "Application error");
                    sentry_config::capture_error_with_context(&*e, "CLI execution failed");
                    eprintln!("{}", format!("{e}").error());
                    1
                }
            };

            observability_boot::shutdown().await;

            if exit_code != 0 {
                exit(exit_code);
            }
        });
}
