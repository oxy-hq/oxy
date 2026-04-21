//! `impl Catalog for SchemaCatalog` — tool-facing catalog API over raw schemas.

use crate::types::AnalyticsIntent;

use super::helpers::{is_id_col, is_metric_col, type_hint};
use super::schema::SchemaCatalog;
use super::traits::Catalog;
use super::types::{
    CatalogError, ColumnRange, DimensionSummary, JoinPath, MetricDef, MetricSummary, QueryContext,
};

impl Catalog for SchemaCatalog {
    fn list_metrics(&self, query: &str) -> Vec<MetricSummary> {
        let q = query.to_lowercase();
        let mut results = Vec::new();
        let mut tables: Vec<(&String, &Vec<String>)> = self.tables().iter().collect();
        tables.sort_by_key(|(k, _)| k.as_str());
        for (table, cols) in tables {
            let mut sorted = cols.clone();
            sorted.sort();
            for col in sorted {
                // Skip columns that look like IDs / foreign keys.
                if is_id_col(&col) {
                    continue;
                }
                // Determine if this column is numeric:
                // 1. Check column_stats (actual DB type) — most reliable.
                // 2. Fall back to column-name keyword heuristic.
                let stats_key = format!("{}.{}", table, col.to_lowercase());
                let is_numeric = self
                    .column_stats()
                    .get(&stats_key)
                    .map(|r| r.data_type == "number")
                    .unwrap_or_else(|| is_metric_col(&col));
                if !is_numeric {
                    continue;
                }
                let full = format!("{table}.{col}");
                // Case-insensitive query match against the original-case names.
                if q.is_empty()
                    || full.to_lowercase().contains(&q)
                    || col.to_lowercase().contains(&q)
                {
                    results.push(MetricSummary {
                        name: full.clone(),
                        description: format!("Numeric column `{col}` in table `{table}`"),
                        metric_type: String::new(),
                        expr: None,
                    });
                }
            }
        }
        results
    }

    fn list_dimensions(&self, metric: &str) -> Vec<DimensionSummary> {
        let matches = self.metric_matches(metric);
        if matches.is_empty() {
            return vec![];
        }
        let (primary_table, _) = &matches[0];
        let reachable = self.reachable_from(primary_table);

        let mut dims = Vec::new();
        for t in &reachable {
            if let Some(cols) = self.tables().get(t) {
                let mut sorted = cols.clone();
                sorted.sort();
                for col in sorted {
                    // Use column_stats type when available, else keyword heuristic.
                    let stats_key = format!("{}.{}", t, col.to_lowercase());
                    let is_numeric = self
                        .column_stats()
                        .get(&stats_key)
                        .map(|r| r.data_type == "number")
                        .unwrap_or_else(|| is_metric_col(&col));
                    if is_numeric && !is_id_col(&col) {
                        continue; // metrics are not dimensions
                    }
                    let data_type = self
                        .column_stats()
                        .get(&stats_key)
                        .map(|r| r.data_type.clone())
                        .unwrap_or_else(|| type_hint(&col).to_string());
                    dims.push(DimensionSummary {
                        name: format!("{t}.{col}"),
                        description: format!("Column `{col}` in table `{t}`"),
                        data_type,
                    });
                }
            }
        }
        dims
    }

    fn get_metric_definition(&self, metric: &str) -> Option<MetricDef> {
        self.metric_matches(metric)
            .into_iter()
            .next()
            .map(|(table, col)| MetricDef {
                name: col.clone(),
                expr: format!("{table}.{col}"),
                metric_type: "column".to_string(),
                data_source: self.table_connector().get(&table).cloned(),
                table,
                description: None,
            })
    }

    fn get_valid_dimensions(&self, metric: &str) -> Vec<DimensionSummary> {
        // Return only dimensions from the metric's own table (not FK-joined
        // tables).  This keeps token usage low — callers can use
        // `list_dimensions` via `search_catalog` for the full set.
        let table = metric.split('.').next().unwrap_or("").to_lowercase();
        if let Some(cols) = self.tables().get(&table) {
            cols.iter()
                .filter(|c| {
                    if is_id_col(c) {
                        return false;
                    }
                    let stats_key = format!("{}.{}", table, c.to_lowercase());
                    let is_numeric = self
                        .column_stats()
                        .get(&stats_key)
                        .map(|r| r.data_type == "number")
                        .unwrap_or_else(|| is_metric_col(c));
                    !is_numeric
                })
                .map(|c| DimensionSummary {
                    name: format!("{table}.{c}"),
                    description: String::new(),
                    data_type: type_hint(c).to_string(),
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn get_column_range(&self, dimension: &str) -> Option<ColumnRange> {
        let col = dimension
            .split('.')
            .next_back()
            .unwrap_or(dimension)
            .to_lowercase();

        // Try a fully-qualified "table.column" cache lookup first.
        if let Some(range) = self.column_stats().get(&dimension.to_lowercase()) {
            return Some(range.clone());
        }

        // Fall back to an unqualified column name scan across all tables.
        let found_in: Vec<&str> = self
            .tables()
            .iter()
            .filter(|(_, cols)| cols.iter().any(|c| c.to_lowercase() == col))
            .map(|(t, _)| t.as_str())
            .collect();

        if found_in.is_empty() {
            return None;
        }

        // Check whether any of those tables have stats cached.
        for table in &found_in {
            let key = format!("{table}.{col}");
            if let Some(range) = self.column_stats().get(&key) {
                return Some(range.clone());
            }
        }

        // Column exists but no statistics were gathered yet — return type-only placeholder.
        Some(ColumnRange {
            min: None,
            max: None,
            sample_values: vec![],
            data_type: type_hint(&col).to_string(),
        })
    }

    fn get_join_path(&self, from: &str, to: &str) -> Option<JoinPath> {
        self.join_key(from, to).map(|key| JoinPath {
            path: format!("{from} JOIN {to} ON {key}"),
            join_type: "INNER".to_string(),
        })
    }

    /// Schema-only catalog always returns [`CatalogError::TooComplex`].
    ///
    /// Without pre-defined business logic every query requires LLM reasoning.
    fn try_compile(&self, _intent: &AnalyticsIntent) -> Result<String, CatalogError> {
        Err(CatalogError::TooComplex(
            "schema-only catalog does not support direct compilation".into(),
        ))
    }

    fn get_context(&self, intent: &AnalyticsIntent) -> QueryContext {
        // Collect tables relevant to the stated metrics.
        let mut relevant: Vec<String> = Vec::new();
        for metric in &intent.metrics {
            for (t, _) in self.metric_matches(metric) {
                if !relevant.contains(&t) {
                    for linked in self.reachable_from(&t) {
                        if !relevant.contains(&linked) {
                            relevant.push(linked);
                        }
                    }
                }
            }
        }

        let metric_defs = intent
            .metrics
            .iter()
            .filter_map(|m| self.get_metric_definition(m))
            .collect();

        let dim_defs = intent
            .dimensions
            .iter()
            .flat_map(|d| self.get_valid_dimensions(d))
            .collect();

        let mut join_paths = Vec::new();
        let mut joins: Vec<(&(String, String), &String)> = self.join_keys().iter().collect();
        joins.sort_by_key(|((a, b), _)| (a.as_str(), b.as_str()));
        for ((a, b), key) in joins {
            if relevant.contains(a) || relevant.contains(b) {
                join_paths.push((
                    a.clone(),
                    b.clone(),
                    JoinPath {
                        path: format!("{a} JOIN {b} ON {key}"),
                        join_type: "INNER".to_string(),
                    },
                ));
            }
        }

        QueryContext {
            metric_definitions: metric_defs,
            dimension_definitions: dim_defs,
            join_paths,
            schema_description: self.to_prompt_string(),
            compile_failure_reason: Some(
                "Schema-only catalog: all queries require LLM reasoning".to_string(),
            ),
        }
    }

    fn table_names(&self) -> Vec<String> {
        SchemaCatalog::table_names(self)
    }

    fn connector_for_table(&self, table: &str) -> Option<&str> {
        self.table_connector()
            .get(&table.to_lowercase())
            .map(|s| s.as_str())
    }
}
