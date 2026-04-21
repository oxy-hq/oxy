//! Translate an airlayer [`QueryRequest`] back to raw-schema context for the
//! Solve fallback path.
//!
//! [`QueryRequest`]: airlayer::engine::query::QueryRequest

use crate::catalog::{Catalog, DimensionSummary, JoinPath, MetricDef, QueryContext};

use super::SemanticCatalog;
use super::filters::format_filter_as_sql;

/// Result of translating an airlayer `QueryRequest` to raw-schema references.
#[derive(Debug, Clone)]
pub struct RawSchemaTranslation {
    /// Rich context with raw table.column references for the Solve prompt.
    pub context: QueryContext,
    /// SQL aggregate expressions (e.g. `"SUM(orders.amount)"`).
    pub resolved_metrics: Vec<String>,
    /// Underlying table names.
    pub resolved_tables: Vec<String>,
    /// Raw WHERE clause fragments.
    pub resolved_filters: Vec<String>,
    /// Join triples: `(left_table, right_table, join_expression)`.
    pub join_path: Vec<(String, String, String)>,
}

impl SemanticCatalog {
    /// Translate an airlayer `QueryRequest` back to raw-schema context for the
    /// Solve fallback path.
    ///
    /// When `engine.compile_query` fails on a QueryRequest produced by the LLM,
    /// the Solve stage needs concrete `table.column` references (not `view.member`
    /// names) to write valid SQL. This method resolves each semantic member to its
    /// underlying expression and table.
    pub fn translate_to_raw_context(
        &self,
        request: &airlayer::engine::query::QueryRequest,
        compile_error: &str,
    ) -> RawSchemaTranslation {
        use std::collections::HashSet;

        let mut resolved_metrics = Vec::new();
        let mut resolved_tables = HashSet::new();
        let mut metric_defs = Vec::new();
        let mut dim_defs = Vec::new();
        let mut resolved_filters = Vec::new();

        // Translate measures → SQL aggregate expressions
        for member in &request.measures {
            if let Some((view, measure)) = self.find_measure(member) {
                let table = view.table.clone().unwrap_or_else(|| view.name.clone());
                let expr = measure.expr.clone().unwrap_or_else(|| measure.name.clone());
                let agg_expr = format!(
                    "{}({}.{})",
                    measure.measure_type.to_string().to_uppercase(),
                    table,
                    expr
                );
                resolved_metrics.push(agg_expr);
                resolved_tables.insert(table.clone());
                metric_defs.push(MetricDef {
                    name: measure.name.clone(),
                    expr: measure.expr.clone().unwrap_or_else(|| measure.name.clone()),
                    metric_type: measure.measure_type.to_string(),
                    table,
                    description: measure.description.clone(),
                    data_source: view.datasource.clone(),
                });
            }
        }

        // Translate dimensions → raw table.column
        for member in &request.dimensions {
            if let Some((view, dim)) = self.find_dimension(member) {
                let table = view.table.clone().unwrap_or_else(|| view.name.clone());
                resolved_tables.insert(table);
                dim_defs.push(DimensionSummary {
                    name: dim.name.clone(),
                    description: dim
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("{} dimension", dim.dimension_type)),
                    data_type: dim.dimension_type.to_string(),
                });
            }
        }

        // Translate time_dimensions → raw table.column + date filters
        for td in &request.time_dimensions {
            if let Some((view, dim)) = self.find_dimension(&td.dimension) {
                let table = view.table.clone().unwrap_or_else(|| view.name.clone());
                let col_expr = format!("{}.{}", table, dim.expr);
                resolved_tables.insert(table);
                dim_defs.push(DimensionSummary {
                    name: dim.name.clone(),
                    description: dim
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("{} time dimension", dim.dimension_type)),
                    data_type: dim.dimension_type.to_string(),
                });
                // Convert date_range to raw filter expressions
                if let Some(range) = &td.date_range
                    && range.len() == 2
                {
                    resolved_filters.push(format!("{} >= '{}'", col_expr, range[0]));
                    resolved_filters.push(format!("{} < '{}'", col_expr, range[1]));
                }
            }
        }

        // Translate structured filters → raw WHERE fragments
        for filter in &request.filters {
            if let Some(member) = &filter.member {
                if let Some((view, dim)) = self.find_dimension(member) {
                    let table = view.table.clone().unwrap_or_else(|| view.name.clone());
                    let col_expr = format!("{}.{}", table, dim.expr);
                    resolved_tables.insert(table);
                    if let Some(op) = &filter.operator {
                        resolved_filters.push(format_filter_as_sql(&col_expr, op, &filter.values));
                    }
                } else if let Some((view, measure)) = self.find_measure(member) {
                    let table = view.table.clone().unwrap_or_else(|| view.name.clone());
                    let expr = measure.expr.clone().unwrap_or_else(|| measure.name.clone());
                    let col_expr = format!("{}.{}", table, expr);
                    resolved_tables.insert(table);
                    if let Some(op) = &filter.operator {
                        resolved_filters.push(format_filter_as_sql(&col_expr, op, &filter.values));
                    }
                }
            }
        }

        // Collect join paths between involved tables
        let table_list: Vec<String> = resolved_tables.iter().cloned().collect();
        let mut join_path = Vec::new();
        for i in 0..table_list.len() {
            for j in (i + 1)..table_list.len() {
                if let Some(jp) = self.get_join_path(&table_list[i], &table_list[j]) {
                    join_path.push((table_list[i].clone(), table_list[j].clone(), jp.path));
                }
            }
        }

        // Build schema description for involved views
        let schema_lines: Vec<String> = self
            .engine
            .views()
            .iter()
            .filter(|v| {
                let t = v.table.as_deref().unwrap_or(&v.name);
                resolved_tables.contains(t) || resolved_tables.contains(&v.name)
            })
            .map(|v| {
                let source = v
                    .table
                    .as_deref()
                    .unwrap_or_else(|| v.sql.as_deref().unwrap_or("(sql)"));
                let measures: Vec<String> = v
                    .measures_list()
                    .iter()
                    .map(|m| {
                        let expr = m.expr.as_deref().unwrap_or(&m.name);
                        format!("{}({}) AS {}", m.measure_type, expr, m.name)
                    })
                    .collect();
                let dims: Vec<String> = v
                    .dimensions
                    .iter()
                    .map(|d| format!("{}.{} ({})", source, d.expr, d.dimension_type))
                    .collect();
                format!(
                    "table `{}`: measures=[{}]  columns=[{}]",
                    source,
                    measures.join(", "),
                    dims.join(", ")
                )
            })
            .collect();

        let mut resolved_tables_vec: Vec<String> = resolved_tables.into_iter().collect();
        resolved_tables_vec.sort();

        let context = QueryContext {
            metric_definitions: metric_defs,
            dimension_definitions: dim_defs,
            join_paths: join_path
                .iter()
                .map(|(a, b, path)| {
                    (
                        a.clone(),
                        b.clone(),
                        JoinPath {
                            path: path.clone(),
                            join_type: "INNER".to_string(),
                        },
                    )
                })
                .collect(),
            schema_description: schema_lines.join("\n"),
            compile_failure_reason: Some(compile_error.to_string()),
        };

        RawSchemaTranslation {
            context,
            resolved_metrics,
            resolved_tables: resolved_tables_vec,
            resolved_filters,
            join_path,
        }
    }
}
