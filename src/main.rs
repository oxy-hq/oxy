mod python_interop;
mod yaml_parsers;
mod init;
mod ai;

use clap::Parser;
use reqwest::Client;
use std::{env, io};
use std::error::Error;
use std::path::PathBuf;

use crate::ai::{generate_ai_response, interpret_results};
use crate::python_interop::execute_bigquery_query;
use crate::yaml_parsers::config_parser::{Config, ParsedConfig, parse_config, get_config_path};

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
        Some(SubCommand::Init) => {
            match init::init() {
                Ok(_) => println!("Initialization complete"),
                Err(e) => eprintln!("Initialization failed: {}", e),
            }
        },
        None => {
            if !args.input.is_empty() {
                let config_path = get_config_path();
                let config = parse_config(config_path)?;
                process_input(&args.input, args.output.as_deref(), &config).await?;
            } else {
                println!("Use 'onyx init' to initialize a new project or provide a question/command.");
            }
        }
    }

    Ok(())
}

async fn process_input(input: &str, output_format: Option<&str>, config: &Config) -> Result<(), Box<dyn Error>> {
    let ParsedConfig { agent, model, warehouse } = config.load_defaults()?;

    let api_key = env::var(&model.key_var)
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, format!("{} not set", model.key_var)))?;

    let client = Client::new();

    let is_code_output = output_format == Some("code");
    let system_message = if is_code_output {
        &agent.instructions
    } else {
        "You are an AI assistant. Answer the user's question or process their command."
    };

    let content = generate_ai_response(&client, &api_key, system_message, input, &model.model_ref).await?;

    if is_code_output {
        println!("Generated SQL query: {}", content);

        let results = execute_bigquery_query(
            &warehouse.key_path,
            &warehouse.name,
            "default_dataset",
            &content,
        )?;

        let result_json = serde_json::to_string_pretty(&results)?;
        println!("Query result: {}", result_json);

        let interpretation = interpret_results(&client, &api_key, input, &content, &result_json, &model.model_ref).await?;
        println!("Interpretation: {}", interpretation);
    } else {
        println!("Response: {}", content);
    }

    Ok(())
}
