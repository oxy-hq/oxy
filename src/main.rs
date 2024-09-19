mod python_interop;
mod yaml_parsers;

use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use serde_json::json;

use crate::yaml_parsers::config_parser::{Config, parse_config};
use crate::python_interop::execute_bigquery_query;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The question to ask
    question: String,

    /// Output format: 'text' (default) or 'code' for SQL
    #[arg(long, value_name = "FORMAT")]
    output: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let client = Client::new();

    // Parse the configuration
    let config = parse_config()?;
    
    let bigquery_warehouse = config.warehouses.iter()
        .find(|w| w.r#type == "bigquery")
        .expect("No BigQuery warehouse found in config.yml");

    // Step 1: Generate SQL using OpenAI
    let sql_request = json!({
        "model": "gpt-3.5-turbo",
        "messages": [
            {
                "role": "system",
                "content": "You are a SQL expert. Generate a SQL query for BigQuery based on the user's question. Respond with only the SQL query, no explanations."
            },
            {
                "role": "user",
                "content": args.question
            }
        ]
    });

    let sql_response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&sql_request)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let sql_query = sql_response["choices"][0]["message"]["content"]
        .as_str()
        .expect("Failed to get SQL query from OpenAI");

    println!("Generated SQL query: {}", sql_query);

    // Step 2: Execute SQL against BigQuery
    let results = execute_bigquery_query(
        &bigquery_warehouse.key_path,
        &bigquery_warehouse.name,
        "default_dataset", // You might want to add this to the Warehouse struct or use a default
        sql_query,
    )?;

    let result_json = serde_json::to_string_pretty(&results)?;
    println!("Query result: {}", result_json);

    // Step 3: Interpret results using OpenAI
    let interpret_request = json!({
        "model": "gpt-3.5-turbo",
        "messages": [
            {
                "role": "system",
                "content": "You are a data analyst. Interpret the following query results and provide a concise summary."
            },
            {
                "role": "user",
                "content": format!("Question: {}\\n\\nSQL Query: {}\\n\\nQuery Results: {}", args.question, sql_query, result_json)
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

    Ok(())
}
