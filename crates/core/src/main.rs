use std::process::exit;

use oxy::cli::cli;
mod theme;
use dotenv::dotenv;
use human_panic::Metadata;
use human_panic::setup_panic;
use once_cell::sync::OnceCell;
use oxy::db::client;
use oxy::theme::StyledText;
use std::env;
use tracing_subscriber::{EnvFilter, fmt};

static LOG_GUARD: OnceCell<tracing_appender::non_blocking::WorkerGuard> = OnceCell::new();

fn init_tracing_logging(log_to_stdout: bool) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"))
        .add_directive("oxy=debug".parse().unwrap())
        .add_directive("deser_incomplete::options_impl=warn".parse().unwrap())
        .add_directive("tower_http=debug".parse().unwrap());
    let is_debug = cfg!(debug_assertions);

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
    let log_builder = fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_level(true)
        .with_writer(non_blocking);

    if !is_debug {
        log_builder.json().init();
    } else {
        log_builder.init();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // at some version, rustls changed default to aws_lc, which some libs are not aware of
    // so we need to set it to default provider to avoid collision of crypto provider
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    setup_panic!(Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .authors("Robert Yi <robert@oxy.tech>") // temporarily using Robert email here, TODO: replace by support email
        .homepage("github.com/oxy-hq/oxy")
        .support("- For support, please email robert@oxy.tech or contact us directly via Discord or Github.")
    );
    dotenv().ok();

    // Log to stdout if `oxy serve`
    let args: Vec<String> = env::args().collect();
    let log_to_stdout = args.iter().any(|a| a == "serve");
    init_tracing_logging(log_to_stdout);

    match cli().await {
        Ok(_) => {}
        Err(e) => {
            tracing::error!(error = %e, "Application error");
            eprintln!("{}", format!("{}", e).error());
            exit(1)
        }
    };
    Ok(())
}
