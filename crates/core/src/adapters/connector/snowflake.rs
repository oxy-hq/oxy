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
        let config = self.config.clone();
        let api = SnowflakeApi::with_password_auth(
            config.account.as_str(),
            Some(config.warehouse.as_str()),
            Some(config.database.as_str()),
            None,
            &config.username,
            config.role.as_deref(),
            &config.get_password().unwrap_or("".to_string()),
        )
        .map_err(|err| connector_internal_error(CREATE_CONN, &err))?;
        let res = api
            .exec(query)
            .await
            .map_err(|err| connector_internal_error(EXECUTE_QUERY, &err))?;
        let record_batches: Vec<RecordBatch>;
        match res {
            QueryResult::Arrow(batches) => {
                record_batches = batches;
            }
            QueryResult::Json(json) => {
                let batches = convert_json_result_to_arrow(&json)?;
                record_batches = batches;
            }
            QueryResult::Empty => return Err(OxyError::DBError("Empty result".to_string())),
        }
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
        .map_err(|err| OxyError::DBError(format!("Failed to infer JSON schema: {}", err)))?;

    let json_string = json_objects.to_string();
    let json_stream_string = json_string[1..json_string.len() - 1]
        .to_string()
        .split(",")
        .join("");
    let cursor = std::io::Cursor::new(json_stream_string);
    let reader = ReaderBuilder::new(Arc::new(arrow_schema))
        .build(cursor)
        .map_err(|err| OxyError::DBError(format!("Failed to create JSON reader: {}", err)))?;
    reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| OxyError::DBError(format!("Failed to convert JSON to Arrow: {}", err)))
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
