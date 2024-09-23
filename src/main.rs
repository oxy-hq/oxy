mod agent;
mod init;
mod yaml_parsers;
mod search;

use clap::Parser;
use std::error::Error;
use std::path::PathBuf;
use skim::prelude::*;

use crate::agent::Agent;
use crate::yaml_parsers::config_parser::{get_config_path, parse_config, Config};
use crate::yaml_parsers::entity_parser::parse_entity_config_from_scope;

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
                handle_input(&args.input, &config).await?;
            } else {
                let config_path = get_config_path();
                let config = parse_config(config_path)?;
                let project_path = PathBuf::from(&config.defaults.project_path);
                search::search_files(&project_path)?;
            }
        }
    }

    Ok(())
}

async fn handle_input(input: &str, config: &Config) -> Result<(), Box<dyn Error>> {
    let parsed_config = config.load_defaults()?;
    let entity_config = parse_entity_config_from_scope(
        &parsed_config.agent_config.scope,
        &config.defaults.project_path,
    )?;

    let agent = Agent::new(parsed_config, entity_config);

    agent.execute_chain(input).await
}
