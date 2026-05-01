//! [`HybridCatalog`] — semantic layer + raw schema fallback.
//!
//! # Effective mode detection
//!
//! | `semantic` | `schema` tables | Effective mode |
//! |---|---|---|
//! | `Some(_)` with views | non-empty | Full hybrid |
//! | `Some(_)` with views | empty | Semantic-only |
//! | `None` | non-empty | Schema-only |
//! | `None` | empty | Empty catalog |
//!
//! # Routing logic
//!
//! - `search_catalog` / `list_metrics` (internal): semantic results take
//!   priority (by name); schema fills tables/columns not covered by a view.
//! - `list_dimensions` (internal), `get_metric_definition`,
//!   `get_valid_dimensions`, `get_column_range`, `get_join_path`: semantic
//!   first, schema fallback.
//! - `try_compile`: try semantic. If `TooComplex` → return `TooComplex`.
//!   If `Unresolvable*` → check schema; if schema covers it return `TooComplex`
//!   (LLM can handle with schema context), else propagate `Unresolvable*`.
//! - `get_context`: merge semantic definitions with schema FK/table context.
//!
//! # Construction
//!
//! ```ignore
//! // Full hybrid
//! let catalog = HybridCatalog::new(Some(semantic), schema);
//!
//! // Schema-only (semantic = None)
//! let catalog = HybridCatalog::new(None, schema);
//!
//! // Load from filesystem
//! let catalog = HybridCatalog::from_path(Some(Path::new("semantics/")), schema)?;
//! ```

use std::collections::HashSet;
use std::path::Path;

use crate::catalog::{
    Catalog, CatalogError, ColumnRange, DimensionSummary, JoinPath, MetricDef, MetricSummary,
    QueryContext, SampleTarget, SchemaCatalog,
};
use crate::semantic::SemanticCatalog;
use crate::types::AnalyticsIntent;

/// Unified analytics catalog combining an optional semantic layer with a
/// raw schema fallback.
///
/// This is the primary runtime type wired into [`AnalyticsDomain`].
///
/// [`AnalyticsDomain`]: super::types::AnalyticsDomain
#[derive(Debug, Default)]
pub struct HybridCatalog {
    /// Optional semantic layer (Oxy views/topics).  When `None` the catalog
    /// behaves identically to a pure [`SchemaCatalog`].
    pub semantic: Option<SemanticCatalog>,
    /// Raw database schema.  Always required — may be empty when the semantic
    /// layer covers all tables.
    pub schema: SchemaCatalog,
}

impl HybridCatalog {
    /// Construct from pre-built components.
    pub fn new(semantic: Option<SemanticCatalog>, schema: SchemaCatalog) -> Self {
        Self { semantic, schema }
    }

    /// Construct by loading the semantic layer from the filesystem.
    ///
    /// ```ignore
    /// let dialects = airlayer::DatasourceDialectMap::with_default(airlayer::Dialect::DuckDB);
    /// let catalog = HybridCatalog::from_path(Some(Path::new("semantics/")), schema, dialects)?;
    /// ```
    pub fn from_path(
        semantic_path: Option<&Path>,
        schema: SchemaCatalog,
        dialects: airlayer::DatasourceDialectMap,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let semantic = semantic_path
            .map(|p| SemanticCatalog::load(p, dialects))
            .transpose()?;
        Ok(Self { semantic, schema })
    }

    /// Prompt-ready schema description combining both sources.
    ///
    /// Used by the Clarify stage to give the LLM context about the catalog.
    pub fn to_prompt_string(&self) -> String {
        match &self.semantic {
            Some(sem) if !sem.table_names().is_empty() => {
                let sem_desc = sem
                    .get_context(&AnalyticsIntent {
                        raw_question: String::new(),
                        summary: String::new(),
                        question_type: crate::types::QuestionType::SingleValue,
                        metrics: vec![],
                        dimensions: vec![],
                        filters: vec![],
                        history: vec![],
                        spec_hint: None,
                        selected_procedure: None,
                        semantic_query: Default::default(),
                        semantic_confidence: 0.0,
                    })
                    .schema_description;

                if self.schema.table_names().is_empty() {
                    sem_desc
                } else {
                    format!(
                        "Semantic views:\n{sem_desc}\n\nRaw schema (fallback for uncovered tables):\n{}",
                        self.schema.to_prompt_string()
                    )
                }
            }
            _ => self.schema.to_prompt_string(),
        }
    }

    /// Table-level summary for decompose: names, column counts, joins.
    ///
    /// Combines semantic view names with schema table names but omits
    /// column-level detail to keep decompose prompts compact.
    pub fn to_table_summary(&self) -> String {
        match &self.semantic {
            Some(sem) if !sem.table_names().is_empty() => {
                let sem_names = sem.table_names();
                let schema_names = self.schema.table_names();
                let mut lines = vec![];
                if !sem_names.is_empty() {
                    lines.push(format!(
                        "Semantic views ({}): {}",
                        sem_names.len(),
                        sem_names.join(", ")
                    ));
                }
                if !schema_names.is_empty() {
                    // Delegate to schema's table summary for column counts + joins.
                    lines.push(format!("\nRaw schema:\n{}", self.schema.to_table_summary()));
                }
                lines.join("\n")
            }
            _ => self.schema.to_table_summary(),
        }
    }

    /// Whether `metric` is defined as a measure in the semantic layer.
    fn metric_in_semantic(&self, metric: &str) -> bool {
        self.semantic
            .as_ref()
            .map_or(false, |s| s.get_metric_definition(metric).is_some())
    }

    // ── Validation helpers (semantic-aware delegation) ───────────────────────

    /// Return `true` if `table` exists in the schema catalog **or** is a
    /// semantic view table/name.
    pub fn table_exists(&self, table: &str) -> bool {
        if self.schema.table_exists(table) {
            return true;
        }
        self.semantic.as_ref().map_or(false, |sem| {
            sem.table_names()
                .iter()
                .any(|t| t.eq_ignore_ascii_case(table))
        })
    }

    /// Return `true` if `column` exists in `table` in the schema catalog,
    /// **or** if `table` is a semantic view with a matching dimension or
    /// measure name.
    pub fn column_exists(&self, table: &str, column: &str) -> bool {
        if self.schema.column_exists(table, column) {
            return true;
        }
        self.semantic_field_exists(table, column)
    }

    /// Return all table names (schema + semantic) that contain `column`.
    pub fn column_tables(&self, column: &str) -> Vec<String> {
        let mut tables: Vec<String> = self
            .schema
            .column_tables(column)
            .into_iter()
            .map(str::to_string)
            .collect();
        if let Some(sem) = &self.semantic {
            let col_lc = column.to_lowercase();
            for view in sem.table_names() {
                if !tables.iter().any(|t| t.eq_ignore_ascii_case(&view))
                    && self.semantic_field_exists(&view, &col_lc)
                {
                    tables.push(view);
                }
            }
        }
        tables
    }

    /// Return all column names for `table` (schema columns + semantic
    /// dimension/measure names).
    pub fn columns_of(&self, table: &str) -> Vec<String> {
        let mut cols = self.schema.columns_of(table);
        if let Some(sem) = &self.semantic {
            for name in sem.table_names() {
                if name.eq_ignore_ascii_case(table) {
                    // Append semantic measure and dimension names.
                    for m in sem.list_metrics("") {
                        if !cols.iter().any(|c| c.eq_ignore_ascii_case(&m.name)) {
                            cols.push(m.name);
                        }
                    }
                    for d in sem.list_dimensions(&name) {
                        if !cols.iter().any(|c| c.eq_ignore_ascii_case(&d.name)) {
                            cols.push(d.name);
                        }
                    }
                    break;
                }
            }
        }
        cols.sort();
        cols
    }

    /// Return the registered join key between two tables (schema only).
    pub fn join_key(&self, a: &str, b: &str) -> Option<String> {
        self.schema.join_key(a, b).map(str::to_string)
    }

    /// Return `true` if `metric` is recognized by the semantic layer —
    /// either as a bare measure name or a SQL expression containing
    /// semantic table.column references.
    pub fn metric_resolves_in_semantic(&self, metric: &str) -> bool {
        let sem = match &self.semantic {
            Some(s) => s,
            None => return false,
        };
        // Bare measure name lookup (e.g. "revenue", "order_count").
        if sem.get_metric_definition(metric).is_some() {
            return true;
        }
        // SQL expression with table.column refs — check if the table is a
        // semantic view and the column is a known measure/dimension.
        if metric.contains('(') {
            let refs = super::validation::extract_table_column_refs(metric);
            if !refs.is_empty() {
                return refs.iter().all(|(t, c)| self.semantic_field_exists(t, c));
            }
        }
        // Dotted "view.measure" format.
        if let Some(pos) = metric.find('.') {
            let (view_part, field_part) = (&metric[..pos], &metric[pos + 1..]);
            return self.semantic_field_exists(view_part, field_part);
        }
        false
    }

    /// Check whether a semantic view named `table` has a dimension or
    /// measure named `column`.
    fn semantic_field_exists(&self, table: &str, column: &str) -> bool {
        let sem = match &self.semantic {
            Some(s) => s,
            None => return false,
        };
        let col_lc = column.to_lowercase();
        // Check if the table matches a view name and the column matches a
        // dimension or measure in that view.
        for view_name in sem.table_names() {
            if view_name.eq_ignore_ascii_case(table) {
                // Check dimensions.
                for d in sem.list_dimensions(&view_name) {
                    if d.name.to_lowercase() == col_lc {
                        return true;
                    }
                }
                // Check measures.
                for m in sem.list_metrics("") {
                    if m.name.to_lowercase() == col_lc {
                        return true;
                    }
                }
                // Also check via get_metric_definition with view-qualified name.
                if sem
                    .get_metric_definition(&format!("{view_name}.{column}"))
                    .is_some()
                {
                    return true;
                }
                return false;
            }
        }
        // Also check if `table` is the underlying table name of a semantic view.
        use crate::catalog::Catalog;
        if let Some(def) = sem.get_metric_definition(column) {
            if def.table.eq_ignore_ascii_case(table) {
                return true;
            }
        }
        false
    }

    /// Return `true` when a semantic join path exists between two views/tables.
    pub fn join_exists_in_semantic(&self, left: &str, right: &str) -> bool {
        use crate::catalog::Catalog;
        self.semantic
            .as_ref()
            .and_then(|s| s.get_join_path(left, right))
            .is_some()
    }
}

impl Catalog for HybridCatalog {
    fn list_metrics(&self, query: &str) -> Vec<MetricSummary> {
        // Start with semantic metrics (highest priority).
        let mut results = match &self.semantic {
            Some(sem) => sem.list_metrics(query),
            None => vec![],
        };

        // Collect semantic measure names for deduplication.
        let sem_names: HashSet<String> = results.iter().map(|m| m.name.clone()).collect();

        // Append schema metrics whose base column name doesn't already appear
        // in the semantic layer.
        for m in self.schema.list_metrics(query) {
            let base = m.name.split('.').last().unwrap_or(&m.name);
            let already_covered = sem_names
                .iter()
                .any(|n| n == base || n.ends_with(&format!(".{base}")) || n == &m.name);
            if !already_covered {
                results.push(m);
            }
        }
        results
    }

    fn list_dimensions(&self, metric: &str) -> Vec<DimensionSummary> {
        if self.metric_in_semantic(metric) {
            self.semantic.as_ref().unwrap().list_dimensions(metric)
        } else {
            self.schema.list_dimensions(metric)
        }
    }

    fn get_metric_definition(&self, metric: &str) -> Option<MetricDef> {
        self.semantic
            .as_ref()
            .and_then(|s| s.get_metric_definition(metric))
            .or_else(|| self.schema.get_metric_definition(metric))
    }

    fn get_valid_dimensions(&self, metric: &str) -> Vec<DimensionSummary> {
        if self.metric_in_semantic(metric) {
            self.semantic.as_ref().unwrap().get_valid_dimensions(metric)
        } else {
            self.schema.get_valid_dimensions(metric)
        }
    }

    fn get_column_range(&self, dimension: &str) -> Option<ColumnRange> {
        // Semantic layer has sample values; schema has type info only.
        self.semantic
            .as_ref()
            .and_then(|s| s.get_column_range(dimension))
            .or_else(|| self.schema.get_column_range(dimension))
    }

    fn get_join_path(&self, from: &str, to: &str) -> Option<JoinPath> {
        self.semantic
            .as_ref()
            .and_then(|s| s.get_join_path(from, to))
            .or_else(|| self.schema.get_join_path(from, to))
    }

    fn resolve_sample_target(&self, table: &str, column: &str) -> Option<SampleTarget> {
        self.semantic
            .as_ref()
            .and_then(|s| s.resolve_sample_target(table, column))
    }

    fn try_compile(&self, intent: &AnalyticsIntent) -> Result<String, CatalogError> {
        match &self.semantic {
            None => {
                tracing::debug!("no semantic catalog, returning TooComplex");
                Err(CatalogError::TooComplex(
                    "no semantic catalog available".into(),
                ))
            }
            Some(sem) => match sem.try_compile(intent) {
                Ok(sql) => Ok(sql),
                // Semantic is capable in principle but this specific query is too
                // complex — pass through unchanged.
                Err(CatalogError::TooComplex(reason)) => Err(CatalogError::TooComplex(reason)),
                // Semantic could not resolve a metric — check schema fallback.
                Err(CatalogError::UnresolvableMetric(m)) => {
                    if self.schema.get_metric_definition(&m).is_some() {
                        // Schema covers it; LLM can generate SQL from schema context.
                        Err(CatalogError::TooComplex(
                            "metric unresolvable in semantic layer but covered by schema".into(),
                        ))
                    } else {
                        // Neither catalog knows this metric.
                        Err(CatalogError::UnresolvableMetric(m))
                    }
                }
                // Semantic could not resolve a dimension — check schema fallback.
                Err(CatalogError::UnresolvableDimension(d)) => {
                    if !self.schema.list_dimensions(&d).is_empty() {
                        Err(CatalogError::TooComplex(
                            "dimension unresolvable in semantic layer but covered by schema".into(),
                        ))
                    } else {
                        Err(CatalogError::UnresolvableDimension(d))
                    }
                }
            },
        }
    }

    fn get_context(&self, intent: &AnalyticsIntent) -> QueryContext {
        match &self.semantic {
            None => self.schema.get_context(intent),
            Some(sem) => {
                let mut ctx = sem.get_context(intent);

                // Supplement with raw schema context for tables not covered by
                // the semantic layer.
                if !self.schema.table_names().is_empty() {
                    let schema_ctx = self.schema.get_context(intent);

                    if !schema_ctx.schema_description.is_empty() {
                        ctx.schema_description = format!(
                            "{}\n\nRaw schema (fallback):\n{}",
                            ctx.schema_description, schema_ctx.schema_description
                        );
                    }

                    // Merge join paths from schema (avoid duplicates).
                    for (a, b, jp) in schema_ctx.join_paths {
                        let already = ctx
                            .join_paths
                            .iter()
                            .any(|(ca, cb, _)| (*ca == a && *cb == b) || (*ca == b && *cb == a));
                        if !already {
                            ctx.join_paths.push((a, b, jp));
                        }
                    }
                }
                ctx
            }
        }
    }

    fn table_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.schema.table_names();

        if let Some(sem) = &self.semantic {
            for name in sem.table_names() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }

        names.sort();
        names
    }

    fn connector_for_table(&self, table: &str) -> Option<&str> {
        self.semantic
            .as_ref()
            .and_then(|s| s.connector_for_table(table))
            .or_else(|| self.schema.connector_for_table(table))
    }
}
