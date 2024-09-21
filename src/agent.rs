use crate::yaml_parsers::config_parser::ParsedConfig;
use crate::yaml_parsers::entity_parser::EntityConfig;
use arrow::record_batch::RecordBatch;
use arrow_cast::pretty::{pretty_format_batches, print_batches};
use connectorx::prelude::{get_arrow, CXQuery, SourceConn};
use reqwest::Client;
use serde_json::json;
use std::convert::TryFrom;
use std::env;
use std::error::Error;

pub struct Agent {
    client: Client,
    model_ref: String,
    model_key: String,
    warehouse_key_path: String,
    entity_config: EntityConfig,
}

impl Agent {
    pub fn new(parsed_config: ParsedConfig, entity_config: EntityConfig) -> Self {
        let model_ref = parsed_config.model.model_ref;
        let model_key_var = parsed_config.model.key_var;
        let model_key = env::var(&model_key_var).expect("Environment variable not found");
        let warehouse_key_path = parsed_config.warehouse.key_path;

        Agent {
            client: Client::new(),
            model_ref,
            model_key,
            warehouse_key_path,
            entity_config,
        }
    }

    pub async fn generate_ai_response(
        &self,
        system_message: &str,
        user_input: &str,
    ) -> Result<String, Box<dyn Error>> {
        let request = json!({
            "model": self.model_ref,
            "messages": [
                {
                    "role": "system",
                    "content": system_message
                },
                {
                    "role": "user",
                    "content": user_input
                }
            ]
        });

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.model_key))
            .json(&request)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        Ok(response["choices"][0]["message"]["content"]
            .as_str()
            .expect("Failed to get content from OpenAI")
            .to_string())
    }

    pub async fn interpret_results(
        &self,
        input: &str,
        sql_query: &str,
        result_string: &str,
    ) -> Result<String, Box<dyn Error>> {
        let system_message = "You are a data analyst. Interpret the following query results and provide a concise summary.";
        let user_message = format!(
            "Question: {}\n\nSQL Query: {}\n\nQuery Results: {}",
            input, sql_query, result_string
        );

        self.generate_ai_response(system_message, &user_message)
            .await
    }

    pub async fn generate_sql_query(&self, input: &str) -> Result<String, Box<dyn Error>> {
        let system_message = "You are an SQL expert. Your task is to generate SQL queries based on user requests. Provide only the SQL query without any explanation or additional text.";

        let user_message = format!("Generate a SQL query for the following request: {}", input);

        let sql_query = self
            .generate_ai_response(system_message, &user_message)
            .await?;

        // Basic validation to ensure the response looks like a SQL query
        if !sql_query.trim().to_lowercase().starts_with("select") {
            return Err("Generated response does not appear to be a valid SQL query".into());
        }

        Ok(sql_query)
    }

    async fn execute_bigquery_query(
        &self,
        query: &str,
    ) -> Result<Vec<RecordBatch>, Box<dyn Error>> {
        let conn_string = format!("bigquery://{}", self.warehouse_key_path);
        let query = query.to_string(); // convert to owned string for closure

        let result = tokio::task::spawn_blocking(move || {
            let source_conn = SourceConn::try_from(conn_string.as_str())?;
            let queries = &[CXQuery::from(query.as_str())];
            let destination =
                get_arrow(&source_conn, None, queries).expect("Run failed at get_arrow.");
            destination.arrow()
        })
        .await??;

        Ok(result)
    }

    pub async fn execute_chain(&self, input: &str) -> Result<(), Box<dyn Error>> {
        let sql_query = self.generate_sql_query(input).await?;
        println!("Generated SQL query: {}", sql_query);

        let record_batches = self.execute_bigquery_query(&sql_query).await?;
        let result_string = pretty_format_batches(&record_batches)?;
        print_batches(&record_batches)?;

        let interpretation = self
            .interpret_results(input, &sql_query, &result_string.to_string())
            .await?;
        println!("Interpretation: {}", interpretation);

        Ok(())
    }
}
