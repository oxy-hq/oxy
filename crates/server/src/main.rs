// A binary is its own crate root, so the `recursion_limit` in lib.rs does not
// apply here. Laying out the futures reached from `cli()` exceeds rustc's
// default query depth since SeaORM 2.0 deepened its query types.
#![recursion_limit = "256"]

// Dev-only dynamic linking (see `oxy-app-dylib` + `just dev-backend-dyn`).
// Forcing the dylib into the link (with `-C prefer-dynamic`) makes oxy-app's
// symbols — and its ~1.4 GB of static deps — resolve dynamically from
// liboxy_app_dylib.dylib instead of being re-linked into the binary every edit.
// `as _` because we import it purely for the link edge, not to use its items.
#[cfg(feature = "dev-dynamic")]
extern crate oxy_app_dylib as _;

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

/// How many daily `oxy.log` files to keep on local installs before the oldest
/// is pruned. Bounds the on-disk log footprint for a long-lived local session.
const LOCAL_LOG_MAX_FILES: usize = 7;

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
             sqlx=warn,sea_orm=warn,tonic=warn,rustls=warn,\
             tokio_postgres=warn,tungstenite=warn,tokio_tungstenite=warn,\
             deser_incomplete=off"
        )
    });
    let make_filter = || EnvFilter::new(&filter_directives);

    // oxy.log is a local-dev convenience — a developer running oxy on their
    // laptop can tail it. In cloud (Kubernetes) every process already ships its
    // logs to the cluster aggregator via stdout/stderr, so an extra on-disk file
    // is pure waste: on the serve/worker fleet the state dir is an emptyDir, and
    // an unrotated oxy.log grows until the kubelet evicts the pod under node
    // DiskPressure. So write the file on Local only — and bound it even there
    // with daily rotation so a long-lived session can't grow it without limit.
    // Rotation dates the filename: `oxy.<YYYY-MM-DD>.log`.
    let file_writer = match log_format {
        LogFormat::Local => match tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("oxy")
            .filename_suffix("log")
            .max_log_files(LOCAL_LOG_MAX_FILES)
            .build(get_state_dir())
        {
            Ok(appender) => {
                let (writer, guard) = tracing_appender::non_blocking(appender);
                LOG_GUARD.set(guard).ok();
                Some(writer)
            }
            Err(e) => {
                eprintln!("oxy: could not initialize oxy.log file appender: {e}");
                None
            }
        },
        LogFormat::Cloud => None,
    };

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
    // agent/automation spans independently of OXY_LOG_LEVEL. A global
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
            let file_layer = file_writer.map(|writer| {
                fmt::layer()
                    .with_target(true)
                    .with_level(true)
                    .with_writer(writer)
                    .with_ansi(false)
                    .with_filter(make_filter())
            });
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
            // No file layer in cloud: logs already go to stdout/stderr → the
            // cluster aggregator, and the state dir is an emptyDir we must not
            // fill (see the `file_writer` comment above).
            let obs_layer =
                obs_collector.map(|l| l.with_filter(oxy_observability::observability_filter()));
            let sentry_layer = sentry::integrations::tracing::layer()
                .with_filter(tracing_subscriber::filter::LevelFilter::WARN);
            tracing_subscriber::registry()
                .with(sentry_layer)
                .with(console_layer)
                .with(obs_layer)
                .init();
        }
    }
}

/// Raise the process's open-file-descriptor soft limit at startup.
///
/// macOS ships a default soft `RLIMIT_NOFILE` of 256. A busy oxy instance
/// (the warehouse + LLM HTTP clients, the embedded Postgres pool, and many
/// concurrent SSE streams from data-app dashboards) blows past that and the
/// server stops accepting connections with
/// `axum::serve::listener: accept error: Too many open files (os error 24)`.
/// We bump the soft limit toward the hard cap (clamped to a sane target that
/// stays under macOS's `kern.maxfilesperproc`) so the server has headroom
/// regardless of the shell/launcher it was started from. Best-effort: any
/// failure is logged and the process continues with the inherited limit.
#[cfg(unix)]
fn raise_fd_limit() {
    // 65536 is ample headroom for a single instance and stays well under the
    // macOS per-process kernel cap on default systems.
    const DESIRED: libc::rlim_t = 65_536;
    // SAFETY: plain libc rlimit syscalls on a zeroed struct; single-threaded
    // here (before the Tokio runtime is built).
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return;
        }
        let target = if lim.rlim_max == libc::RLIM_INFINITY {
            DESIRED
        } else {
            std::cmp::min(DESIRED, lim.rlim_max)
        };
        if lim.rlim_cur >= target {
            return; // already sufficient
        }
        let prev = lim.rlim_cur;
        lim.rlim_cur = target;
        if libc::setrlimit(libc::RLIMIT_NOFILE, &lim) == 0 {
            tracing::debug!(from = prev, to = target, "raised RLIMIT_NOFILE soft limit");
        } else {
            tracing::warn!(
                current = prev,
                "could not raise RLIMIT_NOFILE; if you hit \"Too many open files\", \
                 raise it manually with `ulimit -n 65536` before starting oxy"
            );
        }
    }
}

#[cfg(not(unix))]
fn raise_fd_limit() {}

fn main() {
    dotenv().ok();
    let _sentry_guard = sentry_config::init_sentry();
    if _sentry_guard.is_none() {
        setup_panic!(
            Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
                .authors("Robert Yi <robert@oxygen-hq.com>") // temporarily using Robert email here, TODO: replace by support email
                .homepage("github.com/oxy-hq/oxygen")
                .support(
                    "- For support, please email robert@oxygen-hq.com or contact us directly via Github."
                )
        );
    }

    // Parse args early to check for flags
    let args: Vec<String> = env::args().collect();

    // Check if --enterprise flag is present (gates the observability UI/routes)
    let enterprise_enabled = args.iter().any(|a| a == "--enterprise");

    // Observability is opt-in everywhere — including `--local`. ClickHouse is
    // the sole backend, so there is no embedded store to default to: enabling
    // it implies a running ClickHouse (`oxy start` boots the container when
    // the var is set). With `--enterprise` but no backend, we warn and run
    // with observability disabled — no data is recorded and the UI surfaces a
    // "not configured" banner.
    let observability_enabled = env::var_os("OXY_OBSERVABILITY_BACKEND").is_some();
    if enterprise_enabled && !observability_enabled {
        eprintln!(
            "{}",
            "Observability disabled: OXY_OBSERVABILITY_BACKEND is not set. \
             Set it to clickhouse — with OXY_CLICKHOUSE_URL pointing at your \
             instance, or under `oxy start`, which boots one — to record traces."
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
            // yet — the `oxy start` path boots its ClickHouse container and
            // only then are `OXY_CLICKHOUSE_*` set.
            // `observability_boot::finalize()` is called from `serve.rs` once
            // that endpoint is available, to resolve the backend and spawn
            // the bridge task.
            init_tracing_logging(observability_enabled);

            // Give the server enough file-descriptor headroom before it binds
            // listeners / boots the embedded Postgres — macOS defaults to a
            // soft NOFILE of 256, which busy instances exhaust (EMFILE).
            raise_fd_limit();

            // Surface crates mounted by this composition root. `oxy-api-github`
            // is the first extracted sibling; more merge in as they're pulled
            // out of oxy-app. `cli` forwards these into `serve`'s `api_router`,
            // where they join the protected tree before the auth middleware.
            let exit_code = match cli(
                oxy_api_github::routes()
                    .merge(oxy_api_partner_console::routes())
                    .merge(oxy_api_onboarding::routes()),
                oxy_api_onboarding::workspace_routes(),
            )
            .await
            {
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
