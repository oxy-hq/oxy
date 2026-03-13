use crate::{
    config::model::{LookerIntegration, LookerQueryParams, LookerSortField},
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, EventKind, Output, Table, TableReference},
    },
    observability::events::workflow as workflow_events,
    tools::looker::types::LookerQueryInput,
    types::LookerQuery,
};
use oxy_looker::{InlineQueryRequest, LookerApiClient, LookerAuthConfig, MetadataStorage};
use oxy_shared::errors::OxyError;
use std::collections::HashMap;

/// Shared executor for Looker queries that can be used by both tools and workflow tasks
#[derive(Debug, Clone)]
pub struct LookerQueryExecutable {}

#[async_trait::async_trait]
impl Executable<LookerQueryInput> for LookerQueryExecutable {
    type Response = Output;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = "looker_query.execute",
        oxy.span_type = "looker_query",
        oxy.looker_query.integration = %input.integration,
        oxy.looker_query.model = %input.model,
        oxy.looker_query.explore = %input.explore,
        oxy.looker_query.fields_count = input.params.fields.len(),
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: LookerQueryInput,
    ) -> Result<Output, OxyError> {
        tracing::info!(
            integration = %input.integration,
            model = %input.model,
            explore = %input.explore,
            "Executing Looker query"
        );

        let result = self
            .execute_query(
                execution_context,
                &input.params,
                &input.integration,
                &input.model,
                &input.explore,
            )
            .await;

        if let Ok(ref output) = result {
            tracing::info!("Looker query executed successfully");
            workflow_events::task::looker_query::execute_output(output);
        }

        result
    }
}

impl Default for LookerQueryExecutable {
    fn default() -> Self {
        Self::new()
    }
}

impl LookerQueryExecutable {
    pub fn new() -> Self {
        Self {}
    }

    /// Core execution logic shared between tool and task implementations
    #[tracing::instrument(skip_all, err, fields(
        otel.name = "looker_query.execute_query",
        oxy.span_type = "looker_query",
        oxy.looker_query.integration = integration,
        oxy.looker_query.model = model,
        oxy.looker_query.explore = explore,
        oxy.looker_query.fields_count = params.fields.len(),
    ))]
    pub async fn execute_query(
        &self,
        execution_context: &ExecutionContext,
        params: &LookerQueryParams,
        integration: &str,
        model: &str,
        explore: &str,
    ) -> Result<Output, OxyError> {
        // Validate parameters
        self.validate_params(params)?;

        // Get Looker configuration
        let looker_config = self.get_looker_config(execution_context, integration)?;

        // Resolve client credentials from environment variables
        let client_id = execution_context
            .project
            .secrets_manager
            .resolve_secret(&looker_config.client_id_var)
            .await?
            .ok_or_else(|| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "client_id".to_string(),
                msg: format!(
                    "Looker client ID not found in environment variable: {}",
                    looker_config.client_id_var
                ),
            })?;

        let client_secret = execution_context
            .project
            .secrets_manager
            .resolve_secret(&looker_config.client_secret_var)
            .await?
            .ok_or_else(|| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "client_secret".to_string(),
                msg: format!(
                    "Looker client secret not found in environment variable: {}",
                    looker_config.client_secret_var
                ),
            })?;

        // Create API client
        let auth_config = LookerAuthConfig {
            base_url: looker_config.base_url.clone(),
            client_id,
            client_secret,
        };

        let mut api_client =
            LookerApiClient::new(auth_config).map_err(|e| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "".to_string(),
                msg: format!("Failed to create Looker API client: {}", e),
            })?;

        // Build query request
        let query_request = self
            .build_query_request(execution_context, params, integration, model, explore)
            .await?;

        tracing::info!("Looker query request: {:?}", query_request);

        // Execute query
        let response = api_client
            .run_inline_query(query_request)
            .await
            .map_err(|e| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "".to_string(),
                msg: format!("Query execution failed: {}", e),
            })?;

        let row_count = response.data.len();
        tracing::info!("Query returned {} rows", row_count);

        // Create table output
        let table_output = self.create_table_output(&response, model, explore)?;

        let (result, is_result_truncated) = table_output.to_2d_array()?;
        let sql = table_output
            .reference
            .as_ref()
            .map(|r| r.sql.clone())
            .unwrap_or_default();

        let looker_output = Output::LookerQuery(LookerQuery {
            result,
            is_result_truncated,
            integration: integration.to_string(),
            model: model.to_string(),
            explore: explore.to_string(),
            fields: params.fields.clone(),
            filters: params.filters.clone(),
            sorts: params
                .sorts
                .as_ref()
                .map(|s| s.iter().map(LookerSortField::to_looker_string).collect()),
            limit: params.limit,
            sql,
        });

        let _ = execution_context
            .write_chunk(Chunk {
                key: None,
                delta: looker_output,
                finished: true,
            })
            .await;

        let _ = execution_context
            .write_kind(EventKind::Finished {
                message: format!("Executed Looker query: {} rows returned", row_count),
                attributes: [].into(),
                error: None,
            })
            .await;

        Ok(Output::Table(table_output))
    }

    /// Returns the SQL that Looker would generate for the given query parameters,
    /// without executing the data query.
    pub async fn get_sql(
        &self,
        execution_context: &ExecutionContext,
        params: &LookerQueryParams,
        integration: &str,
        model: &str,
        explore: &str,
    ) -> Result<String, OxyError> {
        self.validate_params(params)?;

        let looker_config = self.get_looker_config(execution_context, integration)?;

        let client_id = execution_context
            .project
            .secrets_manager
            .resolve_secret(&looker_config.client_id_var)
            .await?
            .ok_or_else(|| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "client_id".to_string(),
                msg: format!(
                    "Looker client ID not found in environment variable: {}",
                    looker_config.client_id_var
                ),
            })?;

        let client_secret = execution_context
            .project
            .secrets_manager
            .resolve_secret(&looker_config.client_secret_var)
            .await?
            .ok_or_else(|| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "client_secret".to_string(),
                msg: format!(
                    "Looker client secret not found in environment variable: {}",
                    looker_config.client_secret_var
                ),
            })?;

        let auth_config = LookerAuthConfig {
            base_url: looker_config.base_url.clone(),
            client_id,
            client_secret,
        };

        let mut api_client =
            LookerApiClient::new(auth_config).map_err(|e| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "".to_string(),
                msg: format!("Failed to create Looker API client: {}", e),
            })?;

        let query_request = self
            .build_query_request(execution_context, params, integration, model, explore)
            .await?;

        tracing::debug!(
            model = model,
            explore = explore,
            fields = ?query_request.fields,
            sorts = ?query_request.sorts,
            filters = ?query_request.filters,
            limit = ?query_request.limit,
            "Looker SQL query request"
        );

        let sql = api_client
            .run_inline_query_sql(query_request)
            .await
            .map_err(|e| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "".to_string(),
                msg: format!("SQL generation failed: {}", e),
            })?;

        Ok(sql)
    }

    /// Validate the query parameters
    fn validate_params(&self, params: &LookerQueryParams) -> Result<(), OxyError> {
        if params.fields.is_empty() {
            return Err(OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "fields".to_string(),
                msg: "At least one field must be specified".to_string(),
            });
        }

        // Validate field names are not empty
        for field in &params.fields {
            if field.trim().is_empty() {
                return Err(OxyError::ToolCallError {
                    call_id: "unknown".to_string(),
                    handle: "looker_query".to_string(),
                    param: "fields".to_string(),
                    msg: "Field names cannot be empty".to_string(),
                });
            }
        }

        // Validate limit is reasonable
        if let Some(limit) = params.limit {
            if limit == 0 {
                return Err(OxyError::ToolCallError {
                    call_id: "unknown".to_string(),
                    handle: "looker_query".to_string(),
                    param: "limit".to_string(),
                    msg: "Limit must be greater than 0 or -1 for unlimited".to_string(),
                });
            }
            if limit > 0 && limit > 100000 {
                return Err(OxyError::ToolCallError {
                    call_id: "unknown".to_string(),
                    handle: "looker_query".to_string(),
                    param: "limit".to_string(),
                    msg: "Limit cannot exceed 100,000 rows".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Build the query request from parameters
    async fn build_query_request(
        &self,
        execution_context: &ExecutionContext,
        params: &LookerQueryParams,
        integration: &str,
        model: &str,
        explore: &str,
    ) -> Result<InlineQueryRequest, OxyError> {
        let resolved_view = self
            .resolve_query_view_from_synced_metadata(execution_context, integration, model, explore)
            .await;

        Ok(InlineQueryRequest {
            model: model.to_string(),
            view: resolved_view,
            fields: params.fields.clone(),
            filters: params.filters.clone(),
            filter_expression: params.filter_expression.clone(),
            sorts: params
                .sorts
                .as_ref()
                .map(|s| s.iter().map(LookerSortField::to_looker_string).collect()),
            limit: params.limit,
            query_timezone: None,
            pivots: None,
            fill_fields: None,
        })
    }

    async fn resolve_query_view_from_synced_metadata(
        &self,
        execution_context: &ExecutionContext,
        integration: &str,
        model: &str,
        explore: &str,
    ) -> String {
        let config_manager = &execution_context.project.config_manager;

        let state_dir = match config_manager.resolve_state_dir().await {
            Ok(state_dir) => state_dir,
            Err(error) => {
                tracing::debug!(
                    integration = integration,
                    model = model,
                    explore = explore,
                    error = %error,
                    "Could not resolve state directory for Looker metadata"
                );
                return explore.to_string();
            }
        };

        let storage = MetadataStorage::new(
            state_dir.join(".looker"),
            config_manager.project_path().join("looker"),
            integration.to_string(),
        );

        let metadata = match storage.load_base_metadata(model, explore) {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::debug!(
                    integration = integration,
                    model = model,
                    explore = explore,
                    error = %error,
                    "No synced Looker metadata found; using explore as query view"
                );
                return explore.to_string();
            }
        };

        if let Some(base_view_name) = metadata.base_view_name
            && !base_view_name.trim().is_empty()
        {
            if base_view_name != explore {
                tracing::debug!(
                    integration = integration,
                    model = model,
                    explore = explore,
                    base_view = base_view_name,
                    "Resolved query view from synced Looker metadata"
                );
            }
            return base_view_name;
        }

        explore.to_string()
    }

    /// Get Looker integration configuration from execution context
    fn get_looker_config(
        &self,
        execution_context: &ExecutionContext,
        integration_name: &str,
    ) -> Result<LookerIntegration, OxyError> {
        let config = execution_context.project.config_manager.clone();
        let looker_config = config
            .get_config()
            .integrations
            .iter()
            .find_map(|integration| match &integration.integration_type {
                crate::config::model::IntegrationType::Looker(looker_integration) => {
                    if integration.name == integration_name {
                        Some(looker_integration.clone())
                    } else {
                        None
                    }
                }
                crate::config::model::IntegrationType::Omni(_) => None,
            });

        match looker_config {
            Some(looker_integration) => Ok(looker_integration),
            None => Err(OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "looker_query".to_string(),
                param: "integration".to_string(),
                msg: format!(
                    "Looker integration '{}' not found in configuration",
                    integration_name
                ),
            }),
        }
    }

    /// Create a Table output from the Looker query response
    fn create_table_output(
        &self,
        response: &oxy_looker::QueryResponse,
        model: &str,
        explore: &str,
    ) -> Result<Table, String> {
        let row_count = response.data.len();

        let temp_file_path = self
            .save_json_to_arrow_file(&response.data)
            .map_err(|e| format!("Failed to save query results to Arrow file: {}", e))?;

        let reference = TableReference {
            database_ref: format!("looker::{}.{}", model, explore),
            sql: response.sql.clone().unwrap_or_else(|| "N/A".to_string()),
        };

        let table = Table::with_reference(temp_file_path, reference, None, None);
        tracing::info!(
            "Query result ({} rows) saved to Arrow file: {}",
            row_count,
            table.to_markdown()
        );

        Ok(table)
    }

    /// Convert JSON data to Arrow format and save to a temporary file
    fn save_json_to_arrow_file(
        &self,
        data: &[HashMap<String, serde_json::Value>],
    ) -> Result<String, String> {
        use crate::connector::write_to_ipc;
        use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use std::sync::Arc;

        if data.is_empty() {
            // Return empty table
            let schema = Arc::new(Schema::empty());
            let batch = RecordBatch::new_empty(schema.clone());

            let mut file_path = std::env::temp_dir();
            file_path.push(format!("{}.arrow", uuid::Uuid::new_v4()));
            let file_path_str = file_path.to_string_lossy().to_string();

            write_to_ipc(&vec![batch], &file_path_str, &schema)
                .map_err(|e| format!("Failed to write Arrow data: {}", e))?;

            return Ok(file_path_str);
        }

        // Infer schema from first row; sort keys for deterministic column ordering
        let first_row = &data[0];
        let mut sorted_keys: Vec<&String> = first_row.keys().collect();
        sorted_keys.sort();
        let mut fields = Vec::new();

        for key in sorted_keys {
            let value = first_row.get(key).unwrap();
            let value = Self::normalize_looker_value(value);
            let data_type = match value {
                serde_json::Value::Number(n) => {
                    if n.is_i64() {
                        DataType::Int64
                    } else {
                        DataType::Float64
                    }
                }
                serde_json::Value::Bool(_) => DataType::Boolean,
                _ => DataType::Utf8,
            };
            fields.push(Field::new(key, data_type, true));
        }

        let schema = Arc::new(Schema::new(fields.clone()));

        // Build arrays for each column
        let mut arrays: Vec<ArrayRef> = Vec::new();

        for field in &fields {
            let column_name = field.name();
            let column_values: Vec<_> = data
                .iter()
                .map(|row| row.get(column_name).map(Self::normalize_looker_value))
                .collect();

            let array: ArrayRef = match field.data_type() {
                DataType::Int64 => {
                    let values: Vec<Option<i64>> = column_values
                        .iter()
                        .map(|v| match v {
                            Some(serde_json::Value::Number(n)) => n.as_i64(),
                            _ => None,
                        })
                        .collect();
                    Arc::new(Int64Array::from(values))
                }
                DataType::Float64 => {
                    let values: Vec<Option<f64>> = column_values
                        .iter()
                        .map(|v| match v {
                            Some(serde_json::Value::Number(n)) => n.as_f64(),
                            _ => None,
                        })
                        .collect();
                    Arc::new(Float64Array::from(values))
                }
                _ => {
                    let values: Vec<Option<String>> = column_values
                        .iter()
                        .map(|v| match v {
                            Some(serde_json::Value::String(s)) => Some(s.clone()),
                            Some(serde_json::Value::Null) => None,
                            Some(v) => Some(v.to_string()),
                            None => None,
                        })
                        .collect();
                    Arc::new(StringArray::from(values))
                }
            };

            arrays.push(array);
        }

        // Create record batch
        let batch = RecordBatch::try_new(schema.clone(), arrays)
            .map_err(|e| format!("Failed to create Arrow record batch: {}", e))?;

        // Write to file
        let mut file_path = std::env::temp_dir();
        file_path.push(format!("{}.arrow", uuid::Uuid::new_v4()));
        let file_path_str = file_path.to_string_lossy().to_string();

        write_to_ipc(&vec![batch], &file_path_str, &schema)
            .map_err(|e| format!("Failed to write Arrow data to file: {}", e))?;

        Ok(file_path_str)
    }

    fn normalize_looker_value(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => map
                .get("value")
                .map(Self::normalize_looker_value)
                .unwrap_or_else(|| value.clone()),
            _ => value.clone(),
        }
    }
}
