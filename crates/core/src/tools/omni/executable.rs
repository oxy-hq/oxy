use crate::{
    config::model::OmniIntegration,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        types::{Chunk, EventKind, Output},
    },
    tools::{omni::types::OmniQueryInput, types::OmniQueryParams},
};
use arrow::ipc::reader::StreamReader;
use base64::{Engine as _, engine::general_purpose};
use omni::{OmniApiClient, QueryRequest, QueryStructure, SortField};
use std::io::Cursor;

/// Shared executor for Omni queries that can be used by both tools and workflow tasks
#[derive(Debug, Clone)]
pub struct OmniQueryExecutable {}

#[async_trait::async_trait]
impl Executable<OmniQueryInput> for OmniQueryExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: OmniQueryInput,
    ) -> Result<Output, OxyError> {
        self.execute_query(
            execution_context,
            &input.params,
            &input.integration,
            &input.topic,
        )
        .await
    }
}

impl OmniQueryExecutable {
    pub fn new() -> Self {
        Self {}
    }

    /// Core execution logic shared between tool and task implementations
    pub async fn execute_query(
        &self,
        execution_context: &ExecutionContext,
        params: &OmniQueryParams,
        integration: &str,
        topic: &str,
    ) -> Result<Output, OxyError> {
        // Validate parameters
        self.validate_params(params)?;

        // Emit query generated event
        execution_context
            .write_kind(EventKind::OmniQueryGenerated {
                query: params.clone(),
                is_verified: true,
            })
            .await?;

        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::OmniQuery(params.clone()),
                finished: true,
            })
            .await?;

        // Get Omni configuration
        let omni_config = self.get_omni_config(execution_context, integration)?;

        // Resolve API key from environment variable
        let api_key = execution_context
            .project
            .secrets_manager
            .resolve_secret(&omni_config.api_key_var)
            .await?
            .ok_or_else(|| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "omni_query".to_string(),
                param: "".to_string(),
                msg: "Omni API key not found".to_string(),
            })?;

        // Create API client
        let api_client =
            OmniApiClient::new(omni_config.base_url.clone(), api_key).map_err(|e| {
                OxyError::ToolCallError {
                    call_id: "unknown".to_string(),
                    handle: "omni_query".to_string(),
                    param: "".to_string(),
                    msg: format!("Failed to create Omni API client: {}", e),
                }
            })?;

        // Build query structure
        let query_structure = self.build_query_structure(params, topic, omni_config.clone())?;

        // Create query request
        let query_request = QueryRequest::builder()
            .query(query_structure)
            .build()
            .map_err(|e| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "omni_query".to_string(),
                param: "".to_string(),
                msg: format!("Failed to build query request: {}", e),
            })?;

        tracing::info!("Omni query request {:?}", query_request);

        // Execute query
        let response =
            api_client
                .execute_query(query_request)
                .await
                .map_err(|e| OxyError::ToolCallError {
                    call_id: "unknown".to_string(),
                    handle: "omni_query".to_string(),
                    param: "".to_string(),
                    msg: format!("Query execution failed: {}", e),
                })?;

        let table_output = self.create_table_output(&response)?;

        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: table_output.clone(),
                finished: true,
            })
            .await?;

        execution_context
            .write_kind(EventKind::Finished {
                message: "Executed Omni query".to_string(),
                attributes: [].into(),
                error: None,
            })
            .await?;

        Ok(table_output)
    }

    /// Validate the query parameters
    fn validate_params(&self, params: &OmniQueryParams) -> Result<(), OxyError> {
        if params.fields.is_empty() {
            return Err(OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "omni_query".to_string(),
                param: "".to_string(),
                msg: "At least one field must be specified".to_string(),
            });
        }

        // Validate field names are not empty
        for field in &params.fields {
            if field.trim().is_empty() {
                return Err(OxyError::ToolCallError {
                    call_id: "unknown".to_string(),
                    handle: "omni_query".to_string(),
                    param: "".to_string(),
                    msg: "Field names cannot be empty".to_string(),
                });
            }
        }

        // Validate limit is reasonable
        if let Some(limit) = params.limit {
            if limit == 0 {
                return Err(OxyError::ToolCallError {
                    call_id: "unknown".to_string(),
                    handle: "omni_query".to_string(),
                    param: "".to_string(),
                    msg: "Limit must be greater than 0".to_string(),
                });
            }
            if limit > 10000 {
                return Err(OxyError::ToolCallError {
                    call_id: "unknown".to_string(),
                    handle: "omni_query".to_string(),
                    param: "".to_string(),
                    msg: "Limit cannot exceed 10,000 rows".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Build the query structure from parameters
    fn build_query_structure(
        &self,
        params: &OmniQueryParams,
        topic: &str,
        omni_config: OmniIntegration,
    ) -> Result<QueryStructure, OxyError> {
        let topic_config = omni_config
            .topics
            .iter()
            .find(|t| t.name == topic)
            .ok_or_else(|| OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "omni_query".to_string(),
                param: "".to_string(),
                msg: format!("Topic '{}' not found in Omni configuration", topic),
            })?;

        let mut builder = QueryStructure::builder()
            .topic(topic)
            .fields(params.fields.clone())
            .model_id(topic_config.model_id.clone());

        if let Some(limit) = params.limit {
            builder = builder.limit(limit as u32);
        }

        // Convert sorts if provided
        if let Some(sorts) = &params.sorts {
            let sort_fields: Vec<SortField> = sorts
                .iter()
                .map(|(field, order)| SortField {
                    field: field.clone(),
                    sort_descending: match order {
                        crate::tools::types::OrderType::Ascending => false,
                        crate::tools::types::OrderType::Descending => true,
                    },
                })
                .collect();
            builder = builder.sorts(sort_fields);
        }

        builder.build().map_err(|e| OxyError::ToolCallError {
            call_id: "unknown".to_string(),
            handle: "omni_query".to_string(),
            param: "".to_string(),
            msg: format!("Failed to build query structure: {}", e),
        })
    }

    /// Get Omni integration configuration from execution context
    fn get_omni_config(
        &self,
        execution_context: &ExecutionContext,
        integration_name: &str,
    ) -> Result<OmniIntegration, OxyError> {
        let config = execution_context.project.config_manager.clone();
        let omni_config = config
            .get_config()
            .integrations
            .iter()
            .find_map(|integration| match &integration.integration_type {
                crate::config::model::IntegrationType::Omni(omni_integration) => {
                    if integration.name == integration_name {
                        Some(omni_integration.clone())
                    } else {
                        None
                    }
                }
            });

        match omni_config {
            Some(omni_integration) => Ok(omni_integration),
            None => Err(OxyError::ToolCallError {
                call_id: "unknown".to_string(),
                handle: "omni_query".to_string(),
                param: "".to_string(),
                msg: "Omni integration is not configured".to_string(),
            }),
        }
    }

    /// Create a Table output from the omni query response
    fn create_table_output(&self, response: &omni::QueryResponse) -> Result<Output, String> {
        use crate::execute::types::{Table, TableReference};

        // Check if query was successful
        if let Some(status) = response.status.clone() {
            if status == "FAILED" {
                return Err(format!(
                    "Query execution failed: {}",
                    response.error_message.as_deref().unwrap_or("Unknown error")
                ));
            }
        } else if response.has_timed_out() {
            return Err("Query timed out and requires polling".to_string());
        }

        if let Some(result_data) = &response.result {
            // Save Arrow data to temporary file
            let temp_file_path = self
                .save_arrow_data_to_file(result_data)
                .map_err(|e| format!("Failed to save Arrow data to file: {}", e))?;

            let sql = if let Some(summary) = response.summary.clone() {
                summary.display_sql.unwrap_or("Unknown".to_string())
            } else {
                "Unknown".to_string()
            };

            // Create table reference
            let reference = TableReference {
                database_ref: "omni".to_string(),
                sql,
            };

            // Create table with the file path
            let table = Table::with_reference(temp_file_path, reference, None, None);
            tracing::info!("table output: {:?}", table.to_markdown());

            Ok(Output::Table(table))
        } else {
            Err("Query completed but no result data available".to_string())
        }
    }

    /// Save base64 encoded Arrow data to a file and return the file path
    fn save_arrow_data_to_file(&self, base64_data: &str) -> Result<String, String> {
        use crate::adapters::connector::write_to_ipc;

        // Decode base64
        let arrow_bytes = general_purpose::STANDARD
            .decode(base64_data)
            .map_err(|e| format!("Failed to decode base64: {}", e))?;

        // Create a cursor from the bytes and read the Arrow stream
        let cursor = Cursor::new(arrow_bytes);
        let reader = StreamReader::try_new(cursor, None)
            .map_err(|e| format!("Failed to create Arrow stream reader: {}", e))?;

        let mut record_batches = Vec::new();
        let schema = reader.schema();

        // Read all record batches
        for batch_result in reader {
            let batch = batch_result.map_err(|e| format!("Failed to read Arrow batch: {}", e))?;
            record_batches.push(batch);
        }

        let mut file_path = std::env::temp_dir();
        file_path.push(format!("{}.arrow", uuid::Uuid::new_v4()));
        let file_path = file_path.to_string_lossy().to_string();

        // Write Arrow data to file using the proper IPC file format
        write_to_ipc(&record_batches, &file_path, &schema)
            .map_err(|e| format!("Failed to write Arrow data to file: {}", e))?;

        Ok(file_path)
    }
}
