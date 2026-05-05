use std::io::IsTerminal;
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
    Cloud, // Structured JSON for Kubernetes/cloud log aggregators
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

fn init_tracing_logging(observability_enabled: bool) {
    let log_format = LogFormat::detect();

    // OXY_DEBUG=true: shortcut for debug-level logging. When set, it overrides
    // OXY_LOG_LEVEL so developers get verbose oxy output without having to
    // remember the env var name. Framework crates are still suppressed.
    let debug_mode = env::var("OXY_DEBUG")
        .as_deref()
        .unwrap_or("false")
        .eq_ignore_ascii_case("true");

    let log_level = if debug_mode {
        "debug".to_string()
    } else {
        env::var("OXY_LOG_LEVEL")
            .unwrap_or_else(|_| "warn".to_string())
            .to_lowercase()
    };

    // Suppress known-noisy framework crates regardless of the requested log
    // level. This keeps output actionable even at info/debug by hiding HTTP
    // wire-level traces, raw SQL, and TLS protocol chatter. RUST_LOG bypasses
    // all of this when set, giving experts a full escape hatch.
    //
    // Resolve directives once into a string so the stdout/file layers don't
    // each re-read RUST_LOG and re-parse the directives. EnvFilter doesn't
    // implement Clone, so we rebuild it from the same string for each layer.
    let filter_directives = env::var("RUST_LOG").unwrap_or_else(|_| {
        format!(
            "{log_level},tower_http=warn,h2=warn,hyper=warn,reqwest=warn,\
             sqlx=warn,sea_orm=warn,tonic=warn,rustls=warn,deser_incomplete=off"
        )
    });
    let make_filter = || EnvFilter::new(&filter_directives);

    // Always write to oxy.log for legacy customers who rely on that file.
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
    //
    // `obs_layer`/`sentry_layer` are constructed inside each branch because
    // `with_filter` pins the target Subscriber type, and the Local vs Cloud
    // branches build different subscriber chains (Full vs Compact format).

    match log_format {
        LogFormat::Local => {
            // Console: colorized human-readable on stderr — stderr is the
            // conventional channel for diagnostics so a CLI's stdout stays
            // available for piped/captured program output.
            //
            // ANSI is enabled only when stderr is an interactive TTY. When
            // stderr is captured (Docker/Podman logs, file redirect, journald,
            // CI) the colors would otherwise leak in as `\x1b[2m...\x1b[0m`
            // sequences and make the captured logs unreadable.
            //
            // `.compact()` drops the per-event repetition of the full span-
            // chain breadcrumb (which embedded `oxy.sql=...` on every nested
            // log line during SQL execution). Span field values are still
            // recorded on the span itself so the observability backend can
            // read them — only the visual repetition is suppressed.
            let console_layer = fmt::layer()
                .compact()
                .with_target(true)
                .with_level(true)
                .with_writer(std::io::stderr)
                .with_ansi(std::io::stderr().is_terminal())
                .with_filter(make_filter());
            let file_layer = fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_writer(file_writer)
                .with_ansi(false)
                .with_filter(make_filter());
            let obs_layer =
                obs_collector.map(|l| l.with_filter(oxy_observability::observability_filter()));
            let sentry_layer = sentry::integrations::tracing::layer()
                .with_filter(tracing_subscriber::filter::LevelFilter::WARN);
            tracing_subscriber::registry()
                .with(sentry_layer)
                .with(file_layer)
                .with(console_layer)
                .with(obs_layer)
                .init();
        }
        LogFormat::Cloud => {
            // Console: structured JSON on stderr. Kubernetes/container
            // runtimes capture both stdout and stderr, so cloud aggregators
            // still pick this up while keeping stdout clean for any program
            // output the binary may emit.
            let console_layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(false)
                .with_writer(std::io::stderr)
                .with_filter(make_filter());
            let file_layer = fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_writer(file_writer)
                .with_ansi(false)
                .compact()
                .with_filter(make_filter());
            let obs_layer =
                obs_collector.map(|l| l.with_filter(oxy_observability::observability_filter()));
            let sentry_layer = sentry::integrations::tracing::layer()
                .with_filter(tracing_subscriber::filter::LevelFilter::WARN);
            tracing_subscriber::registry()
                .with(sentry_layer)
                .with(file_layer)
                .with(console_layer)
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
                .homepage("github.com/oxy-hq/oxygen")
                .support(
                    "- For support, please email robert@oxy.tech or contact us directly via Github."
                )
        );
    }

    // Parse args early to check for flags
    let args: Vec<String> = env::args().collect();

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
            init_tracing_logging(observability_enabled);

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
