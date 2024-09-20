mod ai;
mod chain;
mod init;
mod python_interop;
mod yaml_parsers;

use clap::Parser;
use reqwest::Client;
use std::error::Error;
use std::{env, fs, io};

use crate::chain::process_input;
use crate::yaml_parsers::config_parser::{get_config_path, parse_config, Config, ParsedConfig};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// The question to ask or command to execute
    #[clap(default_value = "")]
    input: String,

    /// Output format: 'text' (default) or 'code' for SQL
    #[clap(long, value_name = "FORMAT")]
    output: Option<String>,

    /// Subcommand
    #[clap(subcommand)]
    command: Option<SubCommand>,
}

#[derive(Parser, Debug)]
enum SubCommand {
    Init,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    match args.command {
        Some(SubCommand::Init) => match init::init() {
            Ok(_) => println!("Initialization complete"),
            Err(e) => eprintln!("Initialization failed: {}", e),
        },
        None => {
            if !args.input.is_empty() {
                let config_path = get_config_path();
                let config = parse_config(config_path)?;
                handle_input(&args.input, args.output.as_deref(), &config).await?;
            } else {
                println!(
                    "Use 'onyx init' to initialize a new project or provide a question/command."
                );
            }
        }
    }

    Ok(())
}

async fn handle_input(
    input: &str,
    output_format: Option<&str>,
    config: &Config,
) -> Result<(), Box<dyn Error>> {
    let ParsedConfig {
        agent,
        model,
        warehouse,
    } = config.load_defaults()?;

    let api_key = env::var(&model.key_var).map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} not set", model.key_var),
        )
    })?;

    let client = Client::new();

    process_input(
        &agent,
        &client,
        &api_key,
        input,
        &model.model_ref,
        &warehouse.key_path,
    )
    .await
}
