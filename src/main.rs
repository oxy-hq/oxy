mod python_interop;
mod yaml_parsers;
mod init;

use clap::{Command, Arg, ArgMatches, Parser};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use serde_json::json;

use crate::yaml_parsers::config_parser::{Config, parse_config};
use crate::python_interop::execute_bigquery_query;

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
            return Ok(());
        },
        None => {
            if !args.input.is_empty() {
                // Process the input with OpenAI
                process_input(&args).await?;
            } else {
                println!("Use 'onyx init' to initialize a new project or provide a question/command.");
            }
        }
    }

    Ok(())
}

async fn process_input(args: &Args) -> Result<(), Box<dyn Error>> {
    let api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let client = Client::new();

    // Parse the configuration
    let config = parse_config()?;
    
    let bigquery_warehouse = config.warehouses.iter()
        .find(|w| w.r#type == "bigquery")
        .expect("No BigQuery warehouse found in config.yml");

    // Determine if we're generating SQL or processing a general query
    let is_code_output = args.output.as_deref() == Some("code");
    let system_message = if is_code_output {
        "You are a SQL expert. Generate a SQL query for BigQuery based on the user's question. Respond with only the SQL query, no explanations."
    } else {
        "You are an AI assistant. Answer the user's question or process their command."
    };

    // Step 1: Generate response using OpenAI
    let request = json!({
        "model": "gpt-3.5-turbo",
        "messages": [
            {
                "role": "system",
                "content": system_message
            },
            {
                "role": "user",
                "content": &args.input
            }
        ]
    });

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let content = response["choices"][0]["message"]["content"]
        .as_str()
        .expect("Failed to get content from OpenAI");

    if is_code_output {
        println!("Generated SQL query: {}", content);

        // Execute SQL against BigQuery
        let results = execute_bigquery_query(
            &bigquery_warehouse.key_path,
            &bigquery_warehouse.name,
            "default_dataset",
            content,
        )?;

        let result_json = serde_json::to_string_pretty(&results)?;
        println!("Query result: {}", result_json);

        // Interpret results using OpenAI
        let interpret_request = json!({
            "model": "gpt-3.5-turbo",
            "messages": [
                {
                    "role": "system",
                    "content": "You are a data analyst. Interpret the following query results and provide a concise summary."
                },
                {
                    "role": "user",
                    "content": format!("Question: {}\\n\\nSQL Query: {}\\n\\nQuery Results: {}", args.input, content, result_json)
                }
            ]
        });

        let interpret_response = client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&interpret_request)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let interpretation = interpret_response["choices"][0]["message"]["content"]
            .as_str()
            .expect("Failed to get interpretation from OpenAI");

        println!("Interpretation: {}", interpretation);
    } else {
        println!("Response: {}", content);
    }

    Ok(())
}
