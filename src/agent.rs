use crate::yaml_parsers::agent_parser::MessagePair;
use crate::yaml_parsers::config_parser::ParsedConfig;
use crate::yaml_parsers::entity_parser::EntityConfig;
use arrow::record_batch::RecordBatch;
use arrow_cast::pretty::{pretty_format_batches, print_batches};
use connectorx::prelude::{get_arrow, CXQuery, SourceConn};
use minijinja::context;
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
    instructions: MessagePair,
    tools: Vec<String>,
    postscript: MessagePair,
    entity_config: EntityConfig,
}

impl Agent {
    pub fn new(parsed_config: ParsedConfig, entity_config: EntityConfig) -> Self {
        let model_ref = parsed_config.model.model_ref;
        let model_key_var = parsed_config.model.key_var;
        let model_key = env::var(&model_key_var).expect("Environment variable not found");
        let warehouse_key_path = parsed_config.warehouse.key_path;
        let instructions = parsed_config.agent_config.instructions;

        let tools = parsed_config.agent_config.tools;
        let postscript = parsed_config.agent_config.postscript;

        Agent {
            client: Client::new(),
            model_ref,
            model_key,
            warehouse_key_path,
            instructions,
            tools,
            postscript,
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
        let (system_message, user_message) = self
            .compile_postscript(input, Some(sql_query), Some(result_string), None)
            .await?;

        self.generate_ai_response(&system_message, &user_message)
            .await
    }

    pub async fn generate_sql_query(&self, input: &str) -> Result<String, Box<dyn Error>> {
        let (system_message, user_message) = self.compile_instructions(input).await?;
        self.generate_ai_response(&system_message, &user_message)
            .await
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
        // Uses `instructions` from agent config
        let sql_query = self.generate_sql_query(input).await?;
        println!("Generated SQL query: {}", sql_query);

        // Execute query
        let record_batches = self.execute_bigquery_query(&sql_query).await?;
        let result_string = pretty_format_batches(&record_batches)?.to_string();
        print_batches(&record_batches)?;

        // Uses `postscript` from agent config
        let interpretation = self
            .interpret_results(input, &sql_query, &result_string)
            .await?;
        println!("Interpretation: {}", interpretation);

        Ok(())
    }

    pub async fn compile_instructions(
        &self,
        input: &str,
    ) -> Result<(String, String), Box<dyn Error>> {
        let ctx = context! {
            input => input,
            entities => self.entity_config.format_entities(),
            metrics => self.entity_config.format_metrics(),
            analyses => self.entity_config.format_analyses(),
            schema => self.entity_config.format_schema(),
        };

        self.instructions.compile(ctx)
    }

    pub async fn compile_postscript(
        &self,
        input: &str,
        sql_query: Option<&str>,
        sql_results: Option<&str>,
        retrieve_results: Option<&str>,
    ) -> Result<(String, String), Box<dyn Error>> {
        let ctx = context! {
            input => input,
            sql_query => sql_query,
            sql_results => sql_results,
            retrieve_results => retrieve_results,
        };

        self.postscript.compile(ctx)
    }
}
