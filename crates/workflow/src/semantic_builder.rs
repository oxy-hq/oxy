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
    config::model::{
        SemanticFilter, SemanticFilterType, SemanticOrder, SemanticOrderDirection,
        SemanticQueryTask,
    },
    connector::Connector,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        renderer::Renderer,
        types::{Chunk, EventKind, Output, TableReference, utils::record_batches_to_2d_array},
    },
    observability::events::{tool as tool_events, workflow as workflow_events},
    service::types::SemanticQueryParams,
    types::{DateRange, SemanticQuery, TimeDimension},
    utils::truncate_datasets,
};
use oxy_shared::errors::OxyError;

use crate::semantic_validator_builder::{
    SemanticQueryError, SemanticQueryValidation, ValidatedSemanticQuery,
    validate_semantic_query_task,
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

            // Convert date_range (process relative dates)
            let rendered_date_range = td
                .date_range
                .as_ref()
                .map(|dr| convert_date_range(dr))
                .transpose()?;

            // Convert compare_date_range (process relative dates)
            let rendered_compare_date_range = td
                .compare_date_range
                .as_ref()
                .map(|dr| convert_date_range(dr))
                .transpose()?;

            Ok(TimeDimension {
                dimension: rendered_dimension,
                granularity: td.granularity.clone(),
                date_range: rendered_date_range,
                compare_date_range: rendered_compare_date_range,
            })
        })
        .collect::<Result<Vec<_>, OxyError>>()
}

/// Convert DateRange to absolute ISO 8601 dates
/// Handles relative expressions like "last 7 days", "this month", "from 30 days ago to now"
fn convert_date_range(range: &DateRange) -> Result<DateRange, OxyError> {
    match range {
        DateRange::Relative(expr) => {
            // Parse relative expression using chrono-english
            let result =
                chrono_english::parse_date_string(expr, Local::now(), chrono_english::Dialect::Us)
                    .map_err(|e| {
                        OxyError::RuntimeError(format!(
                            "Failed to parse relative date expression '{}': {}",
                            expr, e
                        ))
                    })?;

            // Convert to ISO 8601 date format (YYYY-MM-DD)
            let date_str = result.format("%Y-%m-%d").to_string();
            Ok(DateRange::Dates(vec![date_str]))
        }
        DateRange::Dates(dates) => {
            // Process each date in the array
            let normalized_dates = dates
                .iter()
                .map(|date| normalize_date_value(date))
                .collect::<Result<Vec<_>, OxyError>>()?;
            Ok(DateRange::Dates(normalized_dates))
        }
    }
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

        let requested_views = self.extract_views_from_query(&input.task, &input.topic.name);
        let date_fields = collect_date_fields(&input.views);

        let cubejs_query = self.convert_to_cubejs_query(
            &input.task,
            &input.topic.name,
            input.topic.base_view.as_ref(),
            &requested_views,
            input.topic.default_filters.as_ref(),
            &date_fields,
        )?;

        let mut sql_query = self.get_sql_from_cubejs(&cubejs_query).await?;

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

        // Step 1: Extract unique views from requested fields to determine join hints
        let requested_views = self.extract_views_from_query(&input.task, &input.topic.name);
        let date_fields = collect_date_fields(&input.views);

        // Step 2: Convert to CubeJS query and get SQL with base_view enforcement and default filters
        // Default filters from the topic will be automatically merged with user-provided filters
        let cubejs_query = match self.convert_to_cubejs_query(
            &input.task,
            &input.topic.name,
            input.topic.base_view.as_ref(),
            &requested_views,
            input.topic.default_filters.as_ref(),
            &date_fields,
        ) {
            Ok(query) => query,
            Err(e) => {
                tracing::error!(
                    "Failed to convert semantic query to CubeJS format for topic '{}': {e}",
                    input.topic.name
                );
                artifact_value.sql_generation_error =
                    Some("Failed to process semantic query".to_string());
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
            "Generated CubeJS query for topic '{}': {cubejs_query:?}",
            input.topic.name
        );

        let mut sql_query = match self.get_sql_from_cubejs(&cubejs_query).await {
            Ok(sql) => sql,
            Err(e) => {
                tracing::error!(
                    "Failed to generate SQL from CubeJS query for topic '{}': {e}",
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

    /// Extract unique view names from the query fields (dimensions, measures, filters, orders)
    fn extract_views_from_query(&self, task: &SemanticQueryTask, topic_name: &str) -> Vec<String> {
        let mut views = HashSet::new();

        // Extract from dimensions
        for dim in &task.query.dimensions {
            if let Some(view_name) = self.extract_view_name(dim, topic_name) {
                views.insert(view_name);
            }
        }

        // Extract from measures
        for measure in &task.query.measures {
            if let Some(view_name) = self.extract_view_name(measure, topic_name) {
                views.insert(view_name);
            }
        }

        // Extract from filters
        for filter in &task.query.filters {
            if let Some(view_name) = self.extract_view_name(&filter.field, topic_name) {
                views.insert(view_name);
            }
        }

        // Extract from orders
        for order in &task.query.orders {
            if let Some(view_name) = self.extract_view_name(&order.field, topic_name) {
                views.insert(view_name);
            }
        }

        views.into_iter().collect()
    }

    /// Extract view name from a field reference
    /// Field can be in format "view.field" or just "field" (assumes topic name as view)
    fn extract_view_name(&self, field: &str, topic_name: &str) -> Option<String> {
        if field.contains('.') {
            field.split('.').next().map(|s| s.to_string())
        } else {
            // If no view prefix, assume it's from the topic itself
            Some(topic_name.to_string())
        }
    }

    /// Generate join hints for base_view enforcement
    /// Returns an array of [from_view, to_view] pairs that CubeJS should use for joins.
    /// This ensures all joins start from the base_view, creating a star schema pattern
    /// where the base_view is at the center and all other views join to it.
    fn generate_join_hints(&self, base_view: &str, requested_views: &[String]) -> Vec<JsonValue> {
        let mut hints = Vec::new();

        for view in requested_views {
            // Don't create a hint for the base view to itself
            if view != base_view {
                // Create a join hint from base_view to this view
                // Format: ["base_view", "target_view"]
                hints.push(serde_json::json!([base_view, view]));
            }
        }

        hints
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

    /// Convert validated semantic query to CubeJS query JSON format
    fn convert_to_cubejs_query(
        &self,
        task: &SemanticQueryTask,
        topic_name: &str,
        base_view: Option<&String>,
        requested_views: &[String],
        default_filters: Option<&Vec<oxy_semantic::TopicFilter>>,
        date_fields: &HashSet<String>,
    ) -> Result<JsonValue, OxyError> {
        let mut query = serde_json::json!({
            "measures": task.query.measures,
            "dimensions": task.query.dimensions
        });

        // Add join hints if base_view is specified
        // This enforces that all joins start from the base_view
        if let Some(base_view_name) = base_view {
            let join_hints = self.generate_join_hints(base_view_name, requested_views);
            if !join_hints.is_empty() {
                query["joinHints"] = JsonValue::Array(join_hints);
                tracing::info!(
                    "Applied base_view enforcement: all joins will start from '{}' (join hints: {:?})",
                    base_view_name,
                    query["joinHints"]
                );
            }
        }

        // Merge default filters from topic with user-provided filters
        let mut all_filters = Vec::new();

        // Add default filters from topic (applied first, with AND logic)
        if let Some(default_filters) = default_filters {
            for default_filter in default_filters {
                // Convert TopicFilter to SemanticFilter, then to CubeJS format
                let filter_type = match &default_filter.filter_type {
                    oxy_semantic::TopicFilterType::Eq(f) => {
                        SemanticFilterType::Eq(oxy::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Neq(f) => {
                        SemanticFilterType::Neq(oxy::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Gt(f) => {
                        SemanticFilterType::Gt(oxy::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Gte(f) => {
                        SemanticFilterType::Gte(oxy::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Lt(f) => {
                        SemanticFilterType::Lt(oxy::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Lte(f) => {
                        SemanticFilterType::Lte(oxy::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::In(f) => {
                        SemanticFilterType::In(oxy::config::model::ArrayFilter {
                            values: f.values.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::NotIn(f) => {
                        SemanticFilterType::NotIn(oxy::config::model::ArrayFilter {
                            values: f.values.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::InDateRange(f) => {
                        SemanticFilterType::InDateRange(oxy::config::model::DateRangeFilter {
                            from: f.from.clone(),
                            to: f.to.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::NotInDateRange(f) => {
                        SemanticFilterType::NotInDateRange(oxy::config::model::DateRangeFilter {
                            from: f.from.clone(),
                            to: f.to.clone(),
                        })
                    }
                };

                let semantic_filter = SemanticFilter {
                    field: default_filter.field.clone(),
                    filter_type,
                };

                let cubejs_filter =
                    self.convert_filter_to_cubejs(&semantic_filter, topic_name, date_fields)?;
                all_filters.push(cubejs_filter);
            }

            if !default_filters.is_empty() {
                tracing::info!(
                    "Applied {} default filter(s) from topic '{}'",
                    default_filters.len(),
                    topic_name
                );
            }
        }

        // Add user-provided filters
        if !task.query.filters.is_empty() {
            let user_filters: Vec<JsonValue> = task
                .query
                .filters
                .iter()
                .map(|f| self.convert_filter_to_cubejs(f, topic_name, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            all_filters.extend(user_filters);
        }

        // Set filters in query if any exist
        if !all_filters.is_empty() {
            query["filters"] = JsonValue::Array(all_filters);
        }

        // Add order - always include the order field, even if empty
        // This prevents Cube from adding a default ORDER BY
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

        // Add limit and offset if present
        if let Some(limit) = task.query.limit {
            query["limit"] = JsonValue::Number(serde_json::Number::from(limit));
        }
        if let Some(offset) = task.query.offset {
            query["offset"] = JsonValue::Number(serde_json::Number::from(offset));
        }

        // Add time dimensions if present
        if !task.query.time_dimensions.is_empty() {
            let time_dims: Vec<JsonValue> = task
                .query
                .time_dimensions
                .iter()
                .map(|td| self.convert_time_dimension_to_cubejs(td, topic_name))
                .collect::<Result<Vec<_>, _>>()?;
            query["timeDimensions"] = JsonValue::Array(time_dims);
            tracing::info!(
                "Added {} time dimension(s) to CubeJS query",
                task.query.time_dimensions.len()
            );
        }

        Ok(query)
    }

    /// Convert a TimeDimension to CubeJS timeDimensions format
    /// Cube.dev format: { "dimension": "View.field", "granularity": "month", "dateRange": ["2023-01-01", "2023-12-31"] }
    fn convert_time_dimension_to_cubejs(
        &self,
        td: &TimeDimension,
        topic_name: &str,
    ) -> Result<JsonValue, OxyError> {
        // Qualify dimension name with topic if not already qualified
        let dimension_name = if td.dimension.contains('.') {
            td.dimension.clone()
        } else {
            format!("{}.{}", topic_name, td.dimension)
        };

        let mut obj = serde_json::json!({
            "dimension": dimension_name
        });

        // Add granularity if present
        if let Some(ref granularity) = td.granularity {
            obj["granularity"] = self.convert_granularity_to_cubejs(granularity);
        }

        // Add dateRange if present (already normalized to ISO format by render_time_dimensions)
        if let Some(ref date_range) = td.date_range {
            obj["dateRange"] = self.convert_date_range_to_cubejs(date_range)?;
        }

        // Add compareDateRange if present (for period-over-period analysis)
        if let Some(ref compare_range) = td.compare_date_range {
            obj["compareDateRange"] = self.convert_date_range_to_cubejs(compare_range)?;
        }

        Ok(obj)
    }

    /// Convert TimeGranularity to Cube.dev granularity string
    fn convert_granularity_to_cubejs(&self, granularity: &TimeGranularity) -> JsonValue {
        match granularity {
            TimeGranularity::Year => serde_json::json!("year"),
            TimeGranularity::Quarter => serde_json::json!("quarter"),
            TimeGranularity::Month => serde_json::json!("month"),
            TimeGranularity::Week => serde_json::json!("week"),
            TimeGranularity::Day => serde_json::json!("day"),
            TimeGranularity::Hour => serde_json::json!("hour"),
            TimeGranularity::Minute => serde_json::json!("minute"),
            TimeGranularity::Second => serde_json::json!("second"),
        }
    }

    /// Convert DateRange to Cube.dev dateRange format (array of 1-2 date strings)
    /// By the time this is called, relative dates have already been converted to ISO format
    fn convert_date_range_to_cubejs(&self, range: &DateRange) -> Result<JsonValue, OxyError> {
        match range {
            DateRange::Relative(expr) => {
                // This shouldn't happen as render_time_dimensions converts relative to absolute
                // But handle it gracefully by passing through as single-element array
                Ok(serde_json::json!([expr]))
            }
            DateRange::Dates(dates) => {
                if dates.is_empty() {
                    return Err(OxyError::ValidationError(
                        "Date range must have at least 1 date".to_string(),
                    ));
                }
                if dates.len() > 2 {
                    return Err(OxyError::ValidationError(format!(
                        "Date range must have at most 2 dates, got {}",
                        dates.len()
                    )));
                }
                // Return as JSON array
                Ok(serde_json::json!(dates))
            }
        }
    }

    /// Convert semantic filter to CubeJS filter format
    fn convert_filter_to_cubejs(
        &self,
        filter: &SemanticFilter,
        topic_name: &str,
        date_fields: &HashSet<String>,
    ) -> Result<JsonValue, OxyError> {
        let field_name = if filter.field.contains('.') {
            filter.field.clone()
        } else {
            format!("{}.{}", topic_name, filter.field)
        };

        let operator = filter.filter_type.operator_name();
        let is_date_field = date_fields.contains(&field_name);

        // Resolve relative dates for date range filters and date-typed scalar/array filters
        let values = match &filter.filter_type {
            oxy::config::model::SemanticFilterType::InDateRange(date_filter)
            | oxy::config::model::SemanticFilterType::NotInDateRange(date_filter) => {
                let mut vals = Vec::new();
                let resolved_from = if let Some(from_str) = date_filter.from.as_str() {
                    serde_json::Value::String(normalize_date_value(from_str)?)
                } else {
                    date_filter.from.clone()
                };
                vals.push(resolved_from);
                let resolved_to = if let Some(to_str) = date_filter.to.as_str() {
                    serde_json::Value::String(normalize_date_value(to_str)?)
                } else {
                    date_filter.to.clone()
                };
                vals.push(resolved_to);
                vals
            }
            _ if is_date_field => {
                // Resolve relative date expressions for scalar/array filters on date fields
                filter
                    .filter_type
                    .values()
                    .into_iter()
                    .map(|v| {
                        if let Some(s) = v.as_str() {
                            Ok(serde_json::Value::String(normalize_date_value(s)?))
                        } else {
                            Ok(v)
                        }
                    })
                    .collect::<Result<Vec<_>, OxyError>>()?
            }
            _ => filter.filter_type.values(),
        };

        Ok(serde_json::json!({
            "member": field_name,
            "operator": operator,
            "values": values
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
            oxy::config::model::SemanticOrderDirection::Asc => "asc",
            oxy::config::model::SemanticOrderDirection::Desc => "desc",
        };

        serde_json::json!([field_name, direction])
    }

    /// Resolve variables in SQL query using RuntimeVariableResolver
    fn resolve_variables_in_sql(
        &self,
        _execution_context: &ExecutionContext,
        sql_query: String,
        variables: HashMap<String, JsonValue>,
    ) -> Result<String, OxyError> {
        // TODO: Implement global variables extraction from GlobalRegistry
        // For now, we'll use an empty HashMap for globals
        let global_vars: HashMap<String, JsonValue> = HashMap::new();

        // Collect environment variables (prefixed with OXY_)
        let env_vars: HashMap<String, JsonValue> = std::env::vars()
            .filter(|(key, _)| key.starts_with("OXY_VAR_"))
            .map(|(key, value)| {
                // Remove OXY_VAR_ prefix for cleaner variable names
                let var_name = key.strip_prefix("OXY_VAR_").unwrap_or(&key).to_lowercase();
                (var_name, JsonValue::String(value))
            })
            .collect();

        // Create variable resolver from multiple sources with priority order
        let resolver = RuntimeVariableResolver::from_sources(
            Some(variables),   // Task variables have highest priority
            None,              // No agent variables for now (could be added later)
            Some(global_vars), // Global variables from config
            Some(env_vars),    // Environment variables (lowest priority)
        )
        .map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create variable resolver: {}", e))
        })?;

        // Resolve variables in the SQL query
        let resolved_sql = resolver.resolve_sql_variables(sql_query).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to resolve variables in SQL: {}", e))
        })?;

        Ok(resolved_sql)
    }

    /// Get SQL query from CubeJS /sql endpoint and handle parameters
    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::task::semantic_query::NAME_GET_SQL_FROM_CUBEJS,
        oxy.span_type = workflow_events::task::semantic_query::TYPE,
    ))]
    async fn get_sql_from_cubejs(&self, query: &JsonValue) -> Result<String, OxyError> {
        workflow_events::task::semantic_query::get_sql_input(query);

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
            .map_err(|e| SemanticQueryError::CubeJSError {
                details: format!("Failed to call CubeJS SQL API: {e}"),
            })?;

        let sql_status = sql_response.status();
        if !sql_status.is_success() {
            let error_text = sql_response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SemanticQueryError::CubeJSError {
                details: format!("SQL API Status {}: {}", sql_status, error_text),
            }
            .into());
        }

        let sql_response_json: JsonValue =
            sql_response
                .json()
                .await
                .map_err(|e| SemanticQueryError::CubeJSError {
                    details: format!("Failed to parse CubeJS SQL response JSON: {e}"),
                })?;

        // Extract SQL from response
        // CubeJS SQL response structure: { "sql": { "status": "ok", "sql": ["SELECT ...", [parameters]] } }
        let sql_obj =
            sql_response_json
                .get("sql")
                .ok_or_else(|| SemanticQueryError::CubeJSError {
                    details: format!(
                        "CubeJS SQL response missing 'sql' object. Response: {}",
                        sql_response_json
                    ),
                })?;

        // Check status
        if let Some(status) = sql_obj.get("status").and_then(|s| s.as_str())
            && status != "ok"
        {
            let error_msg = sql_obj
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(SemanticQueryError::CubeJSError {
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
                || SemanticQueryError::CubeJSError {
                    details: format!("CubeJS SQL response missing expected 'sql' array structure. Expected: {{\"sql\": [\"SELECT ...\", []]}}, got: {}", sql_obj),
                },
            )?;

        // Extract SQL query (first element)
        let sql_template = sql_array.first().and_then(|s| s.as_str()).ok_or_else(|| {
            SemanticQueryError::CubeJSError {
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

        workflow_events::task::semantic_query::get_sql_output(&final_sql);

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

    /// Convert a JSON value to a SQL literal string for parameter substitution.
    ///
    /// ## CubeJS Boolean Parameter Handling
    ///
    /// CubeJS converts boolean filter values to strings "true"/"false" in its parameter
    /// arrays. This causes issues with computed boolean expressions in ClickHouse:
    ///
    /// - Direct column: `deleted = 'true'` works (ClickHouse casts string to Bool)
    /// - Computed expr: `(deleted = true) = 'true'` fails (UInt8 result can't cast from string)
    ///
    /// We detect string "true"/"false" and output them as unquoted boolean literals.
    ///
    /// ## Edge Case: String columns with literal "true"/"false" values
    ///
    /// If you have a string column containing literal "true"/"false" values and need
    /// to filter on them as strings, use the `in` filter instead of `eq`:
    ///
    /// ```yaml
    /// # This will be treated as boolean (unquoted):
    /// default_filters:
    ///   - field: "string_column"
    ///     eq:
    ///       value: "true"
    ///
    /// # Use this for string comparison:
    /// default_filters:
    ///   - field: "string_column"
    ///     in:
    ///       values: ["true"]
    /// ```
    fn json_value_to_sql_literal(&self, value: &JsonValue) -> Result<String, OxyError> {
        match value {
            JsonValue::Null => Ok("NULL".to_string()),
            JsonValue::Bool(b) => Ok(b.to_string()),
            JsonValue::Number(n) => Ok(n.to_string()),
            JsonValue::String(s) => {
                // CubeJS converts boolean filter values to strings "true"/"false".
                // Output as unquoted boolean literals for SQL compatibility with
                // computed boolean expressions (see docstring for details).
                if s == "true" || s == "false" {
                    Ok(s.clone())
                } else {
                    // Escape single quotes and wrap in quotes
                    let escaped = s.replace('\'', "''");
                    Ok(format!("'{}'", escaped))
                }
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

pub fn build_semantic_query_executable() -> impl Executable<SemanticQueryTask, Response = Output> {
    ExecutableBuilder::new()
        .map(SemanticQueryTaskMapper)
        .executable(SemanticQueryExecutable::new())
}
