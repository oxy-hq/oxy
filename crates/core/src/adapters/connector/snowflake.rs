use arrow::datatypes::SchemaRef;
use arrow::json::ReaderBuilder;
use arrow::json::reader::infer_json_schema;
use arrow::record_batch::RecordBatch;
use itertools::Itertools;
use snowflake_api::{QueryResult, SnowflakeApi};
use std::sync::Arc;

use crate::config::model::Snowflake as SnowflakeConfig;
use crate::errors::OxyError;

use super::constants::{CREATE_CONN, EXECUTE_QUERY};
use super::engine::Engine;
use super::utils::connector_internal_error;

#[derive(Debug)]
pub(super) struct Snowflake {
    pub config: SnowflakeConfig,
}

impl Snowflake {
    pub fn new(config: SnowflakeConfig) -> Self {
        Self { config }
    }
}

impl Engine for Snowflake {
    async fn run_query_with_limit(
        &self,
        query: &str,
        _dry_run_limit: Option<u64>,
    ) -> Result<(Vec<RecordBatch>, SchemaRef), OxyError> {
        tracing::info!("🚀 Snowflake: Starting query execution");
        tracing::debug!("🔍 Snowflake query: {}", query);
        
        let config = self.config.clone();
        let api = if let Some(private_key_path) = &config.private_key_path {
            tracing::info!("🔐 Snowflake: Using private key authentication from: {}", private_key_path.display());
            // Use private key authentication
            let private_key_content = std::fs::read_to_string(private_key_path)
                .map_err(|err| OxyError::ConfigurationError(format!("Failed to read private key file: {}", err)))?;
            
            SnowflakeApi::with_certificate_auth(
                config.account.as_str(),
                Some(config.warehouse.as_str()),
                Some(config.database.as_str()),
                None,
                &config.username,
                config.role.as_deref(),
                &private_key_content,
            )
            .map_err(|err| {
                tracing::error!("❌ Snowflake: Failed to create connection with private key: {}", err);
                connector_internal_error(CREATE_CONN, &err)
            })?
        } else {
            tracing::info!("🔑 Snowflake: Using password authentication");
            // Use password authentication
            SnowflakeApi::with_password_auth(
                config.account.as_str(),
                Some(config.warehouse.as_str()),
                Some(config.database.as_str()),
                None,
                &config.username,
                config.role.as_deref(),
                &config.get_password().await?,
            )
            .map_err(|err| {
                tracing::error!("❌ Snowflake: Failed to create connection with password: {}", err);
                connector_internal_error(CREATE_CONN, &err)
            })?
        };
        
        tracing::info!("✅ Snowflake: Connection established successfully");
        tracing::info!("⚡ Snowflake: Executing query...");
        
        let res = api
            .exec(query)
            .await
            .map_err(|err| {
                tracing::error!("❌ Snowflake: Query execution failed: {}", err);
                connector_internal_error(EXECUTE_QUERY, &err)
            })?;
        let record_batches: Vec<RecordBatch>;
        match res {
            QueryResult::Arrow(batches) => {
                tracing::info!("📊 Snowflake: Received Arrow result with {} batches", batches.len());
                record_batches = batches;
            }
            QueryResult::Json(json) => {
                tracing::info!("📄 Snowflake: Received JSON result, converting to Arrow...");
                let batches = convert_json_result_to_arrow(&json)?;
                tracing::info!("✅ Snowflake: Converted JSON to {} Arrow batches", batches.len());
                record_batches = batches;
            }
            QueryResult::Empty => {
                tracing::warn!("⚠️ Snowflake: Query returned empty result");
                return Err(OxyError::DBError("Empty result".to_string()));
            }
        }
        
        if record_batches.is_empty() {
            tracing::warn!("⚠️ Snowflake: No record batches returned");
            return Err(OxyError::DBError("No record batches returned".to_string()));
        }
        
        let total_rows: usize = record_batches.iter().map(|batch| batch.num_rows()).sum();
        tracing::info!("🎯 Snowflake: Query completed successfully - {} batches, {} total rows", record_batches.len(), total_rows);
        
        let schema = record_batches[0].schema();
        Ok((record_batches, schema))
    }
}

fn convert_json_result_to_arrow(
    json: &snowflake_api::JsonResult,
) -> Result<Vec<RecordBatch>, OxyError> {
    let json_objects = convert_to_json_objects(json);
    let infer_cursor = std::io::Cursor::new(json_objects[0].to_string());
    let (arrow_schema, _) = infer_json_schema(infer_cursor, None)
        .map_err(|err| OxyError::DBError(format!("Failed to infer JSON schema: {err}")))?;

    let json_string = json_objects.to_string();
    let json_stream_string = json_string[1..json_string.len() - 1]
        .to_string()
        .split(",")
        .join("");
    let cursor = std::io::Cursor::new(json_stream_string);
    let reader = ReaderBuilder::new(Arc::new(arrow_schema))
        .build(cursor)
        .map_err(|err| OxyError::DBError(format!("Failed to create JSON reader: {err}")))?;
    reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| OxyError::DBError(format!("Failed to convert JSON to Arrow: {err}")))
}

fn convert_to_json_objects(json: &snowflake_api::JsonResult) -> serde_json::Value {
    let mut rs: Vec<serde_json::Value> = vec![];
    if let serde_json::Value::Array(values) = &json.value {
        for value in values {
            let mut m: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            if let serde_json::Value::Array(inner_values) = value {
                for field in &json.schema {
                    let field_name = field.name.clone();
                    let field_index = json
                        .schema
                        .iter()
                        .position(|x| x.name == field_name)
                        .unwrap();
                    let field_value = inner_values[field_index].clone();
                    m.insert(field_name, field_value);
                }
            }
            rs.push(serde_json::Value::Object(m));
        }
    }
    serde_json::Value::Array(rs)
}
