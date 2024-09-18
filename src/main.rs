mod config_parser;

use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fs;
use serde_json::json;
use google_cloud_bigquery::client::{ClientConfig, Client as BigQueryClient};
use google_cloud_bigquery::client::google_cloud_auth::credentials::CredentialsFile;
use google_cloud_bigquery::http::job::query::QueryRequest;
use google_cloud_bigquery::query::row::Row;

use crate::config_parser::{parse_entity_config, format_system_message};

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

#[derive(Deserialize)]
struct Config {
    connections: Vec<Connection>,
}

#[derive(Deserialize)]
struct Connection {
    name: String,
    r#type: String,
    key_path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let client = Client::new();

    // Read and parse config.yml
    let config_content = fs::read_to_string("config.yml")?;
    let config: Config = serde_yaml::from_str(&config_content)?;
    let bigquery_connection = config.connections.iter()
        .find(|c| c.r#type == "bigquery")
        .expect("No BigQuery connection found in config.yml");

    // Step 1: Generate SQL using OpenAI
    let sql_request = json!({
        "model": "gpt-3.5-turbo-0613",
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
    let cred = CredentialsFile::new_from_file(bigquery_connection.key_path).await?;
    let (config, project_id) = ClientConfig::new_with_credentials(cred).await?;
    let bigquery_client = BigQueryClient::new(config).await?;

    let request = QueryRequest {
        query: sql_query.to_string(),
        ..Default::default()
    };

    let mut iter = bigquery_client.query::<Row>(&project_id.unwrap(), request).await?;
    let mut results = Vec::new();
    while let Some(row) = iter.next().await? {
        let mut row_data = Vec::new();
        let mut index = 0;
        loop {
            match row.column::<String>(index) {
                Ok(value) => row_data.push(value),
                Err(_) => break, // Assume we've reached the end of the row
            }
            index += 1;
        }
        results.push(row_data);
    }

    let result_json = serde_json::to_string_pretty(&results)?;
    println!("Query result: {}", result_json);

    // Step 3: Interpret results using OpenAI
    let interpret_request = json!({
        "model": "gpt-3.5-turbo-0613",
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
