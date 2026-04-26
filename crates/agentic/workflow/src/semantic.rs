//! Semantic query compilation using airlayer directly.
//!
//! Uses `airlayer::SemanticEngine::load()` to parse `.view.yml` and `.topic.yml`
//! files and compile semantic queries to SQL. No oxy dependency — uses local
//! config types and airlayer's native types end-to-end.

use std::collections::HashSet;
use std::path::Path;

use airlayer::engine::query::{
    FilterOperator, OrderBy, QueryFilter, QueryRequest, TimeDimensionQuery,
};
use airlayer::schema::models::TopicFilterType;
use chrono::{Local, NaiveDate};
use serde_json::Value as JsonValue;

use crate::config::{SemanticFilterType, SemanticQueryConfig, TimeGranularity};
use crate::error::WorkflowError;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolve a semantic query against the semantic layer and compile to SQL.
///
/// Returns `(sql, database_name)`.
pub fn resolve_and_compile(
    scan_path: &Path,
    databases: &[airlayer::DatabaseConfig],
    task: &SemanticQueryConfig,
) -> Result<(String, String), WorkflowError> {
    let dialects = airlayer::DatasourceDialectMap::from_config_databases(databases);

    // airlayer parses .view.yml + .topic.yml and creates the engine.
    let engine = airlayer::SemanticEngine::load(scan_path, Some(scan_path), dialects)
        .map_err(|e| WorkflowError::Runtime(format!("semantic engine error: {e}")))?;

    let semantic_layer = engine.semantic_layer();

    // Resolve topic.
    let topic = resolve_topic(semantic_layer, task)?;

    // Get database from views.
    let views: Vec<&airlayer::View> = semantic_layer
        .views
        .iter()
        .filter(|v| topic.views.contains(&v.name))
        .collect();

    let database_name = views
        .iter()
        .find_map(|v| v.datasource.clone())
        .ok_or_else(|| {
            WorkflowError::Validation(format!("No datasource found for topic '{}'", topic.name))
        })?;

    // Build date fields for filter normalization.
    let date_fields = collect_date_fields(&views);

    // Build query request.
    let request = build_query_request(
        task,
        &topic.name,
        topic.base_view.as_ref(),
        topic.default_filters.as_ref(),
        &date_fields,
    )?;

    // Compile to SQL.
    let result = engine
        .compile_query(&request)
        .map_err(|e| WorkflowError::Runtime(format!("query compilation error: {e}")))?;

    let sql = substitute_params(&result.sql, &result.params);
    Ok((sql, database_name))
}

/// Get the database (datasource) name from the first view that has one.
pub fn get_database_from_views(views: &[airlayer::View]) -> Option<String> {
    views.iter().find_map(|v| v.datasource.clone())
}

// ---------------------------------------------------------------------------
// Internal: topic resolution
// ---------------------------------------------------------------------------

fn resolve_topic(
    semantic_layer: &airlayer::SemanticLayer,
    task: &SemanticQueryConfig,
) -> Result<airlayer::Topic, WorkflowError> {
    let empty = Vec::new();
    let topics = semantic_layer.topics.as_ref().unwrap_or(&empty);

    if let Some(topic_name) = &task.topic {
        topics
            .iter()
            .find(|t| t.name == *topic_name)
            .cloned()
            .ok_or_else(|| {
                let available: Vec<_> = topics.iter().map(|t| t.name.clone()).collect();
                WorkflowError::Validation(format!(
                    "Topic '{}' not found. Available: {:?}",
                    topic_name, available
                ))
            })
    } else {
        let mut view_names = HashSet::new();
        for dim in &task.dimensions {
            if let Some((view, _)) = dim.split_once('.') {
                view_names.insert(view.to_string());
            }
        }
        for td in &task.time_dimensions {
            if let Some((view, _)) = td.dimension.split_once('.') {
                view_names.insert(view.to_string());
            }
        }
        for m in &task.measures {
            if let Some((view, _)) = m.split_once('.') {
                view_names.insert(view.to_string());
            }
        }
        if view_names.is_empty() {
            return Err(WorkflowError::Validation(
                "No dimensions or measures specified".to_string(),
            ));
        }
        Ok(airlayer::Topic {
            name: "adhoc_query".to_string(),
            description: Some("Ad-hoc query inferred from views".to_string()),
            views: view_names.into_iter().collect(),
            base_view: None,
            retrieval: None,
            default_filters: None,
            meta: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal: date field tracking
// ---------------------------------------------------------------------------

fn collect_date_fields(views: &[&airlayer::View]) -> HashSet<String> {
    use airlayer::schema::models::DimensionType;
    let mut date_fields = HashSet::new();
    for view in views {
        for dim in &view.dimensions {
            if matches!(
                dim.dimension_type,
                DimensionType::Date | DimensionType::Datetime
            ) {
                date_fields.insert(format!("{}.{}", view.name, dim.name));
            }
        }
    }
    date_fields
}

fn normalize_date_value(date: &str) -> Result<String, WorkflowError> {
    if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok() {
        return Ok(date.to_string());
    }
    let result = chrono_english::parse_date_string(date, Local::now(), chrono_english::Dialect::Us)
        .map_err(|e| {
            WorkflowError::Runtime(format!(
                "Failed to parse date '{}': {}. Expected YYYY-MM-DD or relative expression.",
                date, e
            ))
        })?;
    Ok(result.format("%Y-%m-%d").to_string())
}

// ---------------------------------------------------------------------------
// Internal: query request building
// ---------------------------------------------------------------------------

fn build_query_request(
    task: &SemanticQueryConfig,
    topic_name: &str,
    base_view: Option<&String>,
    default_filters: Option<&Vec<airlayer::schema::models::TopicFilter>>,
    date_fields: &HashSet<String>,
) -> Result<QueryRequest, WorkflowError> {
    let mut filters = Vec::new();

    if let Some(defaults) = default_filters {
        for df in defaults {
            let field = qualify_field(&df.field, topic_name);
            let (op, vals) = convert_topic_filter_type(&df.filter_type, &field, date_fields)?;
            filters.push(QueryFilter {
                member: Some(field),
                operator: Some(op),
                values: vals,
                and: None,
                or: None,
            });
        }
    }

    for f in &task.filters {
        let field = qualify_field(&f.field, topic_name);
        let (op, vals) = convert_semantic_filter_type(&f.filter_type, &field, date_fields)?;
        filters.push(QueryFilter {
            member: Some(field),
            operator: Some(op),
            values: vals,
            and: None,
            or: None,
        });
    }

    let order: Vec<OrderBy> = task
        .orders
        .iter()
        .map(|o| OrderBy {
            id: qualify_field(&o.field, topic_name),
            desc: o.direction.to_lowercase() == "desc",
        })
        .collect();

    let time_dimensions: Vec<TimeDimensionQuery> = task
        .time_dimensions
        .iter()
        .map(|td| TimeDimensionQuery {
            dimension: qualify_field(&td.dimension, topic_name),
            granularity: td.granularity.as_ref().map(granularity_to_string),
            date_range: None,
        })
        .collect();

    let through = base_view.map(|bv| vec![bv.clone()]).unwrap_or_default();

    Ok(QueryRequest {
        measures: task.measures.clone(),
        dimensions: task.dimensions.clone(),
        filters,
        segments: vec![],
        time_dimensions,
        order,
        limit: task.limit,
        offset: task.offset,
        timezone: None,
        ungrouped: false,
        through,
        motif: None,
        motif_params: Default::default(),
    })
}

fn qualify_field(field: &str, topic_name: &str) -> String {
    if field.contains('.') {
        field.to_string()
    } else {
        format!("{topic_name}.{field}")
    }
}

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

// ---------------------------------------------------------------------------
// Internal: filter conversion
// ---------------------------------------------------------------------------

fn convert_topic_filter_type(
    ft: &TopicFilterType,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<(FilterOperator, Vec<String>), WorkflowError> {
    match ft {
        TopicFilterType::Eq(f) => Ok((
            FilterOperator::Equals,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Neq(f) => Ok((
            FilterOperator::NotEquals,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Gt(f) => Ok((
            FilterOperator::Gt,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Gte(f) => Ok((
            FilterOperator::Gte,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Lt(f) => Ok((
            FilterOperator::Lt,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::Lte(f) => Ok((
            FilterOperator::Lte,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        TopicFilterType::In(f) => {
            let v = f
                .values
                .iter()
                .map(|v| jv2s(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::Equals, v))
        }
        TopicFilterType::NotIn(f) => {
            let v = f
                .values
                .iter()
                .map(|v| jv2s(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::NotEquals, v))
        }
        TopicFilterType::InDateRange(f) => Ok((
            FilterOperator::InDateRange,
            vec![
                jv2s(&f.from, field, date_fields)?,
                jv2s(&f.to, field, date_fields)?,
            ],
        )),
        TopicFilterType::NotInDateRange(f) => Ok((
            FilterOperator::NotInDateRange,
            vec![
                jv2s(&f.from, field, date_fields)?,
                jv2s(&f.to, field, date_fields)?,
            ],
        )),
    }
}

fn convert_semantic_filter_type(
    ft: &SemanticFilterType,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<(FilterOperator, Vec<String>), WorkflowError> {
    match ft {
        SemanticFilterType::Eq(f) => Ok((
            FilterOperator::Equals,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Neq(f) => Ok((
            FilterOperator::NotEquals,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Gt(f) => Ok((
            FilterOperator::Gt,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Gte(f) => Ok((
            FilterOperator::Gte,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Lt(f) => Ok((
            FilterOperator::Lt,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::Lte(f) => Ok((
            FilterOperator::Lte,
            vec![jv2s(&f.value, field, date_fields)?],
        )),
        SemanticFilterType::In(f) => {
            let v = f
                .values
                .iter()
                .map(|v| jv2s(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::Equals, v))
        }
        SemanticFilterType::NotIn(f) => {
            let v = f
                .values
                .iter()
                .map(|v| jv2s(v, field, date_fields))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((FilterOperator::NotEquals, v))
        }
        SemanticFilterType::InDateRange(f) => Ok((
            FilterOperator::InDateRange,
            vec![
                jv2s(&f.from, field, date_fields)?,
                jv2s(&f.to, field, date_fields)?,
            ],
        )),
        SemanticFilterType::NotInDateRange(f) => Ok((
            FilterOperator::NotInDateRange,
            vec![
                jv2s(&f.from, field, date_fields)?,
                jv2s(&f.to, field, date_fields)?,
            ],
        )),
    }
}

/// JSON value to string, with date normalization for date fields.
fn jv2s(
    value: &JsonValue,
    field: &str,
    date_fields: &HashSet<String>,
) -> Result<String, WorkflowError> {
    let s = match value {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => {
            return Err(WorkflowError::Runtime(format!(
                "NULL filter value for '{field}'"
            )));
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    };
    if date_fields.contains(field) {
        return normalize_date_value(&s);
    }
    Ok(s)
}

// ---------------------------------------------------------------------------
// Internal: parameter substitution
// ---------------------------------------------------------------------------

fn substitute_params(sql: &str, params: &[String]) -> String {
    if params.is_empty() {
        return sql.to_string();
    }
    let uses_positional = (0..params.len())
        .any(|i| sql.contains(&format!("${}", i + 1)) || sql.contains(&format!("@p{}", i)));
    let mut result = sql.to_string();
    if uses_positional {
        for (i, param) in params.iter().enumerate().rev() {
            let lit = format!("'{}'", param.replace('\'', "''"));
            result = result.replace(&format!("${}", i + 1), &lit);
            result = result.replace(&format!("@p{}", i), &lit);
        }
    } else {
        let mut idx = 0;
        while result.contains('?') && idx < params.len() {
            let lit = format!("'{}'", params[idx].replace('\'', "''"));
            result = result.replacen('?', &lit, 1);
            idx += 1;
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_params_positional() {
        let sql = "SELECT * FROM t WHERE a = $1 AND b = $2";
        let params = vec!["hello".into(), "world".into()];
        assert_eq!(
            substitute_params(sql, &params),
            "SELECT * FROM t WHERE a = 'hello' AND b = 'world'"
        );
    }

    #[test]
    fn test_substitute_params_question_mark() {
        let sql = "SELECT * FROM t WHERE a = ? AND b = ?";
        let params = vec!["hello".into(), "world".into()];
        assert_eq!(
            substitute_params(sql, &params),
            "SELECT * FROM t WHERE a = 'hello' AND b = 'world'"
        );
    }

    #[test]
    fn test_qualify_field() {
        assert_eq!(qualify_field("revenue", "orders"), "orders.revenue");
        assert_eq!(qualify_field("orders.revenue", "orders"), "orders.revenue");
    }

    #[test]
    fn test_get_database_from_views() {
        let views = vec![
            airlayer::View {
                name: "v1".into(),
                description: None,
                label: None,
                datasource: None,
                dialect: None,
                table: Some("t".into()),
                sql: None,
                entities: vec![],
                dimensions: vec![],
                measures: None,
                segments: vec![],
                pre_aggregations: None,
                meta: None,
            },
            airlayer::View {
                name: "v2".into(),
                description: None,
                label: None,
                datasource: Some("my_db".into()),
                dialect: None,
                table: Some("t".into()),
                sql: None,
                entities: vec![],
                dimensions: vec![],
                measures: None,
                segments: vec![],
                pre_aggregations: None,
                meta: None,
            },
        ];
        assert_eq!(get_database_from_views(&views), Some("my_db".to_string()));
    }
}
