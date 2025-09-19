use std::process::exit;
use std::str::FromStr;

use onyx::cli::cli;
mod theme;
use fern::colors::{Color, ColoredLevelConfig};
use human_panic::setup_panic;
use human_panic::Metadata;
use onyx::db::client;
use onyx::theme::StyledText;

fn init_logging() -> Result<(), fern::InitError> {
    // allow override stdout log level with RUST_LOG env var
    let stdout_log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "off".to_string());

    let log_file_path = std::path::Path::new(&client::get_state_dir()).join("onyx.log");

    fern::Dispatch::new()
        .chain(
            // log everything to a file
            fern::Dispatch::new()
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "[{} {} {} ] {}",
                        humantime::format_rfc3339_seconds(std::time::SystemTime::now()),
                        record.level(),
                        record.module_path().unwrap_or("unknown"),
                        message,
                    ))
                })
                .level(log::LevelFilter::Trace)
                .chain(fern::log_file(log_file_path)?),
        )
        .chain(
            // log only onyx logs to stdout
            fern::Dispatch::new()
                .level(log::LevelFilter::Off)
                .level_for(
                    "onyx",
                    log::LevelFilter::from_str(&stdout_log_level).expect("Invalid log level"),
                )
                .format(|out, message, record| {
                    let colors = ColoredLevelConfig::new()
                        .info(Color::Green)
                        .warn(Color::BrightYellow)
                        .error(Color::Red);
                    out.finish(format_args!(
                        "[{}] {}",
                        colors.color(record.level()),
                        message,
                    ))
                })
                .chain(std::io::stdout()),
        )
        .apply()?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_panic!(Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .authors("Robert Yi <robert@onyxint.ai>") // temporarily using Robert email here, TODO: replace by support email
        .homepage("github.com/onyx-hq/onyx-public-releases")
        .support("- For support, please email robert@onyxint.ai or contact us directly via Slack if you have access to a shared channel.")
    );
    init_logging()?;
    match cli().await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e);
            eprintln!("{}", format!("{}", e).error());
            exit(1)
        }
    };
    Ok(())
}
