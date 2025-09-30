use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::{
    adapters::connector::Connector,
    config::model::{
        SemanticFilter, SemanticOperator, SemanticOrder, SemanticOrderDirection, SemanticQueryTask,
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        renderer::Renderer,
        types::{Chunk, EventKind, Output, TableReference},
    },
    service::types::SemanticQueryParams,
};

use super::semantic_validator::{ValidatedSemanticQuery, validate_semantic_query_task};

pub fn render_semantic_query(
    renderer: &Renderer,
    task: &SemanticQueryTask,
) -> Result<SemanticQueryTask, OxyError> {
    let topic = render_string(renderer, &task.query.topic, "topic")?;
    let dimensions = task
        .query
        .dimensions
        .iter()
        .map(|d| render_string(renderer, d, "dimension"))
        .collect::<Result<Vec<_>, _>>()?;
    let measures = task
        .query
        .measures
        .iter()
        .map(|m| render_string(renderer, m, "measure"))
        .collect::<Result<Vec<_>, _>>()?;

    let filters = task
        .query
        .filters
        .iter()
        .map(|f| {
            // Convert SemanticQueryFilter to SemanticFilter for rendering
            let op = match f.op.as_str() {
                "eq" => SemanticOperator::Eq,
                "neq" => SemanticOperator::Neq,
                "gt" => SemanticOperator::Gt,
                "gte" => SemanticOperator::Gte,
                "lt" => SemanticOperator::Lt,
                "lte" => SemanticOperator::Lte,
                "in" => SemanticOperator::In,
                "not_in" => SemanticOperator::NotIn,
                _ => SemanticOperator::Eq, // Default fallback
            };
            let filter = SemanticFilter {
                field: f.field.clone(),
                op,
                value: f.value.clone(),
            };
            let rendered_filter = render_filter(renderer, &filter)?;
            // Convert back to SemanticQueryFilter
            Ok(crate::service::types::SemanticQueryFilter {
                field: rendered_filter.field,
                op: f.op.clone(), // Keep original string representation
                value: rendered_filter.value,
            })
        })
        .collect::<Result<Vec<_>, OxyError>>()?;

    let orders = task
        .query
        .orders
        .iter()
        .map(|o| {
            let rendered_field = render_string(renderer, &o.field, "order.field")?;
            Ok(crate::service::types::SemanticQueryOrder {
                field: rendered_field,
                direction: o.direction.clone(),
            })
        })
        .collect::<Result<Vec<_>, OxyError>>()?;

    Ok(SemanticQueryTask {
        query: SemanticQueryParams {
            topic,
            dimensions,
            measures,
            filters,
            orders,
            limit: task.query.limit,
            offset: task.query.offset,
        },
        export: task.export.clone(),
    })
}

fn render_string(renderer: &Renderer, value: &str, ctx: &str) -> Result<String, OxyError> {
    renderer.render(value).map_err(|e| {
        OxyError::RuntimeError(format!(
            "Failed to render semantic query {ctx} template '{value}': {e}"
        ))
    })
}

fn render_filter(renderer: &Renderer, filter: &SemanticFilter) -> Result<SemanticFilter, OxyError> {
    let field = render_string(renderer, &filter.field, "filter.field")?;
    let value = render_filter_value(renderer, &filter.value, &field)?;
    Ok(SemanticFilter {
        field,
        op: filter.op.clone(),
        value,
    })
}

fn render_filter_value(
    renderer: &Renderer,
    value: &JsonValue,
    field_ctx: &str,
) -> Result<JsonValue, OxyError> {
    match value {
        JsonValue::String(s) => render_filter_string(renderer, s, field_ctx),
        JsonValue::Array(arr) => {
            let mut new_arr = Vec::with_capacity(arr.len());
            for (idx, item) in arr.iter().enumerate() {
                match item {
                    JsonValue::String(s) => {
                        // Support expression templating in array elements
                        new_arr.push(render_filter_string(
                            renderer,
                            s,
                            &format!("{field_ctx}[{idx}]"),
                        )?);
                    }
                    other => new_arr.push(other.clone()),
                }
            }
            Ok(JsonValue::Array(new_arr))
        }
        _ => Ok(value.clone()),
    }
}

fn render_filter_string(
    renderer: &Renderer,
    template: &str,
    ctx: &str,
) -> Result<JsonValue, OxyError> {
    let trimmed = template.trim();
    let is_expression = trimmed.starts_with("{{") && trimmed.ends_with("}}");
    if is_expression {
        match renderer.eval_expression(trimmed) {
            Ok(val) => {
                // Convert minijinja::Value to serde_json::Value; fall back to original string if null
                let json_value = serde_json::to_value(&val).unwrap_or(JsonValue::Null);
                if json_value.is_null() {
                    Ok(JsonValue::String(template.to_string()))
                } else {
                    Ok(json_value)
                }
            }
            Err(e) => Err(OxyError::RuntimeError(format!(
                "Failed to evaluate semantic query filter value expression for {ctx}: {template}: {e}"
            ))),
        }
    } else {
        render_string(renderer, template, &format!("filter.value {ctx}")).map(JsonValue::String)
    }
}

/// ParamMapper for semantic query tasks that handles templating and validation
#[derive(Clone)]
struct SemanticQueryTaskMapper;

#[async_trait::async_trait]
impl ParamMapper<SemanticQueryTask, ValidatedSemanticQuery> for SemanticQueryTaskMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: SemanticQueryTask,
    ) -> Result<(ValidatedSemanticQuery, Option<ExecutionContext>), OxyError> {
        // Task 3.1: Pre-Execution Templating
        let rendered_task = render_semantic_query(&execution_context.renderer, &input)?;

        // Task 3.2: Metadata Validation
        let validated_query =
            validate_semantic_query_task(&execution_context.project.config_manager, &rendered_task)
                .await?;

        Ok((validated_query, None))
    }
}

// SemanticQueryExecutable - implements Task 4.2
#[derive(Clone)]
pub struct SemanticQueryExecutable;

impl Default for SemanticQueryExecutable {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticQueryExecutable {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Executable<ValidatedSemanticQuery> for SemanticQueryExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: ValidatedSemanticQuery,
    ) -> Result<Self::Response, OxyError> {
        execution_context
            .write_kind(EventKind::Started {
                name: format!("Semantic Query: {}", input.task.query.topic),
                attributes: HashMap::from_iter([(
                    "topic".to_string(),
                    input.task.query.topic.clone(),
                )]),
            })
            .await?;

        tracing::info!(
            "Executing semantic query for topic '{}': {:?}",
            input.task.query.topic,
            input.task.query
        );

        // Step 1: Convert to CubeJS query and get SQL
        let cubejs_query = self.convert_to_cubejs_query(&input.task, &input.topic.name)?;
        tracing::info!(
            "Generated CubeJS query for topic '{}': {cubejs_query:?}",
            input.task.query.topic
        );

        let sql_query = self.get_sql_from_cubejs(&cubejs_query).await?;

        // Determine database from topic's views
        let database = self.determine_database_from_topic(&input)?;

        // Emit an event showing the generated SQL
        execution_context
            .write_kind(EventKind::SemanticQueryGenerated {
                query: input.task.query.clone(),
                is_verified: true,
            })
            .await?;

        // Step 2: Execute SQL directly using database connector and save results
        let file_path = self
            .execute_sql_and_save_results(
                &sql_query,
                &database,
                &input.task.query.topic,
                execution_context,
            )
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!(
                    "Failed to execute semantic query for topic '{}': {e}",
                    input.task.query.topic
                ))
            })?;

        // Build table output (leveraging existing table/reference system) and emit as chunk
        let table_output = Output::table_with_reference(
            file_path.clone(),
            TableReference {
                sql: sql_query,
                database_ref: database.clone(),
            },
            None,
        );
        execution_context
            .write_chunk(Chunk {
                key: None,
                delta: table_output.clone(),
                finished: true,
            })
            .await?;

        execution_context
            .write_kind(EventKind::Finished {
                message: format!(
                    "Executed semantic query for topic '{}' - results written to {}",
                    input.task.query.topic, file_path
                ),
                attributes: [].into(),
                error: None,
            })
            .await?;

        // Return Table output with semantic query reference
        Ok(table_output)
    }
}

impl SemanticQueryExecutable {
    /// Determine the database from the topic's views
    fn determine_database_from_topic(
        &self,
        input: &ValidatedSemanticQuery,
    ) -> Result<String, OxyError> {
        // Check if any view has a datasource specified
        for view in &input.views {
            if let Some(datasource) = &view.datasource {
                return Ok(datasource.clone());
            }
        }

        // If no datasource is found in views, try to infer from metadata or use a default
        // For now, we'll return an error indicating that datasource must be specified
        Err(OxyError::ValidationError(format!(
            "No datasource found for topic '{}'. At least one view in the topic must specify a datasource.",
            input.task.query.topic
        )))
    }

    /// Convert validated semantic query to CubeJS query JSON format
    fn convert_to_cubejs_query(
        &self,
        task: &SemanticQueryTask,
        topic_name: &str,
    ) -> Result<JsonValue, OxyError> {
        let mut query = serde_json::json!({
            "measures": task.query.measures,
            "dimensions": task.query.dimensions
        });

        // Add filters if present
        if !task.query.filters.is_empty() {
            let filters = task
                .query
                .filters
                .iter()
                .map(|filter| {
                    // Convert SemanticQueryFilter to SemanticFilter for CubeJS conversion
                    let op = match filter.op.as_str() {
                        "eq" => SemanticOperator::Eq,
                        "neq" => SemanticOperator::Neq,
                        "gt" => SemanticOperator::Gt,
                        "gte" => SemanticOperator::Gte,
                        "lt" => SemanticOperator::Lt,
                        "lte" => SemanticOperator::Lte,
                        "in" => SemanticOperator::In,
                        "not_in" => SemanticOperator::NotIn,
                        _ => SemanticOperator::Eq, // Default fallback
                    };
                    let semantic_filter = SemanticFilter {
                        field: filter.field.clone(),
                        op,
                        value: filter.value.clone(),
                    };
                    self.convert_filter_to_cubejs(&semantic_filter, topic_name)
                })
                .collect::<Result<Vec<_>, _>>()?;
            query["filters"] = JsonValue::Array(filters);
        }

        // Add order if present
        if !task.query.orders.is_empty() {
            let orders = task
                .query
                .orders
                .iter()
                .map(|order| {
                    // Convert SemanticQueryOrder to SemanticOrder for CubeJS conversion
                    let direction = match order.direction.to_lowercase().as_str() {
                        "asc" => SemanticOrderDirection::Asc,
                        "desc" => SemanticOrderDirection::Desc,
                        _ => SemanticOrderDirection::Asc, // Default fallback
                    };
                    let semantic_order = SemanticOrder {
                        field: order.field.clone(),
                        direction,
                    };
                    self.convert_order_to_cubejs(&semantic_order, topic_name)
                })
                .collect::<Vec<_>>();
            query["order"] = JsonValue::Array(orders);
        }

        // Add limit and offset if present
        if let Some(limit) = task.query.limit {
            query["limit"] = JsonValue::Number(serde_json::Number::from(limit));
        }
        if let Some(offset) = task.query.offset {
            query["offset"] = JsonValue::Number(serde_json::Number::from(offset));
        }

        Ok(query)
    }

    /// Convert semantic filter to CubeJS filter format
    fn convert_filter_to_cubejs(
        &self,
        filter: &SemanticFilter,
        topic_name: &str,
    ) -> Result<JsonValue, OxyError> {
        let field_name = if filter.field.contains('.') {
            filter.field.clone()
        } else {
            format!("{}.{}", topic_name, filter.field)
        };

        let operator = match &filter.op {
            SemanticOperator::Eq => "equals",
            SemanticOperator::Neq => "notEquals",
            SemanticOperator::Gt => "gt",
            SemanticOperator::Gte => "gte",
            SemanticOperator::Lt => "lt",
            SemanticOperator::Lte => "lte",
            SemanticOperator::In => "set",
            SemanticOperator::NotIn => "notSet",
        };

        Ok(serde_json::json!({
            "member": field_name,
            "operator": operator,
            "values": if matches!(&filter.op, SemanticOperator::In | SemanticOperator::NotIn) {
                // For IN/NOT IN operators, values should be an array
                match &filter.value {
                    JsonValue::Array(arr) => arr.clone(),
                    val => vec![val.clone()],
                }
            } else {
                // For other operators, wrap single value in array
                vec![filter.value.clone()]
            }
        }))
    }

    /// Convert semantic order to CubeJS order format
    fn convert_order_to_cubejs(&self, order: &SemanticOrder, topic_name: &str) -> JsonValue {
        let field_name = if order.field.contains('.') {
            order.field.clone()
        } else {
            format!("{}.{}", topic_name, order.field)
        };

        let direction = match order.direction {
            crate::config::model::SemanticOrderDirection::Asc => "asc",
            crate::config::model::SemanticOrderDirection::Desc => "desc",
        };

        serde_json::json!([field_name, direction])
    }

    /// Get SQL query from CubeJS /sql endpoint and handle parameters
    async fn get_sql_from_cubejs(&self, query: &JsonValue) -> Result<String, OxyError> {
        // Default CubeJS URL
        let cubejs_url =
            std::env::var("CUBEJS_API_URL").unwrap_or_else(|_| "http://localhost:4000".to_string());

        let client = reqwest::Client::new();

        // Get SQL from CubeJS sql API
        let sql_url = format!("{}/cubejs-api/v1/sql", cubejs_url);
        tracing::info!(
            "Calling CubeJS SQL API at {} with query: {}",
            sql_url,
            query
        );
        let sql_response = client
            .post(&sql_url)
            .json(&serde_json::json!({
                "query": query
            }))
            .send()
            .await
            .map_err(
                |e| super::semantic_validator::SemanticQueryError::CubeJSError {
                    details: format!("Failed to call CubeJS SQL API: {e}"),
                },
            )?;

        let sql_status = sql_response.status();
        if !sql_status.is_success() {
            let error_text = sql_response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(super::semantic_validator::SemanticQueryError::CubeJSError {
                details: format!("SQL API Status {}: {}", sql_status, error_text),
            }
            .into());
        }

        let sql_response_json: JsonValue = sql_response.json().await.map_err(|e| {
            super::semantic_validator::SemanticQueryError::CubeJSError {
                details: format!("Failed to parse CubeJS SQL response JSON: {e}"),
            }
        })?;

        // Extract SQL from response
        // CubeJS SQL response structure: { "sql": { "status": "ok", "sql": ["SELECT ...", [parameters]] } }
        let sql_obj = sql_response_json.get("sql").ok_or_else(|| {
            super::semantic_validator::SemanticQueryError::CubeJSError {
                details: format!(
                    "CubeJS SQL response missing 'sql' object. Response: {}",
                    sql_response_json
                ),
            }
        })?;

        // Check status
        if let Some(status) = sql_obj.get("status").and_then(|s| s.as_str())
            && status != "ok"
        {
            let error_msg = sql_obj
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(super::semantic_validator::SemanticQueryError::CubeJSError {
                details: format!(
                    "CubeJS SQL generation failed with status '{}': {}",
                    status, error_msg
                ),
            }
            .into());
        }

        let sql_array = sql_obj
            .get("sql")
            .and_then(|sql_array| sql_array.as_array())
            .ok_or_else(
                || super::semantic_validator::SemanticQueryError::CubeJSError {
                    details: format!("CubeJS SQL response missing expected 'sql' array structure. Expected: {{\"sql\": [\"SELECT ...\", []]}}, got: {}", sql_obj),
                },
            )?;

        // Extract SQL query (first element)
        let sql_template = sql_array.first().and_then(|s| s.as_str()).ok_or_else(|| {
            super::semantic_validator::SemanticQueryError::CubeJSError {
                details: format!(
                    "CubeJS SQL response missing SQL query string in sql[0]. Got: {:?}",
                    sql_array
                ),
            }
        })?;

        // Extract parameters (second element)
        let empty_params = Vec::new();
        let parameters = sql_array
            .get(1)
            .and_then(|p| p.as_array())
            .unwrap_or(&empty_params); // Default to empty if no parameters

        // Substitute parameters into SQL query
        let final_sql = self.substitute_sql_parameters(sql_template, parameters)?;

        tracing::info!("Generated SQL: {}", final_sql);
        tracing::debug!("Original SQL template: {}", sql_template);
        tracing::debug!("Parameters: {:?}", parameters);

        Ok(final_sql)
    }

    /// Substitute parameters into SQL query
    /// CubeJS typically uses positional parameters like $1, $2, etc.
    fn substitute_sql_parameters(
        &self,
        sql_template: &str,
        parameters: &[JsonValue],
    ) -> Result<String, OxyError> {
        let mut result = sql_template.to_string();

        // Replace positional parameters ($1, $2, etc.)
        for (index, param) in parameters.iter().enumerate() {
            let placeholder = format!("${}", index + 1);
            let param_value = self.json_value_to_sql_literal(param)?;
            result = result.replace(&placeholder, &param_value);
        }

        // Also handle ? placeholders (some drivers use this format)
        let mut param_index = 0;
        while result.contains('?') && param_index < parameters.len() {
            let param_value = self.json_value_to_sql_literal(&parameters[param_index])?;
            result = result.replacen('?', &param_value, 1);
            param_index += 1;
        }

        Ok(result)
    }

    /// Convert a JSON value to a SQL literal string
    fn json_value_to_sql_literal(&self, value: &JsonValue) -> Result<String, OxyError> {
        match value {
            JsonValue::Null => Ok("NULL".to_string()),
            JsonValue::Bool(b) => Ok(b.to_string()),
            JsonValue::Number(n) => Ok(n.to_string()),
            JsonValue::String(s) => {
                // Escape single quotes and wrap in quotes
                let escaped = s.replace('\'', "''");
                Ok(format!("'{}'", escaped))
            }
            JsonValue::Array(arr) => {
                // Convert array to SQL array literal
                let elements = arr
                    .iter()
                    .map(|v| self.json_value_to_sql_literal(v))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("ARRAY[{}]", elements.join(", ")))
            }
            JsonValue::Object(_) => {
                // For objects, convert to JSON string literal
                let json_str = serde_json::to_string(value).map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Failed to serialize JSON object parameter: {e}"
                    ))
                })?;
                let escaped = json_str.replace('\'', "''");
                Ok(format!("'{}'", escaped))
            }
        }
    }

    /// Execute SQL query directly using database connector and save results to file
    async fn execute_sql_and_save_results(
        &self,
        sql: &str,
        database_ref: &str,
        _topic: &str,
        execution_context: &ExecutionContext,
    ) -> Result<String, OxyError> {
        use crate::adapters::connector::write_to_ipc;
        use uuid::Uuid;

        let config_manager = &execution_context.project.config_manager;
        let secret_manager = &execution_context.project.secrets_manager;

        // Create database connector
        let connector =
            Connector::from_database(database_ref, config_manager, secret_manager, None).await?;

        // Execute SQL query
        tracing::info!("Executing SQL query: {}", sql);
        let (record_batches, schema_ref) = connector.run_query_and_load(sql).await?;

        // Generate a unique file path
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());

        // Write results to IPC file
        write_to_ipc(&record_batches, &file_path, &schema_ref)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write results to file: {e}")))?;

        tracing::info!("Saved semantic query results to: {}", file_path);
        Ok(file_path)
    }
}

pub fn build_semantic_query_executable() -> impl Executable<SemanticQueryTask, Response = Output> {
    ExecutableBuilder::new()
        .map(SemanticQueryTaskMapper)
        .executable(SemanticQueryExecutable::new())
}
