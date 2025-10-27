use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};

use crate::{
    adapters::connector::Connector,
    config::model::{
        SemanticFilter, SemanticFilterType, SemanticOrder, SemanticOrderDirection,
        SemanticQueryTask,
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
            let filter_type = match f.op.as_str() {
                "eq" => SemanticFilterType::Eq(crate::config::model::ScalarFilter {
                    value: f.value.clone(),
                }),
                "neq" => SemanticFilterType::Neq(crate::config::model::ScalarFilter {
                    value: f.value.clone(),
                }),
                "gt" => SemanticFilterType::Gt(crate::config::model::ScalarFilter {
                    value: f.value.clone(),
                }),
                "gte" => SemanticFilterType::Gte(crate::config::model::ScalarFilter {
                    value: f.value.clone(),
                }),
                "lt" => SemanticFilterType::Lt(crate::config::model::ScalarFilter {
                    value: f.value.clone(),
                }),
                "lte" => SemanticFilterType::Lte(crate::config::model::ScalarFilter {
                    value: f.value.clone(),
                }),
                "in" => {
                    let values = if let JsonValue::Array(arr) = &f.value {
                        arr.clone()
                    } else {
                        vec![f.value.clone()]
                    };
                    SemanticFilterType::In(crate::config::model::ArrayFilter { values })
                }
                "not_in" => {
                    let values = if let JsonValue::Array(arr) = &f.value {
                        arr.clone()
                    } else {
                        vec![f.value.clone()]
                    };
                    SemanticFilterType::NotIn(crate::config::model::ArrayFilter { values })
                }
                "in_date_range" => {
                    let values = if let JsonValue::Array(arr) = &f.value {
                        arr.clone()
                    } else {
                        vec![f.value.clone()]
                    };
                    let from = values.first().cloned().unwrap_or(JsonValue::Null);
                    let to = values.get(1).cloned().unwrap_or(JsonValue::Null);
                    SemanticFilterType::InDateRange(crate::config::model::DateRangeFilter {
                        from,
                        to,
                    })
                }
                "not_in_date_range" => {
                    let values = if let JsonValue::Array(arr) = &f.value {
                        arr.clone()
                    } else {
                        vec![f.value.clone()]
                    };
                    let from = values.first().cloned().unwrap_or(JsonValue::Null);
                    let to = values.get(1).cloned().unwrap_or(JsonValue::Null);
                    SemanticFilterType::NotInDateRange(crate::config::model::DateRangeFilter {
                        from,
                        to,
                    })
                }
                _ => SemanticFilterType::Eq(crate::config::model::ScalarFilter {
                    value: f.value.clone(),
                }), // Default fallback
            };
            let filter = SemanticFilter {
                field: f.field.clone(),
                filter_type,
            };
            let rendered_filter = render_filter(renderer, &filter)?;
            // Convert back to SemanticQueryFilter - get first value or array
            let value = match &rendered_filter.filter_type {
                SemanticFilterType::Eq(f)
                | SemanticFilterType::Neq(f)
                | SemanticFilterType::Gt(f)
                | SemanticFilterType::Gte(f)
                | SemanticFilterType::Lt(f)
                | SemanticFilterType::Lte(f) => f.value.clone(),
                SemanticFilterType::In(f) | SemanticFilterType::NotIn(f) => {
                    JsonValue::Array(f.values.clone())
                }
                SemanticFilterType::InDateRange(f) | SemanticFilterType::NotInDateRange(f) => {
                    JsonValue::Array(vec![f.from.clone(), f.to.clone()])
                }
            };
            Ok(crate::service::types::SemanticQueryFilter {
                field: rendered_filter.field,
                op: f.op.clone(), // Keep original string representation
                value,
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
    let filter_type = match &filter.filter_type {
        SemanticFilterType::Eq(f) => SemanticFilterType::Eq(crate::config::model::ScalarFilter {
            value: render_filter_value(renderer, &f.value, &field)?,
        }),
        SemanticFilterType::Neq(f) => SemanticFilterType::Neq(crate::config::model::ScalarFilter {
            value: render_filter_value(renderer, &f.value, &field)?,
        }),
        SemanticFilterType::Gt(f) => SemanticFilterType::Gt(crate::config::model::ScalarFilter {
            value: render_filter_value(renderer, &f.value, &field)?,
        }),
        SemanticFilterType::Gte(f) => SemanticFilterType::Gte(crate::config::model::ScalarFilter {
            value: render_filter_value(renderer, &f.value, &field)?,
        }),
        SemanticFilterType::Lt(f) => SemanticFilterType::Lt(crate::config::model::ScalarFilter {
            value: render_filter_value(renderer, &f.value, &field)?,
        }),
        SemanticFilterType::Lte(f) => SemanticFilterType::Lte(crate::config::model::ScalarFilter {
            value: render_filter_value(renderer, &f.value, &field)?,
        }),
        SemanticFilterType::In(f) => {
            let rendered_values = f
                .values
                .iter()
                .enumerate()
                .map(|(idx, v)| render_filter_value(renderer, v, &format!("{field}[{idx}]")))
                .collect::<Result<Vec<_>, _>>()?;
            SemanticFilterType::In(crate::config::model::ArrayFilter {
                values: rendered_values,
            })
        }
        SemanticFilterType::NotIn(f) => {
            let rendered_values = f
                .values
                .iter()
                .enumerate()
                .map(|(idx, v)| render_filter_value(renderer, v, &format!("{field}[{idx}]")))
                .collect::<Result<Vec<_>, _>>()?;
            SemanticFilterType::NotIn(crate::config::model::ArrayFilter {
                values: rendered_values,
            })
        }
        SemanticFilterType::InDateRange(f) => {
            let from = render_filter_value(renderer, &f.from, &format!("{field}.from"))?;
            let to = render_filter_value(renderer, &f.to, &format!("{field}.to"))?;
            SemanticFilterType::InDateRange(crate::config::model::DateRangeFilter { from, to })
        }
        SemanticFilterType::NotInDateRange(f) => {
            let from = render_filter_value(renderer, &f.from, &format!("{field}.from"))?;
            let to = render_filter_value(renderer, &f.to, &format!("{field}.to"))?;
            SemanticFilterType::NotInDateRange(crate::config::model::DateRangeFilter { from, to })
        }
    };
    Ok(SemanticFilter { field, filter_type })
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

        // Step 1: Extract unique views from requested fields to determine join hints
        let requested_views = self.extract_views_from_query(&input.task, &input.topic.name);

        // Step 2: Convert to CubeJS query and get SQL with base_view enforcement and default filters
        // Default filters from the topic will be automatically merged with user-provided filters
        let cubejs_query = self.convert_to_cubejs_query(
            &input.task,
            &input.topic.name,
            input.topic.base_view.as_ref(),
            &requested_views,
            input.topic.default_filters.as_ref(),
        )?;
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
            input.task.query.topic
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
                        SemanticFilterType::Eq(crate::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Neq(f) => {
                        SemanticFilterType::Neq(crate::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Gt(f) => {
                        SemanticFilterType::Gt(crate::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Gte(f) => {
                        SemanticFilterType::Gte(crate::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Lt(f) => {
                        SemanticFilterType::Lt(crate::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::Lte(f) => {
                        SemanticFilterType::Lte(crate::config::model::ScalarFilter {
                            value: f.value.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::In(f) => {
                        SemanticFilterType::In(crate::config::model::ArrayFilter {
                            values: f.values.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::NotIn(f) => {
                        SemanticFilterType::NotIn(crate::config::model::ArrayFilter {
                            values: f.values.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::InDateRange(f) => {
                        SemanticFilterType::InDateRange(crate::config::model::DateRangeFilter {
                            from: f.from.clone(),
                            to: f.to.clone(),
                        })
                    }
                    oxy_semantic::TopicFilterType::NotInDateRange(f) => {
                        SemanticFilterType::NotInDateRange(crate::config::model::DateRangeFilter {
                            from: f.from.clone(),
                            to: f.to.clone(),
                        })
                    }
                };

                let semantic_filter = SemanticFilter {
                    field: default_filter.field.clone(),
                    filter_type,
                };

                let cubejs_filter = self.convert_filter_to_cubejs(&semantic_filter, topic_name)?;
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
            let user_filters = task
                .query
                .filters
                .iter()
                .map(|filter| {
                    // Convert SemanticQueryFilter to SemanticFilter for CubeJS conversion
                    let filter_type = match filter.op.as_str() {
                        "eq" => SemanticFilterType::Eq(crate::config::model::ScalarFilter {
                            value: filter.value.clone(),
                        }),
                        "neq" => SemanticFilterType::Neq(crate::config::model::ScalarFilter {
                            value: filter.value.clone(),
                        }),
                        "gt" => SemanticFilterType::Gt(crate::config::model::ScalarFilter {
                            value: filter.value.clone(),
                        }),
                        "gte" => SemanticFilterType::Gte(crate::config::model::ScalarFilter {
                            value: filter.value.clone(),
                        }),
                        "lt" => SemanticFilterType::Lt(crate::config::model::ScalarFilter {
                            value: filter.value.clone(),
                        }),
                        "lte" => SemanticFilterType::Lte(crate::config::model::ScalarFilter {
                            value: filter.value.clone(),
                        }),
                        "in" => {
                            let values = if let JsonValue::Array(arr) = &filter.value {
                                arr.clone()
                            } else {
                                vec![filter.value.clone()]
                            };
                            SemanticFilterType::In(crate::config::model::ArrayFilter { values })
                        }
                        "not_in" => {
                            let values = if let JsonValue::Array(arr) = &filter.value {
                                arr.clone()
                            } else {
                                vec![filter.value.clone()]
                            };
                            SemanticFilterType::NotIn(crate::config::model::ArrayFilter { values })
                        }
                        "in_date_range" => {
                            let values = if let JsonValue::Array(arr) = &filter.value {
                                arr.clone()
                            } else {
                                vec![filter.value.clone()]
                            };
                            let from = values.first().cloned().unwrap_or(JsonValue::Null);
                            let to = values.get(1).cloned().unwrap_or(JsonValue::Null);
                            SemanticFilterType::InDateRange(crate::config::model::DateRangeFilter {
                                from,
                                to,
                            })
                        }
                        "not_in_date_range" => {
                            let values = if let JsonValue::Array(arr) = &filter.value {
                                arr.clone()
                            } else {
                                vec![filter.value.clone()]
                            };
                            let from = values.first().cloned().unwrap_or(JsonValue::Null);
                            let to = values.get(1).cloned().unwrap_or(JsonValue::Null);
                            SemanticFilterType::NotInDateRange(
                                crate::config::model::DateRangeFilter { from, to },
                            )
                        }
                        _ => SemanticFilterType::Eq(crate::config::model::ScalarFilter {
                            value: filter.value.clone(),
                        }), // Default fallback
                    };
                    let semantic_filter = SemanticFilter {
                        field: filter.field.clone(),
                        filter_type,
                    };
                    self.convert_filter_to_cubejs(&semantic_filter, topic_name)
                })
                .collect::<Result<Vec<_>, _>>()?;
            all_filters.extend(user_filters);
        }

        // Set filters in query if any exist
        if !all_filters.is_empty() {
            query["filters"] = JsonValue::Array(all_filters);
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

        let operator = filter.filter_type.operator_name();

        // Resolve relative dates for date range filters
        let values = match &filter.filter_type {
            crate::config::model::SemanticFilterType::InDateRange(date_filter)
            | crate::config::model::SemanticFilterType::NotInDateRange(date_filter) => {
                let resolved = date_filter.resolve_relative_dates()?;
                vec![resolved.from, resolved.to]
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
        Ok(file_path)
    }
}

pub fn build_semantic_query_executable() -> impl Executable<SemanticQueryTask, Response = Output> {
    ExecutableBuilder::new()
        .map(SemanticQueryTaskMapper)
        .executable(SemanticQueryExecutable::new())
}
