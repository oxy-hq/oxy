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

    fn is_cloud() -> bool {
        matches!(Self::detect(), LogFormat::Cloud)
    }
}

fn init_tracing_logging(
    log_to_stdout: bool,
    observability_store: Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
) {
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

    // Always log to file
    let log_file_path = std::path::Path::new(&get_state_dir()).join("oxy.log");
    let file_appender = tracing_appender::rolling::never(
        log_file_path.parent().unwrap(),
        log_file_path.file_name().unwrap(),
    );
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    LOG_GUARD.set(guard).ok();

    // Build the `SpanCollectorLayer` up front so it can be composed with the
    // same subscriber as Sentry + file appender + fmt (instead of being
    // installed as a second, unrelated subscriber that would drop them).
    // The filter is applied inline at the `.with()` site so that S can be
    // inferred against the full subscriber stack.
    let obs_collector = observability_store.map(oxy_observability::build_observability_layer);

    match log_format {
        LogFormat::Local => {
            let stdout_layer = log_to_stdout.then(|| {
                fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_writer(std::io::stdout)
                    .with_ansi(true)
            });
            tracing_subscriber::registry()
                .with(env_filter)
                .with(sentry::integrations::tracing::layer())
                .with(
                    fmt::layer()
                        .with_target(true)
                        .with_level(true)
                        .with_writer(file_writer)
                        .with_ansi(false),
                )
                .with(stdout_layer)
                .with(
                    obs_collector.map(|l| l.with_filter(oxy_observability::observability_filter())),
                )
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
            });
            tracing_subscriber::registry()
                .with(env_filter)
                .with(sentry::integrations::tracing::layer())
                .with(
                    fmt::layer()
                        .with_target(false)
                        .with_level(true)
                        .with_writer(file_writer)
                        .with_ansi(false)
                        .compact(),
                )
                .with(stdout_layer)
                .with(
                    obs_collector.map(|l| l.with_filter(oxy_observability::observability_filter())),
                )
                .init();
        }
    }
}

/// Resolve the observability backend, open the store, and build a status
/// message. The message is returned (not printed) so the caller can emit it
/// AFTER the tracing subscriber is installed — this avoids mixing raw stdout
/// with structured logs, and guarantees the printed backend matches the one
/// actually in use (no misleading "postgres" label when we fell back to DuckDB).
async fn resolve_observability_backend() -> (
    Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    Option<String>,
) {
    let requested = env::var("OXY_OBSERVABILITY_BACKEND").ok();
    let has_db_url = env::var("OXY_DATABASE_URL").is_ok();
    let backend = requested.clone().unwrap_or_else(|| {
        if has_db_url {
            "postgres".to_string()
        } else {
            "duckdb".to_string()
        }
    });

    match backend.as_str() {
        "duckdb" => {
            let db_path = get_state_dir().join("observability.duckdb");
            match oxy_observability::backends::duckdb::DuckDBStorage::open(&db_path) {
                Ok(storage) => (
                    Some(std::sync::Arc::new(storage)),
                    Some(format!("Observability: duckdb ({})", db_path.display())),
                ),
                Err(e) => {
                    eprintln!(
                        "{}",
                        format!("Failed to open DuckDB observability: {e}").error()
                    );
                    (None, None)
                }
            }
        }
        "clickhouse" => {
            match oxy_observability::backends::clickhouse::ClickHouseObservabilityStorage::from_env(
            )
            .await
            {
                Ok(storage) => match storage.ensure_schema().await {
                    Ok(()) => {
                        // Apply retention TTL at the engine level so background
                        // merges expire old rows automatically — no app-level
                        // DELETE loop needed for ClickHouse.
                        if let Err(e) = storage
                            .apply_retention_ttl(oxy_observability::RETENTION_DAYS)
                            .await
                        {
                            eprintln!("{}", format!("ClickHouse TTL apply failed: {e}").error());
                        }
                        (
                            Some(std::sync::Arc::new(storage)
                                as std::sync::Arc<dyn oxy_observability::ObservabilityStore>),
                            Some("Observability: clickhouse (OXY_CLICKHOUSE_URL)".to_string()),
                        )
                    }
                    Err(e) => {
                        eprintln!("{}", format!("ClickHouse schema init failed: {e}").error());
                        (None, None)
                    }
                },
                Err(e) => {
                    eprintln!("{}", format!("ClickHouse init failed: {e}").error());
                    (None, None)
                }
            }
        }
        _ => {
            // "postgres" or unknown: try Postgres if OXY_DATABASE_URL is set, else fall back to DuckDB.
            if has_db_url {
                match oxy_observability::backends::postgres::PostgresObservabilityStorage::from_env(
                )
                .await
                {
                    Ok(storage) => (
                        Some(std::sync::Arc::new(storage)
                            as std::sync::Arc<dyn oxy_observability::ObservabilityStore>),
                        Some("Observability: postgres (OXY_DATABASE_URL)".to_string()),
                    ),
                    Err(e) => {
                        eprintln!(
                            "{}",
                            format!("Postgres observability failed: {e}. Falling back to DuckDB.")
                                .error()
                        );
                        fallback_to_duckdb()
                    }
                }
            } else {
                let label = if requested.as_deref() == Some("postgres") {
                    "Observability: postgres → duckdb (OXY_DATABASE_URL not set)"
                } else {
                    "Observability: duckdb (no OXY_DATABASE_URL)"
                };
                let (store, _) = fallback_to_duckdb();
                (store, Some(label.to_string()))
            }
        }
    }
}

fn fallback_to_duckdb() -> (
    Option<std::sync::Arc<dyn oxy_observability::ObservabilityStore>>,
    Option<String>,
) {
    let db_path = get_state_dir().join("observability.duckdb");
    match oxy_observability::backends::duckdb::DuckDBStorage::open(&db_path) {
        Ok(s) => (
            Some(
                std::sync::Arc::new(s) as std::sync::Arc<dyn oxy_observability::ObservabilityStore>
            ),
            None,
        ),
        Err(e) => {
            eprintln!("{}", format!("DuckDB fallback failed: {e}").error());
            (None, None)
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
            // Initialize observability inside the Tokio runtime.
            // OXY_OBSERVABILITY_BACKEND selects the storage backend:
            //   "duckdb"    -> embedded DuckDB file
            //   "postgres"  -> shared Postgres via OXY_DATABASE_URL
            //   "clickhouse"-> ClickHouse via OXY_CLICKHOUSE_URL
            // Default: postgres if OXY_DATABASE_URL is set, else duckdb.
            let (observability_store, backend_msg) = if enterprise_enabled {
                resolve_observability_backend().await
            } else {
                (None, None)
            };

            // Install tracing with Sentry + file appender + optional
            // observability layer all on the same subscriber.
            init_tracing_logging(log_to_stdout, observability_store.clone());

            if let Some(msg) = backend_msg {
                println!("{msg}");
            }

            if let Some(ref store) = observability_store {
                oxy_observability::global::set_global(std::sync::Arc::clone(store));

                // Start background retention cleanup. The retention window is
                // derived from the longest duration the UI supports (see
                // `oxy_observability::RETENTION_DAYS`). For ClickHouse this
                // is a no-op since TTL was applied on schema init; for DuckDB
                // and Postgres it runs a periodic DELETE loop.
                oxy_observability::spawn_retention_cleanup(std::sync::Arc::clone(store));
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

            let exit_code = match cli().await {
                Ok(_) => 0,
                Err(e) => {
                    tracing::error!(error = %e, "Application error");
                    sentry_config::capture_error_with_context(&*e, "CLI execution failed");
                    eprintln!("{}", format!("{e}").error());
                    1
                }
            };

            // Gracefully shut down observability storage
            if let Some(ref store) = observability_store {
                store.shutdown().await;
            }
            oxy_observability::shutdown();

            if exit_code != 0 {
                exit(exit_code);
            }
        });
}
