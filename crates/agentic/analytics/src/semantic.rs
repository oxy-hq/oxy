//! [`SemanticCatalog`] — Oxy semantic layer backed by [`airlayer::SemanticEngine`].
//!
//! All catalog operations (search, metric definitions, join paths, and query
//! compilation) delegate to airlayer's data structures.  YAML parsing goes
//! through a thin shim in [`crate::airlayer_compat`] that handles minor format
//! differences between oxy's `.view.yml` files and airlayer's strict schema.
//!
//! # Oxy YAML format
//!
//! **views/<name>.view.yml**
//! ```yaml
//! name: orders_view
//! description: Order analytics
//! table: orders          # OR: sql: "SELECT …"
//! entities:
//!   - name: order_id
//!     type: primary
//!     key: order_id
//!   - name: customer_id
//!     type: foreign
//!     key: customer_id
//! dimensions:
//!   - name: status
//!     type: string
//!     expr: status
//!     description: Order status
//!     samples: [completed, pending, cancelled]
//!   - name: order_date
//!     type: date
//!     expr: order_date
//! measures:
//!   - name: revenue
//!     type: sum
//!     expr: amount
//!     description: Total revenue
//!   - name: order_count
//!     type: count
//! ```
//!
//! **topics/<name>.topic.yml**
//! ```yaml
//! name: sales_analytics
//! description: Sales analytics domain
//! views:
//!   - orders_view
//!   - customers_view
//! ```

use std::path::{Path, PathBuf};

use crate::airlayer_compat;
use crate::catalog::{
    Catalog, CatalogError, ColumnRange, DimensionSummary, JoinPath, MetricDef, MetricSummary,
    QueryContext, SampleTarget,
};
use crate::types::AnalyticsIntent;

// ── SemanticCatalog ───────────────────────────────────────────────────────────

/// Semantic layer catalog backed by [`airlayer::SemanticEngine`].
///
/// # Behavior
///
/// - **`search_catalog`**: batch-search metrics and dimensions across all views.
/// - **`list_metrics`**: searches measure names and descriptions across all views.
/// - **`list_dimensions`**: returns dimensions from the metric's view plus views
///   joinable via entity relationships.
/// - **`try_compile`**: delegates to `engine.compile_query()` — supports multi-hop
///   joins, CTE fan-out protection, and dialect-aware SQL generation.  Returns
///   [`CatalogError::TooComplex`] for queries that airlayer cannot compile.
pub struct SemanticCatalog {
    engine: airlayer::SemanticEngine,
}

impl std::fmt::Debug for SemanticCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticCatalog")
            .field("views", &self.engine.views().len())
            .finish()
    }
}

impl SemanticCatalog {
    /// Create an empty catalog with no views.
    ///
    /// Useful for the "no semantic layer" case — the LLM relies on database
    /// lookup tools instead.
    pub fn empty() -> Self {
        let layer = airlayer::SemanticLayer::new(vec![], None);
        let dialects = airlayer::DatasourceDialectMap::new();
        let engine = airlayer::SemanticEngine::from_semantic_layer(layer, dialects)
            .expect("empty semantic layer should always be valid");
        Self { engine }
    }

    /// Return `true` when this catalog has no views.
    pub fn is_empty(&self) -> bool {
        self.engine.views().is_empty()
    }

    /// Wrap a pre-built engine (useful for testing).
    pub fn from_engine(engine: airlayer::SemanticEngine) -> Self {
        Self { engine }
    }

    /// Load from a `semantics/` directory containing `views/` and `topics/`
    /// subdirectories.
    pub fn load(
        semantics_dir: &Path,
        dialects: airlayer::DatasourceDialectMap,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let views_dir = semantics_dir.join("views");
        let topics_dir = semantics_dir.join("topics");

        let topics_path = if topics_dir.exists() {
            Some(topics_dir.as_path())
        } else {
            None
        };

        let engine = airlayer::SemanticEngine::load(&views_dir, topics_path, dialects).map_err(
            |e| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            },
        )?;

        Ok(Self { engine })
    }

    /// Load from an explicit list of `.view.yml` and `.topic.yml` paths.
    ///
    /// Unlike [`load`], this does not assume a directory structure — each path
    /// is classified by its filename suffix and loaded directly.
    ///
    /// [`load`]: SemanticCatalog::load
    pub fn load_files(
        paths: &[PathBuf],
        dialects: airlayer::DatasourceDialectMap,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut views = Vec::new();
        let mut topics = Vec::new();

        for path in paths {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let content = std::fs::read_to_string(path)?;
            if name.ends_with(".view.yml") || name.ends_with(".view.yaml") {
                views.push(airlayer_compat::parse_view_yaml(&content)?);
            } else if name.ends_with(".topic.yml") || name.ends_with(".topic.yaml") {
                topics.push(airlayer_compat::parse_topic_yaml(&content)?);
            }
            // Other suffixes silently ignored.
        }

        let topic_opt = if topics.is_empty() {
            None
        } else {
            Some(topics)
        };

        let layer = airlayer::SemanticLayer::new(views, topic_opt);
        let engine = airlayer::SemanticEngine::from_semantic_layer(layer, dialects).map_err(
            |e| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            },
        )?;

        Ok(Self { engine })
    }

    // ── Validation helpers (used by validation rules) ────────────────────────

    /// Return `true` if `table` matches a view name (case-insensitive).
    pub fn table_exists(&self, table: &str) -> bool {
        self.engine
            .views()
            .iter()
            .any(|v| v.name.eq_ignore_ascii_case(table))
    }

    /// Return `true` if `column` is a dimension or measure of the view named
    /// `table` (case-insensitive match on both view name and field name).
    pub fn column_exists(&self, table: &str, column: &str) -> bool {
        self.field_exists(table, column)
    }

    /// Return all view names that contain a dimension or measure named `column`.
    pub fn column_tables(&self, column: &str) -> Vec<String> {
        let col_lc = column.to_lowercase();
        self.engine
            .views()
            .iter()
            .filter(|v| {
                v.dimensions.iter().any(|d| d.name.to_lowercase() == col_lc)
                    || v.measures_list()
                        .iter()
                        .any(|m| m.name.to_lowercase() == col_lc)
            })
            .map(|v| v.name.clone())
            .collect()
    }

    /// Return all dimension + measure names for the view named `table`.
    pub fn columns_of(&self, table: &str) -> Vec<String> {
        let Some(view) = self
            .engine
            .views()
            .iter()
            .find(|v| v.name.eq_ignore_ascii_case(table))
        else {
            return vec![];
        };
        let mut cols: Vec<String> = view.dimensions.iter().map(|d| d.name.clone()).collect();
        for m in view.measures_list() {
            if !cols.iter().any(|c| c.eq_ignore_ascii_case(&m.name)) {
                cols.push(m.name.clone());
            }
        }
        cols.sort();
        cols
    }

    /// Return the join key between two views, if an entity relationship exists.
    pub fn join_key(&self, a: &str, b: &str) -> Option<String> {
        use crate::catalog::Catalog;
        self.get_join_path(a, b).map(|jp| {
            // Extract the key from the path string "A JOIN B ON A.key = B.key".
            jp.path
                .split("ON ")
                .nth(1)
                .and_then(|s| s.split('=').next())
                .and_then(|s| s.split('.').nth(1))
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        })
    }

    /// Return `true` if `metric` is recognized as a measure, a SQL expression
    /// containing view.column refs, or a dotted "view.measure" name.
    pub fn metric_resolves_in_semantic(&self, metric: &str) -> bool {
        // Bare measure name lookup.
        use crate::catalog::Catalog;
        if self.get_metric_definition(metric).is_some() {
            return true;
        }
        // SQL expression with table.column refs.
        if metric.contains('(') {
            let refs = super::validation::extract_table_column_refs(metric);
            if !refs.is_empty() {
                return refs.iter().all(|(t, c)| self.field_exists(t, c));
            }
        }
        // Dotted "view.measure" format.
        if let Some(pos) = metric.find('.') {
            let (view_part, field_part) = (&metric[..pos], &metric[pos + 1..]);
            return self.field_exists(view_part, field_part);
        }
        false
    }

    /// Return `true` when a join path exists between two views via entities.
    pub fn join_exists_in_semantic(&self, left: &str, right: &str) -> bool {
        use crate::catalog::Catalog;
        self.get_join_path(left, right).is_some()
    }

    /// Prompt-ready description of the semantic layer views.
    pub fn to_prompt_string(&self) -> String {
        if self.engine.views().is_empty() {
            return "No semantic layer configured. Use list_tables and describe_table tools \
                    to discover database schema."
                .to_string();
        }
        use crate::catalog::Catalog;
        let dummy = crate::types::AnalyticsIntent {
            raw_question: String::new(),
            question_type: crate::types::QuestionType::SingleValue,
            metrics: vec![],
            dimensions: vec![],
            filters: vec![],
            history: vec![],
            spec_hint: None,
            selected_procedure: None,
        };
        self.get_context(&dummy).schema_description
    }

    /// Compact table-level summary for decompose prompts.
    pub fn to_table_summary(&self) -> String {
        let names = self.table_names();
        if names.is_empty() {
            return "No semantic views.".to_string();
        }
        format!("Semantic views ({}): {}", names.len(), names.join(", "))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Check whether a view named `table` has a dimension or measure named
    /// `column` (case-insensitive on both).
    fn field_exists(&self, table: &str, column: &str) -> bool {
        let col_lc = column.to_lowercase();
        for view in self.engine.views() {
            if view.name.eq_ignore_ascii_case(table) {
                if view
                    .dimensions
                    .iter()
                    .any(|d| d.name.to_lowercase() == col_lc)
                {
                    return true;
                }
                if view
                    .measures_list()
                    .iter()
                    .any(|m| m.name.to_lowercase() == col_lc)
                {
                    return true;
                }
                // Also check view-qualified name.
                use crate::catalog::Catalog;
                if self
                    .get_metric_definition(&format!("{}.{}", view.name, column))
                    .is_some()
                {
                    return true;
                }
                return false;
            }
        }
        // Check if `table` is the underlying table name of a view.
        use crate::catalog::Catalog;
        if let Some(def) = self.get_metric_definition(column) {
            if def.table.eq_ignore_ascii_case(table) {
                return true;
            }
        }
        false
    }

    // ── Private lookup helpers ────────────────────────────────────────────────

    /// Find the view and measure definition for `metric`.
    ///
    /// Accepts both bare names (`"revenue"`) and view-qualified names
    /// (`"orders_view.revenue"`).
    fn find_measure<'a>(
        &'a self,
        metric: &str,
    ) -> Option<(&'a airlayer::View, &'a airlayer::Measure)> {
        self.engine.views().iter().find_map(|v| {
            v.measures_list()
                .iter()
                .find(|m| m.name == metric || format!("{}.{}", v.name, m.name) == metric)
                .map(|m| (v, m))
        })
    }

    /// Find the view and dimension definition for `dim`.
    fn find_dimension<'a>(
        &'a self,
        dim: &str,
    ) -> Option<(&'a airlayer::View, &'a airlayer::Dimension)> {
        self.engine.views().iter().find_map(|v| {
            v.dimensions
                .iter()
                .find(|d| d.name == dim || format!("{}.{}", v.name, d.name) == dim)
                .map(|d| (v, d))
        })
    }

    /// Return all views reachable from `start_view` via entity joins.
    ///
    /// A view `B` is joinable from `A` when `A` has a `primary` entity whose
    /// key matches a `foreign` entity key in `B`.
    fn reachable_views<'a>(&'a self, start: &'a airlayer::View) -> Vec<&'a airlayer::View> {
        let mut reachable = vec![start];
        let primary_keys: Vec<String> = start
            .entities
            .iter()
            .filter(|e| e.entity_type == airlayer::schema::models::EntityType::Primary)
            .flat_map(|e| e.get_keys())
            .collect();

        for view in self.engine.views() {
            if view.name == start.name {
                continue;
            }
            let joinable = view.entities.iter().any(|e| {
                e.entity_type == airlayer::schema::models::EntityType::Foreign
                    && e.get_keys().iter().any(|k| primary_keys.contains(k))
            });
            if joinable && !reachable.iter().any(|r| r.name == view.name) {
                reachable.push(view);
            }
        }
        reachable
    }

    /// Qualify bare metric/dimension names to `ViewName.field` format.
    ///
    /// When `preferred_views` is non-empty, fields that exist in a preferred
    /// view are resolved there first.  This is critical for dimensions like
    /// `"date"` that appear in every view — without a hint the resolver would
    /// pick whichever view comes first in the list, which may differ from the
    /// metrics' view and cause an unjoinable cross-view query.
    ///
    /// Resolution tiers (tried in order, preferred views first at each tier):
    /// 1. Exact name match (case-sensitive)
    /// 2. Case-insensitive match
    /// 3. Fuzzy match (Jaro-Winkler ≥ 0.8)
    ///
    /// Returns `None` if any name cannot be resolved.
    fn qualify_names(
        &self,
        names: &[String],
        is_metric: bool,
        preferred_views: &[String],
    ) -> Option<Vec<String>> {
        names
            .iter()
            .map(|name| {
                // If already dot-qualified AND matches a real view.field, accept as-is.
                if name.contains('.') {
                    let is_known = self.engine.views().iter().any(|v| {
                        let prefix = format!("{}.", v.name);
                        if let Some(field) = name.strip_prefix(&prefix) {
                            if is_metric {
                                v.measures_list().iter().any(|m| m.name == field)
                            } else {
                                v.dimensions.iter().any(|d| d.name == field)
                            }
                        } else {
                            false
                        }
                    });
                    if is_known {
                        return Some(name.clone());
                    }
                }

                // Strip table prefix if present (LLM may qualify with raw table
                // name, e.g. "cardio_4_4.Max Heart Rate").
                let bare_name = name.find('.').map(|pos| &name[pos + 1..]).unwrap_or(name);

                // Build search order: preferred views first, then the rest.
                let all_views = self.engine.views();
                let ordered: Vec<&airlayer::View> = preferred_views
                    .iter()
                    .filter_map(|pv| all_views.iter().find(|v| &v.name == pv))
                    .chain(
                        all_views
                            .iter()
                            .filter(|v| !preferred_views.contains(&v.name)),
                    )
                    .collect();

                // Tier 1: exact match
                for view in &ordered {
                    if is_metric {
                        if view.measures_list().iter().any(|m| m.name == bare_name) {
                            return Some(format!("{}.{}", view.name, bare_name));
                        }
                    } else if view.dimensions.iter().any(|d| d.name == bare_name) {
                        return Some(format!("{}.{}", view.name, bare_name));
                    }
                }
                // Tier 2: case-insensitive match
                let lower = bare_name.to_lowercase();
                for view in &ordered {
                    if is_metric {
                        if let Some(m) = view
                            .measures_list()
                            .iter()
                            .find(|m| m.name.to_lowercase() == lower)
                        {
                            return Some(format!("{}.{}", view.name, m.name));
                        }
                    } else if let Some(d) = view
                        .dimensions
                        .iter()
                        .find(|d| d.name.to_lowercase() == lower)
                    {
                        return Some(format!("{}.{}", view.name, d.name));
                    }
                }
                // Tier 3: fuzzy match
                let normalized = normalize_for_fuzzy(bare_name);
                let mut best: Option<(f64, String, String)> = None;
                for view in &ordered {
                    if is_metric {
                        for m in view.measures_list() {
                            let score =
                                strsim::jaro_winkler(&normalized, &normalize_for_fuzzy(&m.name));
                            if score >= 0.8 && best.as_ref().map_or(true, |b| score > b.0) {
                                best = Some((score, view.name.clone(), m.name.clone()));
                            }
                        }
                    } else {
                        for d in &view.dimensions {
                            let score =
                                strsim::jaro_winkler(&normalized, &normalize_for_fuzzy(&d.name));
                            if score >= 0.8 && best.as_ref().map_or(true, |b| score > b.0) {
                                best = Some((score, view.name.clone(), d.name.clone()));
                            }
                        }
                    }
                }
                best.map(|(_, view, field)| format!("{view}.{field}"))
            })
            .collect()
    }

    /// Parse raw filter strings from the intent into structured airlayer
    /// [`QueryFilter`](airlayer::engine::query::QueryFilter) objects.
    ///
    /// Supports simple comparison filters like `"date >= '2024-01-01'"` or
    /// `"status = 'active'"`.  The column name is qualified against the
    /// semantic layer using `find_dimension` / `find_measure`, preferring
    /// `preferred_views` (the views already selected for metrics).
    ///
    /// Returns `Err(TooComplex)` when a filter contains SQL functions or
    /// expressions that cannot be represented in airlayer's filter API.
    fn parse_intent_filters(
        &self,
        filters: &[String],
        preferred_views: &[String],
    ) -> Result<Vec<airlayer::engine::query::QueryFilter>, CatalogError> {
        use airlayer::engine::query::{FilterOperator, QueryFilter};

        filters
            .iter()
            .map(|raw| {
                Self::parse_single_filter(raw, |col| {
                    self.qualify_filter_column(col, preferred_views)
                })
            })
            .collect()
    }

    /// Parse a single raw filter string into a [`QueryFilter`].
    ///
    /// Expected format: `<column> <op> <value>` where `<op>` is one of
    /// `=`, `!=`, `<>`, `>=`, `<=`, `>`, `<`, `IN`, `NOT IN`, `IS NULL`,
    /// `IS NOT NULL`, `LIKE`, `NOT LIKE`, `BETWEEN`.
    fn parse_single_filter(
        raw: &str,
        qualify: impl Fn(&str) -> Option<String>,
    ) -> Result<airlayer::engine::query::QueryFilter, CatalogError> {
        use airlayer::engine::query::{FilterOperator, QueryFilter};

        let trimmed = raw.trim();
        let upper = trimmed.to_uppercase();

        // IS NULL / IS NOT NULL
        if upper.ends_with("IS NOT NULL") {
            let col = trimmed[..trimmed.len() - "IS NOT NULL".len()].trim();
            let member = qualify(col)
                .ok_or_else(|| CatalogError::TooComplex("unresolvable filter column".into()))?;
            return Ok(QueryFilter {
                member: Some(member),
                operator: Some(FilterOperator::Set),
                values: vec![],
                and: None,
                or: None,
            });
        }
        if upper.ends_with("IS NULL") {
            let col = trimmed[..trimmed.len() - "IS NULL".len()].trim();
            let member = qualify(col)
                .ok_or_else(|| CatalogError::TooComplex("unresolvable filter column".into()))?;
            return Ok(QueryFilter {
                member: Some(member),
                operator: Some(FilterOperator::NotSet),
                values: vec![],
                and: None,
                or: None,
            });
        }

        // BETWEEN ... AND ...
        if let Some(between_pos) = upper.find(" BETWEEN ") {
            let col = trimmed[..between_pos].trim();
            let rest = trimmed[between_pos + " BETWEEN ".len()..].trim();
            // Split on " AND " (case-insensitive)
            let rest_upper = rest.to_uppercase();
            if let Some(and_pos) = rest_upper.find(" AND ") {
                let lo = Self::strip_quotes(rest[..and_pos].trim());
                let hi = Self::strip_quotes(rest[and_pos + " AND ".len()..].trim());
                // Values containing SQL functions → too complex
                if Self::value_is_expression(&lo) || Self::value_is_expression(&hi) {
                    return Err(CatalogError::TooComplex(
                        "filter value contains SQL expression".into(),
                    ));
                }
                let member = qualify(col)
                    .ok_or_else(|| CatalogError::TooComplex("unresolvable filter column".into()))?;
                return Ok(QueryFilter {
                    member: Some(member),
                    operator: Some(FilterOperator::InDateRange),
                    values: vec![lo, hi],
                    and: None,
                    or: None,
                });
            }
            return Err(CatalogError::TooComplex("malformed BETWEEN filter".into()));
        }

        // Comparison operators (ordered longest-first to avoid prefix conflicts)
        let ops: &[(&str, FilterOperator)] = &[
            (">=", FilterOperator::Gte),
            ("<=", FilterOperator::Lte),
            ("!=", FilterOperator::NotEquals),
            ("<>", FilterOperator::NotEquals),
            (">", FilterOperator::Gt),
            ("<", FilterOperator::Lt),
            ("=", FilterOperator::Equals),
        ];

        for (op_str, op) in ops {
            if let Some(pos) = trimmed.find(op_str) {
                let col = trimmed[..pos].trim();
                let val = Self::strip_quotes(trimmed[pos + op_str.len()..].trim());
                if Self::value_is_expression(&val) {
                    return Err(CatalogError::TooComplex(
                        "filter value contains SQL expression".into(),
                    ));
                }
                let member = qualify(col)
                    .ok_or_else(|| CatalogError::TooComplex("unresolvable filter column".into()))?;
                return Ok(QueryFilter {
                    member: Some(member),
                    operator: Some(op.clone()),
                    values: vec![val],
                    and: None,
                    or: None,
                });
            }
        }

        // Could not parse → too complex for the semantic layer
        Err(CatalogError::TooComplex(format!(
            "unable to parse filter: {}",
            trimmed
        )))
    }

    /// Qualify a bare or dotted column name for use in a filter member.
    ///
    /// Tries `find_dimension` first (filters usually target dimensions),
    /// then `find_measure`.  `preferred_views` biases resolution toward
    /// views already selected for the current query's metrics.
    fn qualify_filter_column(&self, col: &str, preferred_views: &[String]) -> Option<String> {
        // Already qualified (view.column)
        if col.contains('.') {
            // Verify it resolves
            if self.find_dimension(col).is_some() || self.find_measure(col).is_some() {
                return Some(col.to_string());
            }
            return None;
        }

        // Try qualifying via qualify_names (reuses the same preference logic).
        self.qualify_names(&[col.to_string()], false, preferred_views)
            .and_then(|v| v.into_iter().next())
            .or_else(|| {
                // Fallback: try as a measure name
                self.qualify_names(&[col.to_string()], true, preferred_views)
                    .and_then(|v| v.into_iter().next())
            })
    }

    /// Strip surrounding single or double quotes from a value.
    fn strip_quotes(s: &str) -> String {
        let s = s.trim();
        if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }

    /// Return `true` when a filter value looks like a SQL expression rather
    /// than a simple literal (contains parentheses or SQL keywords).
    fn value_is_expression(val: &str) -> bool {
        let u = val.to_uppercase();
        val.contains('(')
            || u.contains("CURRENT_DATE")
            || u.contains("CURRENT_TIMESTAMP")
            || u.contains("NOW(")
            || u.contains("DATE_SUB")
            || u.contains("DATE_ADD")
            || u.contains("INTERVAL")
            || u.contains("SELECT")
    }

    /// Return `true` when any filter expression suggests window/CTE complexity.
    fn filters_are_complex(filters: &[String]) -> bool {
        const COMPLEX: &[&str] = &[
            "OVER",
            "ROW_NUMBER",
            "RANK(",
            "DENSE_RANK",
            "LAG(",
            "LEAD(",
            "WITH ",
            "HAVING",
        ];
        filters.iter().any(|f| {
            let u = f.to_uppercase();
            COMPLEX.iter().any(|kw| u.contains(kw))
        })
    }

    // ── Airlayer-native fallback translation ─────────────────────────────

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
                if let Some(range) = &td.date_range {
                    if range.len() == 2 {
                        resolved_filters.push(format!("{} >= '{}'", col_expr, range[0]));
                        resolved_filters.push(format!("{} < '{}'", col_expr, range[1]));
                    }
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

    /// Expose the underlying airlayer engine for direct compilation.
    pub fn engine(&self) -> &airlayer::SemanticEngine {
        &self.engine
    }
}

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

/// Format a structured filter into a raw SQL WHERE clause fragment.
fn format_filter_as_sql(
    col_expr: &str,
    operator: &airlayer::engine::query::FilterOperator,
    values: &[String],
) -> String {
    use airlayer::engine::query::FilterOperator;
    match operator {
        FilterOperator::Equals if values.len() == 1 => {
            format!("{} = '{}'", col_expr, values[0])
        }
        FilterOperator::Equals => {
            let vals: Vec<String> = values.iter().map(|v| format!("'{v}'")).collect();
            format!("{} IN ({})", col_expr, vals.join(", "))
        }
        FilterOperator::NotEquals if values.len() == 1 => {
            format!("{} != '{}'", col_expr, values[0])
        }
        FilterOperator::NotEquals => {
            let vals: Vec<String> = values.iter().map(|v| format!("'{v}'")).collect();
            format!("{} NOT IN ({})", col_expr, vals.join(", "))
        }
        FilterOperator::Contains if !values.is_empty() => {
            format!("{} LIKE '%{}%'", col_expr, values[0])
        }
        FilterOperator::NotContains if !values.is_empty() => {
            format!("{} NOT LIKE '%{}%'", col_expr, values[0])
        }
        FilterOperator::StartsWith if !values.is_empty() => {
            format!("{} LIKE '{}%'", col_expr, values[0])
        }
        FilterOperator::EndsWith if !values.is_empty() => {
            format!("{} LIKE '%{}'", col_expr, values[0])
        }
        FilterOperator::Gt if !values.is_empty() => {
            format!("{} > '{}'", col_expr, values[0])
        }
        FilterOperator::Gte if !values.is_empty() => {
            format!("{} >= '{}'", col_expr, values[0])
        }
        FilterOperator::Lt if !values.is_empty() => {
            format!("{} < '{}'", col_expr, values[0])
        }
        FilterOperator::Lte if !values.is_empty() => {
            format!("{} <= '{}'", col_expr, values[0])
        }
        FilterOperator::Set => format!("{} IS NOT NULL", col_expr),
        FilterOperator::NotSet => format!("{} IS NULL", col_expr),
        FilterOperator::InDateRange if values.len() == 2 => {
            format!(
                "{} >= '{}' AND {} < '{}'",
                col_expr, values[0], col_expr, values[1]
            )
        }
        FilterOperator::NotInDateRange if values.len() == 2 => {
            format!(
                "({} < '{}' OR {} >= '{}')",
                col_expr, values[0], col_expr, values[1]
            )
        }
        FilterOperator::BeforeDate if !values.is_empty() => {
            format!("{} < '{}'", col_expr, values[0])
        }
        FilterOperator::AfterDate if !values.is_empty() => {
            format!("{} > '{}'", col_expr, values[0])
        }
        FilterOperator::BeforeOrOnDate if !values.is_empty() => {
            format!("{} <= '{}'", col_expr, values[0])
        }
        FilterOperator::AfterOrOnDate if !values.is_empty() => {
            format!("{} >= '{}'", col_expr, values[0])
        }
        // Fallback
        _ => format!(
            "{} = '{}'",
            col_expr,
            values.first().unwrap_or(&String::new())
        ),
    }
}

/// Normalize a name for fuzzy comparison: lowercase, strip underscores/hyphens,
/// collapse whitespace.  `"Max Heart Rate"` and `"max_heart_rate"` both become
/// `"maxheartrate"`.
fn normalize_for_fuzzy(s: &str) -> String {
    s.to_lowercase()
        .replace(['_', '-'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
}

// ── Catalog impl ──────────────────────────────────────────────────────────────

impl Catalog for SemanticCatalog {
    fn list_metrics(&self, query: &str) -> Vec<MetricSummary> {
        let q = query.to_lowercase();
        self.engine
            .views()
            .iter()
            .flat_map(|v| {
                let q2 = q.clone();
                let view_name = v.name.clone();
                v.measures_list()
                    .iter()
                    .filter(move |m| {
                        q2.is_empty()
                            || m.name.to_lowercase().contains(&q2)
                            || m.description
                                .as_deref()
                                .map_or(false, |d| d.to_lowercase().contains(&q2))
                            || view_name.to_lowercase().contains(&q2)
                    })
                    .map(move |m| MetricSummary {
                        // Return qualified view.measure name so downstream stages
                        // can use it directly in airlayer QueryRequests.
                        name: format!("{}.{}", v.name, m.name),
                        description: m.description.clone().unwrap_or_else(|| {
                            format!("{} measure from view `{}`", m.measure_type, v.name)
                        }),
                        metric_type: m.measure_type.to_string(),
                    })
            })
            .collect()
    }

    fn list_dimensions(&self, metric: &str) -> Vec<DimensionSummary> {
        let Some((view, _)) = self.find_measure(metric) else {
            return vec![];
        };
        self.reachable_views(view)
            .into_iter()
            .flat_map(|v| {
                v.dimensions.iter().map(|d| DimensionSummary {
                    // Return qualified view.dimension name for airlayer compatibility.
                    name: format!("{}.{}", v.name, d.name),
                    description: d
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("{} dimension", d.dimension_type)),
                    data_type: d.dimension_type.to_string(),
                })
            })
            .collect()
    }

    fn get_metric_definition(&self, metric: &str) -> Option<MetricDef> {
        let (view, measure) = self.find_measure(metric)?;
        Some(MetricDef {
            // Return qualified view.measure name for airlayer compatibility.
            name: format!("{}.{}", view.name, measure.name),
            expr: measure.expr.clone().unwrap_or_else(|| measure.name.clone()),
            metric_type: measure.measure_type.to_string(),
            table: view.table.clone().unwrap_or_else(|| view.name.clone()),
            description: measure.description.clone(),
            data_source: view.datasource.clone(),
        })
    }

    fn get_valid_dimensions(&self, metric: &str) -> Vec<DimensionSummary> {
        // Return only dimensions from the metric's own view — not joined views.
        let Some((view, _)) = self.find_measure(metric) else {
            return vec![];
        };
        view.dimensions
            .iter()
            .map(|d| DimensionSummary {
                // Return qualified view.dimension name for airlayer compatibility.
                name: format!("{}.{}", view.name, d.name),
                description: d
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("{} dimension", d.dimension_type)),
                data_type: d.dimension_type.to_string(),
            })
            .collect()
    }

    fn get_column_range(&self, dimension: &str) -> Option<ColumnRange> {
        let (_, dim) = self.find_dimension(dimension)?;
        Some(ColumnRange {
            min: None,
            max: None,
            sample_values: dim
                .samples
                .as_ref()
                .map(|s| {
                    s.iter()
                        .map(|v| serde_json::Value::String(v.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            data_type: dim.dimension_type.to_string(),
        })
    }

    fn get_join_path(&self, from: &str, to: &str) -> Option<JoinPath> {
        let from_view = self.engine.view(from)?;
        let to_view = self.engine.view(to)?;

        let pk_keys: Vec<String> = from_view
            .entities
            .iter()
            .filter(|e| e.entity_type == airlayer::schema::models::EntityType::Primary)
            .flat_map(|e| e.get_keys())
            .collect();

        let join_key = to_view.entities.iter().find(|e| {
            e.entity_type == airlayer::schema::models::EntityType::Foreign
                && e.get_keys().iter().any(|k| pk_keys.contains(k))
        })?;

        let key = join_key.get_keys().into_iter().next()?;
        let from_table = from_view.table.as_deref().unwrap_or(&from_view.name);
        let to_table = to_view.table.as_deref().unwrap_or(&to_view.name);

        Some(JoinPath {
            path: format!("{from_table} JOIN {to_table} ON {from_table}.{key} = {to_table}.{key}"),
            join_type: "INNER".to_string(),
        })
    }

    fn resolve_sample_target(&self, table: &str, column: &str) -> Option<SampleTarget> {
        // Find the view matching `table` (by view name or underlying table name).
        let view = self.engine.views().iter().find(|v| {
            v.name.eq_ignore_ascii_case(table)
                || v.table
                    .as_deref()
                    .map_or(false, |t| t.eq_ignore_ascii_case(table))
        })?;

        // Look up `column` as a dimension first, then as a measure.
        let underlying_table = view.table.clone().unwrap_or_else(|| view.name.clone());

        if let Some(dim) = view
            .dimensions
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case(column))
        {
            return Some(SampleTarget {
                table: underlying_table,
                column_expr: dim.expr.clone(),
                static_samples: dim.samples.clone().unwrap_or_default(),
                data_type: Some(dim.dimension_type.to_string()),
            });
        }

        if let Some(measure) = view
            .measures_list()
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(column))
        {
            return Some(SampleTarget {
                table: underlying_table,
                column_expr: measure.expr.clone().unwrap_or_else(|| measure.name.clone()),
                static_samples: measure.samples.clone().unwrap_or_default(),
                data_type: None,
            });
        }

        None
    }

    fn try_compile(&self, intent: &AnalyticsIntent) -> Result<String, CatalogError> {
        if intent.metrics.is_empty() {
            return Err(CatalogError::TooComplex("no metrics in intent".into()));
        }
        if Self::filters_are_complex(&intent.filters) {
            return Err(CatalogError::TooComplex(
                "filters contain SQL functions or complex expressions".into(),
            ));
        }

        // Qualify metrics first (no view preference).
        let measures = self
            .qualify_names(&intent.metrics, true, &[])
            .ok_or_else(|| {
                let bad = intent
                    .metrics
                    .iter()
                    .find(|m| self.find_measure(m).is_none())
                    .cloned()
                    .unwrap_or_default();
                eprintln!(
                    "[try_compile] qualify_names FAILED for metrics: {:?} → unresolvable: {bad}",
                    intent.metrics
                );
                CatalogError::UnresolvableMetric(bad)
            })?;

        // Extract view names from qualified metrics to use as dimension hints.
        // e.g. ["cardio.max_heart_rate"] → ["cardio"]
        let metric_views: Vec<String> = measures
            .iter()
            .filter_map(|m| m.split('.').next().map(String::from))
            .collect();

        // Qualify dimensions, preferring the metrics' views so that shared
        // names like "date" resolve to the same view as the measures.
        let dimensions = self
            .qualify_names(&intent.dimensions, false, &metric_views)
            .ok_or_else(|| {
                let bad = intent
                    .dimensions
                    .iter()
                    .find(|d| self.find_dimension(d).is_none())
                    .cloned()
                    .unwrap_or_default();
                eprintln!(
                    "[try_compile] qualify_names FAILED for dimensions: {:?} → unresolvable: {bad}",
                    intent.dimensions
                );
                CatalogError::UnresolvableDimension(bad)
            })?;

        eprintln!("[try_compile] qualified: measures={measures:?} dimensions={dimensions:?}");

        // Parse intent filters into structured airlayer QueryFilters.
        // Filters that contain SQL functions or complex expressions cannot be
        // represented in airlayer's filter API, so we bail to TooComplex.
        let filters = self.parse_intent_filters(&intent.filters, &metric_views)?;
        eprintln!("[try_compile] parsed filters: {filters:?}");

        let request = airlayer::engine::query::QueryRequest {
            measures,
            dimensions,
            filters,
            segments: vec![],
            time_dimensions: vec![],
            order: vec![],
            limit: None,
            offset: None,
            timezone: None,
            ungrouped: false,
            through: vec![],
        };

        let result = self.engine.compile_query(&request).map_err(|e| {
            eprintln!("[try_compile] airlayer compile_query FAILED: {e}");
            CatalogError::TooComplex(format!("airlayer compile error: {e}"))
        })?;

        let sql = crate::airlayer_compat::substitute_params(&result.sql, &result.params);

        Ok(sql)
    }

    fn get_context(&self, intent: &AnalyticsIntent) -> QueryContext {
        let metric_defs = intent
            .metrics
            .iter()
            .filter_map(|m| self.get_metric_definition(m))
            .collect();

        let dim_defs = intent
            .dimensions
            .iter()
            .filter_map(|d| self.find_dimension(d))
            .map(|(_, d)| DimensionSummary {
                name: d.name.clone(),
                description: d.description.clone().unwrap_or_default(),
                data_type: d.dimension_type.to_string(),
            })
            .collect();

        // Collect join paths relevant to the intent's metrics.
        let mut join_paths = Vec::new();
        for metric in &intent.metrics {
            if let Some((view, _)) = self.find_measure(metric) {
                for other in self.engine.views() {
                    if other.name == view.name {
                        continue;
                    }
                    if let Some(jp) = self.get_join_path(&view.name, &other.name) {
                        let key = (view.name.clone(), other.name.clone());
                        if !join_paths
                            .iter()
                            .any(|(a, b, _): &(String, String, JoinPath)| {
                                (*a == key.0 && *b == key.1) || (*a == key.1 && *b == key.0)
                            })
                        {
                            join_paths.push((key.0, key.1, jp));
                        }
                    }
                }
            }
        }

        // Build a prompt-ready description of all views.
        let schema_lines: Vec<String> = self
            .engine
            .views()
            .iter()
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
                    .map(|d| format!("{}:{}", d.name, d.dimension_type))
                    .collect();
                format!(
                    "view `{}` (source: {})  measures=[{}]  dimensions=[{}]",
                    v.name,
                    source,
                    measures.join(", "),
                    dims.join(", ")
                )
            })
            .collect();

        QueryContext {
            metric_definitions: metric_defs,
            dimension_definitions: dim_defs,
            join_paths,
            schema_description: schema_lines.join("\n"),
            compile_failure_reason: None,
        }
    }

    fn table_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .engine
            .views()
            .iter()
            .map(|v| v.table.clone().unwrap_or_else(|| v.name.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn connector_for_table(&self, table: &str) -> Option<&str> {
        self.engine.views().iter().find_map(|v| {
            let matches = v.name.eq_ignore_ascii_case(table)
                || v.table
                    .as_deref()
                    .map_or(false, |t| t.eq_ignore_ascii_case(table));
            if matches {
                v.datasource.as_deref()
            } else {
                None
            }
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airlayer_compat;

    fn build_catalog(view_yamls: &[&str]) -> SemanticCatalog {
        let mut views = Vec::new();
        for yaml in view_yamls {
            views.push(airlayer_compat::parse_view_yaml(yaml).unwrap());
        }
        let layer = airlayer::SemanticLayer::new(views, None);
        let dialects = airlayer::DatasourceDialectMap::with_default(airlayer::Dialect::DuckDB);
        let engine = airlayer::SemanticEngine::from_semantic_layer(layer, dialects).unwrap();
        SemanticCatalog::from_engine(engine)
    }

    fn orders_view() -> &'static str {
        r#"
name: orders_view
description: Order analytics
table: orders
entities:
  - name: order_id
    type: primary
    key: order_id
  - name: customer_id
    type: primary
    key: customer_id
dimensions:
  - name: order_id
    type: number
    expr: order_id
  - name: customer_id
    type: number
    expr: customer_id
  - name: status
    type: string
    expr: status
    samples: [completed, pending]
  - name: order_date
    type: date
    expr: order_date
measures:
  - name: revenue
    type: sum
    expr: amount
    description: Total revenue
  - name: order_count
    type: count
"#
    }

    fn customers_view() -> &'static str {
        r#"
name: customers_view
description: Customer dimension
table: customers
entities:
  - name: customer_id
    type: foreign
    key: customer_id
dimensions:
  - name: customer_id
    type: number
    expr: customer_id
  - name: region
    type: string
    expr: region
  - name: name
    type: string
    expr: name
"#
    }

    fn catalog() -> SemanticCatalog {
        build_catalog(&[orders_view(), customers_view()])
    }

    // ── empty() ───────────────────────────────────────────────────────────────

    #[test]
    fn empty_catalog_has_no_views() {
        let cat = SemanticCatalog::empty();
        assert!(cat.is_empty());
        assert!(cat.table_names().is_empty());
    }

    #[test]
    fn empty_catalog_table_exists_false() {
        assert!(!SemanticCatalog::empty().table_exists("anything"));
    }

    #[test]
    fn empty_catalog_to_prompt_string_mentions_tools() {
        let s = SemanticCatalog::empty().to_prompt_string();
        assert!(s.contains("list_tables"));
    }

    // ── table_exists ──────────────────────────────────────────────────────────

    #[test]
    fn table_exists_known_view() {
        assert!(catalog().table_exists("orders_view"));
    }

    #[test]
    fn table_exists_case_insensitive() {
        assert!(catalog().table_exists("ORDERS_VIEW"));
    }

    #[test]
    fn table_exists_unknown() {
        assert!(!catalog().table_exists("ghost"));
    }

    // ── column_exists ─────────────────────────────────────────────────────────

    #[test]
    fn column_exists_dimension() {
        assert!(catalog().column_exists("orders_view", "status"));
    }

    #[test]
    fn column_exists_measure() {
        assert!(catalog().column_exists("orders_view", "revenue"));
    }

    #[test]
    fn column_exists_unknown() {
        assert!(!catalog().column_exists("orders_view", "ghost"));
    }

    // ── column_tables ─────────────────────────────────────────────────────────

    #[test]
    fn column_tables_finds_all_views() {
        let tables = catalog().column_tables("customer_id");
        assert!(tables.contains(&"orders_view".to_string()));
        assert!(tables.contains(&"customers_view".to_string()));
    }

    // ── columns_of ────────────────────────────────────────────────────────────

    #[test]
    fn columns_of_returns_dims_and_measures() {
        let cols = catalog().columns_of("orders_view");
        assert!(cols.contains(&"status".to_string()));
        assert!(cols.contains(&"revenue".to_string()));
        assert!(cols.contains(&"order_count".to_string()));
    }

    // ── metric_resolves_in_semantic ───────────────────────────────────────────

    #[test]
    fn metric_resolves_known_measure() {
        assert!(catalog().metric_resolves_in_semantic("revenue"));
    }

    #[test]
    fn metric_resolves_unknown() {
        assert!(!catalog().metric_resolves_in_semantic("ghost"));
    }

    #[test]
    fn metric_resolves_dotted() {
        assert!(catalog().metric_resolves_in_semantic("orders_view.revenue"));
    }

    // ── join_exists_in_semantic ───────────────────────────────────────────────

    #[test]
    fn join_exists_between_views() {
        assert!(catalog().join_exists_in_semantic("orders_view", "customers_view"));
    }

    #[test]
    fn join_does_not_exist_for_unknown() {
        assert!(!catalog().join_exists_in_semantic("orders_view", "ghost"));
    }

    // ── to_prompt_string / to_table_summary ──────────────────────────────────

    #[test]
    fn to_prompt_string_includes_views() {
        let s = catalog().to_prompt_string();
        assert!(s.contains("orders_view") || s.contains("revenue"));
    }

    #[test]
    fn to_table_summary_compact() {
        let s = catalog().to_table_summary();
        // table_names() returns underlying table names, not view names.
        assert!(s.contains("orders") || s.contains("customers"));
        assert!(s.contains("2")); // view count
    }
}
