use arrow::{array::RecordBatch, datatypes::Schema};
use chrono::{Local, NaiveDate};
use oxy::types::TimeGranularity;
use oxy_semantic::variables::RuntimeVariableResolver;
use serde_json::Value as JsonValue;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use oxy::{
    config::model::{SemanticFilterType, SemanticQueryTask},
    connector::Connector,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        renderer::Renderer,
        types::{Chunk, EventKind, Output, TableReference, utils::record_batches_to_2d_array},
    },
    observability::events::{tool as tool_events, workflow as workflow_events},
    service::types::SemanticQueryParams,
    types::{SemanticQuery, TimeDimension},
    utils::truncate_datasets,
};
use oxy_shared::errors::OxyError;

use crate::semantic_validator_builder::{
    SemanticQueryValidation, ValidatedSemanticQuery, validate_semantic_query_task,
};

#[tracing::instrument(skip_all, err, fields(
    otel.name = workflow_events::task::semantic_query::NAME_RENDER,
    oxy.span_type = workflow_events::task::semantic_query::TYPE,
    oxy.semantic_query.topic = tracing::field::Empty,
    oxy.semantic_query.dimensions_count = task.query.dimensions.len(),
    oxy.semantic_query.measures_count = task.query.measures.len(),
    oxy.semantic_query.filters_count = task.query.filters.len(),
))]
pub fn render_semantic_query(
    renderer: &Renderer,
    task: &SemanticQueryTask,
) -> Result<SemanticQueryTask, OxyError> {
    workflow_events::task::semantic_query::render_input(task);

    let span = tracing::Span::current();
    if let Some(ref topic) = task.query.topic {
        span.record("oxy.semantic_query.topic", topic.as_str());
    }
    let topic = if let Some(t) = &task.query.topic {
        Some(render_string(renderer, t, "topic")?)
    } else {
        None
    };
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

    // Render variables if present
    let variables = task
        .variables
        .as_ref()
        .map(|vars| {
            vars.iter()
                .map(|(k, v)| {
                    if let Some(template) = v.as_str() {
                        let rendered_value = renderer.eval_expression(template)?;
                        let json_value = serde_json::to_value(rendered_value)?;
                        let final_value = match json_value.is_null() {
                            true => v.clone(),
                            false => json_value,
                        };
                        Ok((k.to_string(), final_value))
                    } else {
                        Ok((k.to_string(), v.clone()))
                    }
                })
                .collect::<Result<HashMap<String, JsonValue>, OxyError>>()
        })
        .transpose()?;

    let filters = task.query.filters.clone();

    let orders = task
        .query
        .orders
        .iter()
        .map(|o| {
            let rendered_field = render_string(renderer, &o.field, "order.field")?;
            Ok(oxy::service::types::SemanticQueryOrder {
                field: rendered_field,
                direction: o.direction.clone(),
            })
        })
        .collect::<Result<Vec<_>, OxyError>>()?;

    // Render time dimensions with template variables and relative date conversion
    let time_dimensions = render_time_dimensions(renderer, &task.query.time_dimensions)?;

    let result = SemanticQueryTask {
        query: SemanticQueryParams {
            topic,
            dimensions,
            measures,
            filters,
            orders,
            limit: task.query.limit,
            offset: task.query.offset,
            variables: variables.clone(),
            time_dimensions,
        },
        export: task.export.clone(),
        variables,
    };

    workflow_events::task::semantic_query::render_output(&result);

    Ok(result)
}

fn render_string(renderer: &Renderer, value: &str, ctx: &str) -> Result<String, OxyError> {
    renderer.render_str(value).map_err(|e| {
        OxyError::RuntimeError(format!(
            "Failed to render semantic query {ctx} template '{value}': {e}"
        ))
    })
}

/// Render time dimensions with template variables and convert relative dates to ISO 8601
fn render_time_dimensions(
    renderer: &Renderer,
    time_dimensions: &[TimeDimension],
) -> Result<Vec<TimeDimension>, OxyError> {
    time_dimensions
        .iter()
        .map(|td| {
            // Render dimension field with templates
            let rendered_dimension =
                render_string(renderer, &td.dimension, "time_dimension.dimension")?;

            Ok(TimeDimension {
                dimension: rendered_dimension,
                granularity: td.granularity.clone(),
            })
        })
        .collect::<Result<Vec<_>, OxyError>>()
}

/// Normalize a single date value - convert relative expressions to ISO 8601 format
fn normalize_date_value(date: &str) -> Result<String, OxyError> {
    // Try parsing as ISO date first (YYYY-MM-DD format)
    if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok() {
        return Ok(date.to_string());
    }

    // Try parsing as relative expression
    let result = chrono_english::parse_date_string(date, Local::now(), chrono_english::Dialect::Us)
        .map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to parse date value '{}': {}. Expected ISO 8601 format (YYYY-MM-DD) or relative expression (e.g., '7 days ago', 'now', 'next monday')",
                date, e
            ))
        })?;

    // Convert to ISO 8601 date format
    Ok(result.format("%Y-%m-%d").to_string())
}

/// Collect fully-qualified field names of date/datetime dimensions from views.
/// Returns a set like `{"ViewName.field_name", ...}` used to decide whether
/// filter values need relative-date normalisation.
fn collect_date_fields(views: &[oxy_semantic::View]) -> HashSet<String> {
    let mut date_fields = HashSet::new();
    for view in views {
        for dim in &view.dimensions {
            if matches!(
                dim.dimension_type,
                oxy_semantic::DimensionType::Date | oxy_semantic::DimensionType::Datetime
            ) {
                date_fields.insert(format!("{}.{}", view.name, dim.name));
            }
        }
    }
    date_fields
}

/// ParamMapper for semantic query tasks that handles templating and validation
#[derive(Clone)]
struct SemanticQueryTaskMapper;

#[async_trait::async_trait]
impl ParamMapper<SemanticQueryTask, ValidatedSemanticQuery> for SemanticQueryTaskMapper {
    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::task::semantic_query::NAME_MAP,
        oxy.span_type = workflow_events::task::semantic_query::TYPE,
    ))]
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: SemanticQueryTask,
    ) -> Result<(ValidatedSemanticQuery, Option<ExecutionContext>), OxyError> {
        workflow_events::task::semantic_query::map_input(&input);

        // Task 3.1: Pre-Execution Templating
        let rendered_task = render_semantic_query(&execution_context.renderer, &input)?;

        // Task 3.2: Metadata Validation
        let validated_query =
            validate_semantic_query_task(&execution_context.project.config_manager, &rendered_task)
                .await?;

        workflow_events::task::semantic_query::map_output(
            &validated_query.topic.name,
            validated_query.task.query.dimensions.len(),
            validated_query.task.query.measures.len(),
        );

        Ok((validated_query, None))
    }
}

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

    #[tracing::instrument(skip_all, err, fields(
        otel.name = tool_events::SEMANTIC_QUERY_COMPILE,
        oxy.span_type = tool_events::SEMANTIC_QUERY_COMPILE_TYPE,
        oxy.semantic_query.topic = %input.topic.name,
    ))]
    pub async fn compile(
        &mut self,
        execution_context: &ExecutionContext,
        input: ValidatedSemanticQuery,
    ) -> Result<String, OxyError> {
        // Record explicit metrics and emit tracing event
        tool_events::semantic_query_compile_input(&input.topic.name, &input.task.query);

        let config_manager = &execution_context.project.config_manager;
        let date_fields = collect_date_fields(&input.views);

        let mut sql_query = compile_with_airlayer(
            &input.task,
            &input.topic.name,
            input.topic.base_view.as_ref(),
            input.topic.default_filters.as_ref(),
            &input.views,
            config_manager,
            &date_fields,
        )?;

        let variables = input.task.variables.clone().unwrap_or_default();

        sql_query = self.resolve_variables_in_sql(execution_context, sql_query, variables)?;

        tool_events::semantic_query_compile_output(&sql_query);

        Ok(sql_query)
    }
}

#[async_trait::async_trait]
impl Executable<SemanticQueryValidation> for SemanticQueryExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        validation_result: SemanticQueryValidation,
    ) -> Result<Self::Response, OxyError> {
        // Handle validation errors first
        match validation_result {
            SemanticQueryValidation::Invalid { task, error } => {
                let topic_name = task.query.topic.as_deref().unwrap_or("unknown").to_string();

                execution_context
                    .write_kind(EventKind::Started {
                        name: format!("Semantic Query: {}", topic_name),
                        attributes: HashMap::from_iter([("topic".to_string(), topic_name.clone())]),
                    })
                    .await?;

                let artifact_value = SemanticQuery {
                    database: String::new(),
                    sql_query: String::new(),
                    result: vec![],
                    error: None,
                    validation_error: Some(error.clone()),
                    sql_generation_error: None,
                    is_result_truncated: false,
                    topic: task.query.topic.clone(),
                    dimensions: task.query.dimensions.clone(),
                    measures: task.query.measures.clone(),
                    time_dimensions: task.query.time_dimensions.clone(),
                    filters: task.query.filters.clone(),
                    orders: task.query.orders.clone(),
                    limit: task.query.limit,
                    offset: task.query.offset,
                };

                execution_context
                    .write_chunk(Chunk {
                        key: None,
                        delta: Output::SemanticQuery(artifact_value.clone()),
                        finished: true,
                    })
                    .await?;

                execution_context
                    .write_kind(EventKind::Finished {
                        message: format!("Semantic query validation failed: {}", error),
                        attributes: [].into(),
                        error: Some(error.clone()),
                    })
                    .await?;

                return Err(OxyError::ValidationError(error));
            }
            SemanticQueryValidation::Valid(input) => {
                self.execute_validated(execution_context, input).await
            }
        }
    }
}

#[async_trait::async_trait]
impl Executable<ValidatedSemanticQuery> for SemanticQueryExecutable {
    type Response = Output;

    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::task::semantic_query::NAME_EXECUTE,
        oxy.span_type = workflow_events::task::semantic_query::TYPE,
        oxy.semantic_query.topic = %input.topic.name,
        oxy.semantic_query.dimensions_count = input.task.query.dimensions.len(),
        oxy.semantic_query.measures_count = input.task.query.measures.len(),
        oxy.semantic_query.filters_count = input.task.query.filters.len(),
    ))]
    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: ValidatedSemanticQuery,
    ) -> Result<Self::Response, OxyError> {
        self.execute_validated(execution_context, input).await
    }
}

impl SemanticQueryExecutable {
    async fn execute_validated(
        &mut self,
        execution_context: &ExecutionContext,
        input: ValidatedSemanticQuery,
    ) -> Result<Output, OxyError> {
        workflow_events::task::semantic_query::execute_input(
            &input.topic.name,
            &input.task.query.dimensions,
            &input.task.query.measures,
        );

        // Create a child context for the semantic query task execution
        // This prevents the task's Finished event from closing the artifact block
        let task_context = execution_context.with_child_source(
            format!("semantic-query-{}", input.topic.name),
            "semantic_query_task".to_string(),
        );

        task_context
            .write_kind(EventKind::Started {
                name: format!("Semantic Query: {}", input.topic.name),
                attributes: HashMap::from_iter([("topic".to_string(), input.topic.name.clone())]),
            })
            .await?;

        tracing::info!(
            "Executing semantic query for topic '{}': {:?}",
            input.topic.name,
            input.task.query
        );

        let mut artifact_value = SemanticQuery {
            database: String::new(),
            sql_query: String::new(),
            result: vec![],
            error: None,
            validation_error: None,
            sql_generation_error: None,
            is_result_truncated: false,
            topic: Some(input.topic.name.clone()),
            dimensions: input.task.query.dimensions.clone(),
            measures: input.task.query.measures.clone(),
            time_dimensions: input.task.query.time_dimensions.clone(),
            filters: input.task.query.filters.clone(),
            orders: input.task.query.orders.clone(),
            limit: input.task.query.limit,
            offset: input.task.query.offset,
        };

        task_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::SemanticQuery(artifact_value.clone()),
                finished: true,
            })
            .await?;

        let config_manager = &execution_context.project.config_manager;
        let date_fields = collect_date_fields(&input.views);

        // Compile semantic query to SQL using airlayer engine
        let mut sql_query = match compile_with_airlayer(
            &input.task,
            &input.topic.name,
            input.topic.base_view.as_ref(),
            input.topic.default_filters.as_ref(),
            &input.views,
            config_manager,
            &date_fields,
        ) {
            Ok(sql) => sql,
            Err(e) => {
                tracing::error!(
                    "Failed to compile semantic query for topic '{}': {e}",
                    input.topic.name
                );
                artifact_value.sql_generation_error = Some("Failed to generate SQL".to_string());
                task_context
                    .write_chunk(Chunk {
                        key: None,
                        delta: Output::SemanticQuery(artifact_value.clone()),
                        finished: true,
                    })
                    .await?;
                return Err(e);
            }
        };
        tracing::info!(
            "Generated SQL for topic '{}': {}",
            input.topic.name,
            sql_query
        );

        let variables = input.task.variables.clone().unwrap_or_default();

        tracing::info!("Resolving variables in SQL query: {:?}", variables);
        sql_query = self.resolve_variables_in_sql(execution_context, sql_query, variables)?;
        tracing::info!("SQL query after variable resolution: {}", sql_query);

        // Determine database from topic's views
        let database = self.determine_database_from_topic(&input)?;
        artifact_value.database = database.clone();
        artifact_value.sql_query = sql_query.clone();

        // Emit semantic query params as a chunk
        task_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::SemanticQuery(artifact_value.clone()),
                finished: true,
            })
            .await?;

        // Step 2: Execute SQL directly using database connector and save results
        let (file_path, record_batches, schema_ref) = match self
            .execute_sql_and_save_results(
                &sql_query,
                &database,
                &input.topic.name,
                execution_context,
            )
            .await
        {
            Ok(result) => result,
            Err(e) => {
                // Emit artifact value with error
                artifact_value.error = Some(format!(
                    "Failed to execute semantic query for topic '{}': {e}",
                    input.topic.name
                ));
                task_context
                    .write_chunk(Chunk {
                        key: None,
                        delta: Output::SemanticQuery(artifact_value.clone()),
                        finished: true,
                    })
                    .await?;
                return Err(OxyError::RuntimeError(format!(
                    "Failed to execute semantic query for topic '{}': {e}",
                    input.topic.name
                )));
            }
        };

        // Truncate results for artifact display (not for file output)
        let (truncated_batches, is_truncated) = truncate_datasets(&record_batches, None);

        // Convert record batches to 2D array for artifact result
        let result_2d =
            record_batches_to_2d_array(&truncated_batches, &schema_ref).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to convert results to 2D array: {e}"))
            })?;

        // Populate artifact_value with results
        artifact_value.result = result_2d;
        artifact_value.is_result_truncated = is_truncated;

        // Build table output (leveraging existing table/reference system) and emit as chunk
        let table_output = Output::table_with_reference(
            file_path.clone(),
            TableReference {
                sql: sql_query,
                database_ref: database.clone(),
            },
            None,
        );

        task_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::SemanticQuery(artifact_value.clone()),
                finished: true,
            })
            .await?;

        task_context
            .write_kind(EventKind::Finished {
                message: format!(
                    "Executed semantic query for topic '{}' - results written to {}",
                    input.topic.name, file_path
                ),
                attributes: [].into(),
                error: None,
            })
            .await?;

        workflow_events::task::semantic_query::execute_output(&table_output);

        // Return Table output with semantic query reference
        Ok(table_output)
    }

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
            input.topic.name
        )))
    }

    /// Resolve variables in SQL query using RuntimeVariableResolver
    fn resolve_variables_in_sql(
        &self,
        _execution_context: &ExecutionContext,
        sql_query: String,
        variables: HashMap<String, JsonValue>,
    ) -> Result<String, OxyError> {
        let global_vars: HashMap<String, JsonValue> = HashMap::new();

        let env_vars: HashMap<String, JsonValue> = std::env::vars()
            .filter(|(key, _)| key.starts_with("OXY_VAR_"))
            .map(|(key, value)| {
                let var_name = key.strip_prefix("OXY_VAR_").unwrap_or(&key).to_lowercase();
                (var_name, JsonValue::String(value))
            })
            .collect();

        let resolver = RuntimeVariableResolver::from_sources(
            Some(variables),
            None,
            Some(global_vars),
            Some(env_vars),
        )
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create variable resolver: {}", e))
        })?;

        let resolved_sql = resolver.resolve_sql_variables(sql_query).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to resolve variables in SQL: {}", e))
        })?;

        Ok(resolved_sql)
    }

    /// Execute SQL query directly using database connector and save results to file
    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::task::semantic_query::NAME_EXECUTE_SQL,
        oxy.span_type = workflow_events::task::semantic_query::TYPE,
        oxy.database.ref = database_ref,
    ))]
    async fn execute_sql_and_save_results(
        &self,
        sql: &str,
        database_ref: &str,
        _topic: &str,
        execution_context: &ExecutionContext,
    ) -> Result<(String, Vec<RecordBatch>, Arc<Schema>), OxyError> {
        workflow_events::task::semantic_query::execute_sql_input(database_ref, sql);
        use oxy::connector::write_to_ipc;
        use uuid::Uuid;

        let config_manager = &execution_context.project.config_manager;
        let secret_manager = &execution_context.project.secrets_manager;

        // Create database connector
        let connector = Connector::from_database(
            database_ref,
            config_manager,
            secret_manager,
            None,
            execution_context.filters.clone(),
            execution_context.connections.clone(),
        )
        .await?;

        // Execute SQL query
        tracing::info!("Executing SQL query: {}", sql);
        let (record_batches, schema_ref) = connector.run_query_and_load(sql).await?;

        // Generate a unique file path
        let file_path = format!("/tmp/{}.arrow", Uuid::new_v4());

        // Write results to IPC file
        write_to_ipc(&record_batches, &file_path, &schema_ref)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to write results to file: {e}")))?;

        tracing::info!("Saved semantic query results to: {}", file_path);

        workflow_events::task::semantic_query::execute_sql_output(&file_path);

        Ok((file_path, record_batches, schema_ref))
    }
}

/// Compile a semantic query to SQL using the airlayer in-process engine.
fn compile_with_airlayer(
    task: &SemanticQueryTask,
    topic_name: &str,
    base_view: Option<&String>,
    default_filters: Option<&Vec<oxy_semantic::TopicFilter>>,
    views: &[oxy_semantic::View],
    config_manager: &oxy::config::ConfigManager,
    date_fields: &HashSet<String>,
) -> Result<String, OxyError> {
    // 1. Convert oxy-semantic views to airlayer views
    let airlayer_views: Vec<airlayer::View> = views.iter().map(convert_view_to_airlayer).collect();

    // 2. Build datasource→dialect map from oxy-internal's config databases
    let dialects = build_dialect_map(config_manager);

    // 3. Build airlayer SemanticLayer and engine
    let layer = airlayer::SemanticLayer::new(airlayer_views, None);
    let engine = airlayer::SemanticEngine::from_semantic_layer(layer, dialects)
        .map_err(|e| OxyError::RuntimeError(format!("airlayer engine error: {e}")))?;

    // 4. Convert query params to airlayer QueryRequest
    let request = build_airlayer_query(task, topic_name, base_view, default_filters, date_fields)?;

    // 5. Compile
    let result = engine
        .compile_query(&request)
        .map_err(|e| OxyError::RuntimeError(format!("airlayer compilation error: {e}")))?;

    // 6. Substitute parameter placeholders with actual values.
    // Airlayer returns parameterized SQL (e.g. $1, ?, @p0) with a separate params vec,
    // but oxy-internal's Engine trait only accepts raw SQL with no param binding.
    let sql = substitute_params(&result.sql, &result.params);

    Ok(sql)
}

/// Substitute positional parameter placeholders ($1, $2, ...) and ? placeholders
/// with escaped string literals. This is needed because the Engine trait sends raw SQL
/// to connectors with no separate param binding support.
fn substitute_params(sql: &str, params: &[String]) -> String {
    if params.is_empty() {
        return sql.to_string();
    }

    // Check if the original SQL uses positional ($N or @pN) placeholders.
    // $N/@pN and ? are mutually exclusive dialects — we must check the original SQL
    // to avoid corrupting already-substituted values that contain '?' characters
    // (e.g., URLs like 'https://example.com?q=test').
    let uses_positional = (0..params.len())
        .any(|i| sql.contains(&format!("${}", i + 1)) || sql.contains(&format!("@p{}", i)));

    let mut result = sql.to_string();

    if uses_positional {
        // Replace $1, $2, ... and @p0, @p1, ... (Postgres/DuckDB/ClickHouse and BigQuery styles)
        for (i, param) in params.iter().enumerate().rev() {
            let escaped = param.replace('\'', "''");
            let literal = format!("'{}'", escaped);
            result = result.replace(&format!("${}", i + 1), &literal);
            result = result.replace(&format!("@p{}", i), &literal);
        }
    } else {
        // Replace ? placeholders (MySQL/Snowflake/Databricks/SQLite style)
        let mut param_index = 0;
        while result.contains('?') && param_index < params.len() {
            let escaped = params[param_index].replace('\'', "''");
            let literal = format!("'{}'", escaped);
            result = result.replacen('?', &literal, 1);
            param_index += 1;
        }
    }

    result
}

/// Convert an oxy-semantic View to an airlayer View.
fn convert_view_to_airlayer(view: &oxy_semantic::View) -> airlayer::View {
    airlayer::View {
        name: view.name.clone(),
        description: view.description.clone(),
        label: view.label.clone(),
        datasource: view.datasource.clone(),
        dialect: None, // Dialect comes from datasource mapping
        table: view.table.clone(),
        sql: view.sql.clone(),
        entities: view
            .entities
            .iter()
            .map(convert_entity_to_airlayer)
            .collect(),
        dimensions: view
            .dimensions
            .iter()
            .map(convert_dimension_to_airlayer)
            .collect(),
        measures: view
            .measures
            .as_ref()
            .map(|ms| ms.iter().map(convert_measure_to_airlayer).collect()),
        // TODO: pass through segments when oxy-semantic adds support
        segments: vec![],
        meta: None,
    }
}

fn convert_entity_to_airlayer(entity: &oxy_semantic::Entity) -> airlayer::Entity {
    airlayer::Entity {
        name: entity.name.clone(),
        entity_type: match entity.entity_type {
            oxy_semantic::EntityType::Primary => airlayer::schema::models::EntityType::Primary,
            oxy_semantic::EntityType::Foreign => airlayer::schema::models::EntityType::Foreign,
        },
        description: Some(entity.description.clone()),
        key: entity.key.clone(),
        keys: entity.keys.clone(),
        inherits_from: None,
        meta: None,
    }
}

fn convert_dimension_to_airlayer(dim: &oxy_semantic::Dimension) -> airlayer::Dimension {
    airlayer::Dimension {
        name: dim.name.clone(),
        dimension_type: match dim.dimension_type {
            oxy_semantic::DimensionType::String => airlayer::schema::models::DimensionType::String,
            oxy_semantic::DimensionType::Number => airlayer::schema::models::DimensionType::Number,
            oxy_semantic::DimensionType::Date => airlayer::schema::models::DimensionType::Date,
            oxy_semantic::DimensionType::Datetime => {
                airlayer::schema::models::DimensionType::Datetime
            }
            oxy_semantic::DimensionType::Boolean => {
                airlayer::schema::models::DimensionType::Boolean
            }
        },
        description: dim.description.clone(),
        expr: dim.expr.clone(),
        original_expr: dim.original_expr.clone(),
        samples: dim.samples.clone(),
        synonyms: dim.synonyms.clone(),
        primary_key: None,
        sub_query: None,
        inherits_from: None,
        meta: None,
    }
}

fn convert_measure_to_airlayer(measure: &oxy_semantic::Measure) -> airlayer::Measure {
    airlayer::Measure {
        name: measure.name.clone(),
        measure_type: match measure.measure_type {
            oxy_semantic::MeasureType::Count => airlayer::schema::models::MeasureType::Count,
            oxy_semantic::MeasureType::Sum => airlayer::schema::models::MeasureType::Sum,
            oxy_semantic::MeasureType::Average => airlayer::schema::models::MeasureType::Average,
            oxy_semantic::MeasureType::Min => airlayer::schema::models::MeasureType::Min,
            oxy_semantic::MeasureType::Max => airlayer::schema::models::MeasureType::Max,
            oxy_semantic::MeasureType::CountDistinct => {
                airlayer::schema::models::MeasureType::CountDistinct
            }
            oxy_semantic::MeasureType::Median => airlayer::schema::models::MeasureType::Median,
            oxy_semantic::MeasureType::Custom => airlayer::schema::models::MeasureType::Custom,
        },
        description: measure.description.clone(),
        expr: measure.expr.clone(),
        original_expr: measure.original_expr.clone(),
        filters: measure.filters.as_ref().map(|fs| {
            fs.iter()
                .map(|f| airlayer::schema::models::MeasureFilter {
                    expr: f.expr.clone(),
                    original_expr: f.original_expr.clone(),
                    description: f.description.clone(),
                })
                .collect()
        }),
        samples: measure.samples.clone(),
        synonyms: measure.synonyms.clone(),
        rolling_window: None,
        inherits_from: None,
        meta: None,
    }
}

/// Build a DatasourceDialectMap from the oxy-internal config databases.
fn build_dialect_map(
    config_manager: &oxy::config::ConfigManager,
) -> airlayer::DatasourceDialectMap {
    let mut map = airlayer::DatasourceDialectMap::new();
    let databases = config_manager.list_databases();

    for db in databases {
        let dialect_str = db.database_type.to_string();
        if let Some(dialect) = airlayer::Dialect::from_str(&dialect_str) {
            map.insert(&db.name, dialect);
        }
    }

    // Use first database as default
    if let Some(first_db) = databases.first() {
        let dialect_str = first_db.database_type.to_string();
        if let Some(dialect) = airlayer::Dialect::from_str(&dialect_str) {
            map.set_default(dialect);
        }
    }

    map
}

/// Build an airlayer QueryRequest from oxy-internal's SemanticQueryTask.
fn build_airlayer_query(
    task: &SemanticQueryTask,
    topic_name: &str,
    base_view: Option<&String>,
    default_filters: Option<&Vec<oxy_semantic::TopicFilter>>,
    date_fields: &HashSet<String>,
) -> Result<airlayer::engine::query::QueryRequest, OxyError> {
    use airlayer::engine::query::{OrderBy, QueryFilter, QueryRequest, TimeDimensionQuery};

    let mut filters = Vec::new();

    // Add default filters from topic
    if let Some(default_filters) = default_filters {
        for df in default_filters {
            let field = qualify_field(&df.field, topic_name);
            let (operator, values) =
                convert_topic_filter_type(&df.filter_type, &field, date_fields)?;
            filters.push(QueryFilter {
                member: Some(field),
                operator: Some(operator),
                values,
                and: None,
                or: None,
            });
        }
    }

    // Add user-provided filters
    for f in &task.query.filters {
        let field = qualify_field(&f.field, topic_name);
        let (operator, values) = convert_semantic_filter_type(&f.filter_type, &field, date_fields)?;
        filters.push(QueryFilter {
            member: Some(field),
            operator: Some(operator),
            values,
            and: None,
            or: None,
        });
    }

    // Convert orders
    let order: Vec<OrderBy> = task
        .query
        .orders
        .iter()
        .map(|o| OrderBy {
            id: qualify_field(&o.field, topic_name),
            desc: o.direction.to_lowercase() == "desc",
        })
        .collect();

    // Convert time dimensions
    let time_dimensions: Vec<TimeDimensionQuery> = task
        .query
        .time_dimensions
        .iter()
        .map(|td| {
            let dimension = qualify_field(&td.dimension, topic_name);
            let granularity = td.granularity.as_ref().map(granularity_to_string);
            TimeDimensionQuery {
                dimension,
                granularity,
                date_range: None,
            }
        })
        .collect();

    // Build through hints from base_view
    let through = if let Some(bv) = base_view {
        vec![bv.clone()]
    } else {
        vec![]
    };

    Ok(QueryRequest {
        measures: task.query.measures.clone(),
        dimensions: task.query.dimensions.clone(),
        filters,
        // TODO: pass through segments when oxy-semantic adds support
        segments: vec![],
        time_dimensions,
        order,
        limit: task.query.limit,
        offset: task.query.offset,
        timezone: None,
        // TODO: expose ungrouped option when oxy-semantic adds support
        ungrouped: false,
        through,
        motif: None,
        motif_params: Default::default(),
    })
}

/// Qualify a field name with the topic name if not already qualified.
fn qualify_field(field: &str, topic_name: &str) -> String {
    if field.contains('.') {
        field.to_string()
    } else {
        format!("{}.{}", topic_name, field)
    }
}

/// Convert TimeGranularity to airlayer granularity string.
fn granularity_to_string(g: &TimeGranularity) -> String {
    match g {
        TimeGranularity::Year => "year",
        TimeGranularity::Quarter => "quarter",
        TimeGranularity::Month => "month",
        TimeGranularity::Week => "week",
        TimeGranularity::Day => "day",
        TimeGranularity::Hour => "hour",
        TimeGranularity::Minute => "minute",
        TimeGranularity::Second => "second",
    }
    .to_string()
}

/// Convert oxy_semantic::TopicFilterType to airlayer FilterOperator + values.
fn convert_topic_filter_type(
    filter_type: &oxy_semantic::TopicFilterType,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<(airlayer::engine::query::FilterOperator, Vec<String>), OxyError> {
    use airlayer::engine::query::FilterOperator;

    match filter_type {
        oxy_semantic::TopicFilterType::Eq(f) => Ok((
            FilterOperator::Equals,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        oxy_semantic::TopicFilterType::Neq(f) => Ok((
            FilterOperator::NotEquals,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        oxy_semantic::TopicFilterType::Gt(f) => Ok((
            FilterOperator::Gt,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        oxy_semantic::TopicFilterType::Gte(f) => Ok((
            FilterOperator::Gte,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        oxy_semantic::TopicFilterType::Lt(f) => Ok((
            FilterOperator::Lt,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        oxy_semantic::TopicFilterType::Lte(f) => Ok((
            FilterOperator::Lte,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        // airlayer's Equals/NotEquals with a Vec of values generates IN (...) / NOT IN (...) SQL
        oxy_semantic::TopicFilterType::In(f) => {
            let values = f
                .values
                .iter()
                .map(|v| json_value_to_string(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::Equals, values))
        }
        oxy_semantic::TopicFilterType::NotIn(f) => {
            let values = f
                .values
                .iter()
                .map(|v| json_value_to_string(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::NotEquals, values))
        }
        oxy_semantic::TopicFilterType::InDateRange(f) => {
            let from = json_value_to_string(&f.from, field, date_fields)?;
            let to = json_value_to_string(&f.to, field, date_fields)?;
            Ok((FilterOperator::InDateRange, vec![from, to]))
        }
        oxy_semantic::TopicFilterType::NotInDateRange(f) => {
            let from = json_value_to_string(&f.from, field, date_fields)?;
            let to = json_value_to_string(&f.to, field, date_fields)?;
            Ok((FilterOperator::NotInDateRange, vec![from, to]))
        }
    }
}

/// Convert SemanticFilterType to airlayer FilterOperator + values.
fn convert_semantic_filter_type(
    filter_type: &SemanticFilterType,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<(airlayer::engine::query::FilterOperator, Vec<String>), OxyError> {
    use airlayer::engine::query::FilterOperator;

    match filter_type {
        SemanticFilterType::Eq(f) => Ok((
            FilterOperator::Equals,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Neq(f) => Ok((
            FilterOperator::NotEquals,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Gt(f) => Ok((
            FilterOperator::Gt,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Gte(f) => Ok((
            FilterOperator::Gte,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Lt(f) => Ok((
            FilterOperator::Lt,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Lte(f) => Ok((
            FilterOperator::Lte,
            vec![json_value_to_string(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::In(f) => {
            let values = f
                .values
                .iter()
                .map(|v| json_value_to_string(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::Equals, values))
        }
        SemanticFilterType::NotIn(f) => {
            let values = f
                .values
                .iter()
                .map(|v| json_value_to_string(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::NotEquals, values))
        }
        SemanticFilterType::InDateRange(f) => {
            let from = json_value_to_string(&f.from, field, date_fields)?;
            let to = json_value_to_string(&f.to, field, date_fields)?;
            Ok((FilterOperator::InDateRange, vec![from, to]))
        }
        SemanticFilterType::NotInDateRange(f) => {
            let from = json_value_to_string(&f.from, field, date_fields)?;
            let to = json_value_to_string(&f.to, field, date_fields)?;
            Ok((FilterOperator::NotInDateRange, vec![from, to]))
        }
    }
}

/// Convert a JSON value to a string suitable for airlayer filter values.
/// Resolves relative date expressions for date fields.
fn json_value_to_string(
    value: &JsonValue,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<String, OxyError> {
    let s = match value {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => {
            return Err(OxyError::RuntimeError(format!(
                "NULL filter value for field '{field}' is not supported"
            )));
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    };

    // Resolve relative dates for date fields
    if date_fields.contains(field) {
        return normalize_date_value(&s);
    }

    Ok(s)
}

pub fn build_semantic_query_executable() -> impl Executable<SemanticQueryTask, Response = Output> {
    ExecutableBuilder::new()
        .map(SemanticQueryTaskMapper)
        .executable(SemanticQueryExecutable::new())
}

/// Compile a [`ValidatedSemanticQuery`] to SQL without an `ExecutionContext`.
///
/// Used by the builder copilot to test semantic query definitions before
/// proposing changes to `.view.yml` / `.topic.yml` files.
pub fn compile_validated_to_sql(
    validated: &crate::semantic_validator_builder::ValidatedSemanticQuery,
    config_manager: &oxy::config::ConfigManager,
) -> Result<String, oxy_shared::errors::OxyError> {
    let date_fields = collect_date_fields(&validated.views);
    compile_with_airlayer(
        &validated.task,
        &validated.topic.name,
        validated.topic.base_view.as_ref(),
        validated.topic.default_filters.as_ref(),
        &validated.views,
        config_manager,
        &date_fields,
    )
}

/// Get the database reference from a [`ValidatedSemanticQuery`] (inspects view datasources).
pub fn get_database_from_validated(
    validated: &crate::semantic_validator_builder::ValidatedSemanticQuery,
) -> Result<String, oxy_shared::errors::OxyError> {
    for view in &validated.views {
        if let Some(datasource) = &view.datasource {
            return Ok(datasource.clone());
        }
    }
    Err(oxy_shared::errors::OxyError::ValidationError(format!(
        "No datasource found for topic '{}'. At least one view must specify a datasource.",
        validated.topic.name
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Basic functionality ---

    #[test]
    fn test_substitute_params_empty_params() {
        let sql = "SELECT * FROM users WHERE id = $1";
        assert_eq!(substitute_params(sql, &[]), sql);
    }

    #[test]
    fn test_substitute_params_single_postgres() {
        let result = substitute_params("SELECT * FROM users WHERE id = $1", &["42".to_string()]);
        assert_eq!(result, "SELECT * FROM users WHERE id = '42'");
    }

    #[test]
    fn test_substitute_params_multiple_postgres() {
        let result = substitute_params(
            "SELECT * FROM users WHERE id = $1 AND name = $2",
            &["42".to_string(), "alice".to_string()],
        );
        assert_eq!(
            result,
            "SELECT * FROM users WHERE id = '42' AND name = 'alice'"
        );
    }

    #[test]
    fn test_substitute_params_single_bigquery() {
        let result = substitute_params("SELECT * FROM users WHERE id = @p0", &["42".to_string()]);
        assert_eq!(result, "SELECT * FROM users WHERE id = '42'");
    }

    #[test]
    fn test_substitute_params_multiple_bigquery() {
        let result = substitute_params(
            "SELECT * FROM users WHERE id = @p0 AND name = @p1",
            &["42".to_string(), "alice".to_string()],
        );
        assert_eq!(
            result,
            "SELECT * FROM users WHERE id = '42' AND name = 'alice'"
        );
    }

    #[test]
    fn test_substitute_params_question_marks_snowflake() {
        let result = substitute_params(
            "SELECT * FROM users WHERE id = ? AND name = ?",
            &["42".to_string(), "alice".to_string()],
        );
        assert_eq!(
            result,
            "SELECT * FROM users WHERE id = '42' AND name = 'alice'"
        );
    }

    // --- The prefix collision bug (the fix we're testing) ---

    #[test]
    fn test_substitute_params_postgres_prefix_collision_11_params() {
        let params: Vec<String> = (1..=11).map(|i| format!("val{}", i)).collect();
        let sql = "SELECT * FROM t WHERE a = $1 AND b = $2 AND c = $3 AND d = $4 \
                   AND e = $5 AND f = $6 AND g = $7 AND h = $8 AND i = $9 AND j = $10 \
                   AND k = $11";
        let result = substitute_params(sql, &params);
        assert_eq!(
            result,
            "SELECT * FROM t WHERE a = 'val1' AND b = 'val2' AND c = 'val3' AND d = 'val4' \
             AND e = 'val5' AND f = 'val6' AND g = 'val7' AND h = 'val8' AND i = 'val9' AND j = 'val10' \
             AND k = 'val11'"
        );
        // Key assertions: $11 must NOT become 'val1'1
        assert!(!result.contains("'val1'1"));
        assert!(result.contains("'val11'"));
    }

    #[test]
    fn test_substitute_params_bigquery_prefix_collision_11_params() {
        let params: Vec<String> = (0..11).map(|i| format!("val{}", i)).collect();
        let sql = "SELECT * FROM t WHERE a = @p0 AND b = @p1 AND c = @p10";
        let result = substitute_params(sql, &params);
        assert_eq!(
            result,
            "SELECT * FROM t WHERE a = 'val0' AND b = 'val1' AND c = 'val10'"
        );
        assert!(!result.contains("'val1'0"));
        assert!(result.contains("'val10'"));
    }

    #[test]
    fn test_substitute_params_postgres_20_params() {
        let params: Vec<String> = (1..=20).map(|i| format!("p{}", i)).collect();
        let placeholders: Vec<String> = (1..=20).map(|i| format!("${}", i)).collect();
        let sql = format!("SELECT {}", placeholders.join(", "));
        let result = substitute_params(&sql, &params);
        let expected_vals: Vec<String> = (1..=20).map(|i| format!("'p{}'", i)).collect();
        let expected = format!("SELECT {}", expected_vals.join(", "));
        assert_eq!(result, expected);
    }

    // --- SQL injection / escaping ---

    #[test]
    fn test_substitute_params_escapes_single_quotes() {
        let result = substitute_params(
            "SELECT * FROM users WHERE name = $1",
            &["O'Brien".to_string()],
        );
        assert_eq!(result, "SELECT * FROM users WHERE name = 'O''Brien'");
    }

    #[test]
    fn test_substitute_params_escapes_multiple_quotes() {
        let result = substitute_params(
            "SELECT * FROM t WHERE a = $1",
            &["it's a 'test'".to_string()],
        );
        assert_eq!(result, "SELECT * FROM t WHERE a = 'it''s a ''test'''");
    }

    // --- Dialect detection: $N/@pN and ? are mutually exclusive ---

    #[test]
    fn test_substitute_params_mixed_dollar_and_question() {
        // When $N placeholders are present, the SQL is treated as positional dialect.
        // The ? is NOT treated as a placeholder — it's left as-is.
        let result = substitute_params("SELECT $1, ?", &["a".to_string(), "b".to_string()]);
        // $1 → 'a', ? left untouched because positional dialect detected
        assert_eq!(result, "SELECT 'a', ?");
    }

    #[test]
    fn test_substitute_params_positional_value_with_question_mark() {
        // Critical: substituted values containing '?' must not be treated as placeholders.
        // e.g., a URL with query params: https://example.com?q=test
        let result = substitute_params(
            "SELECT * FROM t WHERE url = $1 AND status = $2",
            &[
                "https://example.com?q=test".to_string(),
                "active".to_string(),
            ],
        );
        assert_eq!(
            result,
            "SELECT * FROM t WHERE url = 'https://example.com?q=test' AND status = 'active'"
        );
    }

    // --- Edge cases ---

    #[test]
    fn test_substitute_params_repeated_placeholder() {
        let result =
            substitute_params("SELECT * FROM t WHERE a = $1 OR b = $1", &["x".to_string()]);
        assert_eq!(result, "SELECT * FROM t WHERE a = 'x' OR b = 'x'");
    }

    #[test]
    fn test_substitute_params_no_placeholders_in_sql() {
        let result = substitute_params("SELECT 1", &["unused".to_string()]);
        assert_eq!(result, "SELECT 1");
    }

    #[test]
    fn test_substitute_params_question_mark_fewer_params() {
        let result = substitute_params("SELECT ? , ? , ?", &["a".to_string(), "b".to_string()]);
        // Only first two ? get replaced; third remains
        assert_eq!(result, "SELECT 'a' , 'b' , ?");
    }
}
