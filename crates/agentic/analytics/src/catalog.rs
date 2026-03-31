//! Unified catalog interface and schema-only implementation.
//!
//! Three catalog implementations live across three files:
//!
//! | File | Type | Description |
//! |---|---|---|
//! | `catalog.rs` (this file) | [`SchemaCatalog`] | Raw database schema — no business logic |
//! | `semantic.rs` | [`SemanticCatalog`] | Oxy .view.yml/.topic.yml semantic layer |
//! | `hybrid.rs` | [`SemanticCatalog`] | Semantic + schema fallback (primary runtime type) |
//!
//! The FSM, tools, and Specifying handler work through the [`Catalog`] trait
//! exclusively — they never inspect the concrete type behind it.

use std::collections::HashMap;

use agentic_connector::SchemaInfo;

use crate::types::AnalyticsIntent;

// ── Fuzzy matching ───────────────────────────────────────────────────────────

/// Minimum Jaro-Winkler similarity score for a fuzzy match.
///
/// 0.8 is conservative enough to avoid matching unrelated short names
/// (e.g. `"min"` vs `"max"` scores ~0.67) while catching common typos and
/// abbreviations (e.g. `"revnue"` vs `"revenue"` scores ~0.95).
const FUZZY_THRESHOLD: f64 = 0.8;

/// Check whether `query` fuzzy-matches `candidate` using Jaro-Winkler similarity.
///
/// Operates on lowercase strings.  Returns `true` when the similarity exceeds
/// [`FUZZY_THRESHOLD`].  Short queries (< 3 chars) are excluded to avoid
/// noise from very short strings where Jaro-Winkler is unreliable.
pub fn fuzzy_matches(query: &str, candidate: &str) -> bool {
    if query.len() < 3 {
        return false;
    }
    let q = query.to_lowercase();
    let c = candidate.to_lowercase();
    strsim::jaro_winkler(&q, &c) >= FUZZY_THRESHOLD
}

// ── Return types ──────────────────────────────────────────────────────────────

/// Brief metric description for browsing/searching.
#[derive(Debug, Clone)]
pub struct MetricSummary {
    pub name: String,
    pub description: String,
    /// Aggregation kind: `"sum"`, `"count"`, `"avg"`, etc.
    /// Empty for raw-schema metrics that have no semantic definition.
    pub metric_type: String,
}

/// Result of a batch catalog search across multiple queries.
///
/// Returned by [`Catalog::search_catalog`].  Contains **deduplicated**
/// metrics and dimensions matching *any* of the supplied query terms.
#[derive(Debug, Clone, Default)]
pub struct CatalogSearchResult {
    pub metrics: Vec<MetricSummary>,
    pub dimensions: Vec<DimensionSummary>,
}

/// Brief dimension description for browsing/searching.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DimensionSummary {
    pub name: String,
    pub description: String,
    /// Semantic type: `"string"`, `"number"`, `"date"`, `"boolean"`.
    pub data_type: String,
}

/// Full metric definition returned by [`Catalog::get_metric_definition`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricDef {
    pub name: String,
    /// SQL expression or column reference (e.g. `"orders.revenue"`, `"amount"`).
    pub expr: String,
    /// Aggregation kind: `"column"`, `"sum"`, `"count"`, `"avg"`, `"min"`,
    /// `"max"`, `"count_distinct"`.
    pub metric_type: String,
    /// Source table or view name.
    pub table: String,
    pub description: Option<String>,
    /// Logical connector name that owns this metric's source table.
    ///
    /// `None` means "use the default connector".  Set by [`SchemaCatalog`] from
    /// the connector tag recorded during [`from_schema_info_named`], and by
    /// [`SemanticCatalog`] from the `data_source:` field on the view YAML.
    ///
    /// [`from_schema_info_named`]: SchemaCatalog::from_schema_info_named
    pub data_source: Option<String>,
}

/// Value range and sample values for a dimension column.
#[derive(Debug, Clone)]
pub struct ColumnRange {
    pub min: Option<serde_json::Value>,
    pub max: Option<serde_json::Value>,
    pub sample_values: Vec<serde_json::Value>,
    /// Semantic type: `"string"`, `"number"`, `"date"`, `"boolean"`.
    pub data_type: String,
}

/// Resolved physical target for the `sample_column` tool.
///
/// When the semantic layer is active the LLM refers to views and dimensions
/// by their logical names (e.g. `orders_view.status`).  `SampleTarget` maps
/// those to the underlying database table and column expression so the tool
/// can build a valid SQL query.
#[derive(Debug, Clone)]
pub struct SampleTarget {
    /// Underlying database table (e.g. `"orders"`).
    pub table: String,
    /// SQL column expression (e.g. `"order_status"`).
    pub column_expr: String,
    /// Pre-existing sample values from the semantic definition, if any.
    /// When non-empty the tool can return these directly instead of hitting
    /// the database.
    pub static_samples: Vec<String>,
    /// Semantic data type (e.g. `"string"`, `"date"`).
    pub data_type: Option<String>,
}

/// A resolved join path between two tables or views.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JoinPath {
    /// Human-readable path expression (e.g. `"orders JOIN customers ON customer_id"`).
    pub path: String,
    pub join_type: String,
}

/// Rich context provided to the LLM for SQL generation when
/// [`Catalog::try_compile`] returns [`CatalogError::TooComplex`].
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct QueryContext {
    pub metric_definitions: Vec<MetricDef>,
    pub dimension_definitions: Vec<DimensionSummary>,
    /// `(from_entity, to_entity, path)` tuples.
    pub join_paths: Vec<(String, String, JoinPath)>,
    /// Prompt-ready schema description (tables, columns, join keys).
    pub schema_description: String,
    /// Why `try_compile` returned `TooComplex`, if applicable.
    pub compile_failure_reason: Option<String>,
}

// ── CatalogError ──────────────────────────────────────────────────────────────

/// Error variants from [`Catalog::try_compile`] and related routing decisions.
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogError {
    /// The query is too complex for direct compilation; the LLM must handle it.
    ///
    /// The `String` contains a human-readable reason for debugging
    /// (e.g. "filters contain SQL functions", "airlayer compile error: ...").
    TooComplex(String),
    /// The named metric is unknown to this catalog.
    UnresolvableMetric(String),
    /// The named dimension is unknown or unreachable via any join path.
    UnresolvableDimension(String),
}

// ── Catalog trait ─────────────────────────────────────────────────────────────

/// Unified interface for all catalog implementations.
///
/// The FSM, tools, and Specifying handler call only these methods — they never
/// cast to a concrete type or check which implementation is behind the trait.
///
/// # Two paths through Specifying
///
/// ```text
/// catalog.try_compile(&intent):
///   Ok(sql)                    → SemanticLayer path: skip Solving
///   Err(TooComplex)            → LlmWithSemanticContext path: enter Solving
///   Err(Unresolvable*)         → fatal: metric/dimension not found anywhere
/// ```
pub trait Catalog: Send + Sync {
    // ── Clarifying tools ─────────────────────────────────────────────────

    /// List metrics whose name or description contains `query`.
    /// Empty `query` returns all metrics.
    fn list_metrics(&self, query: &str) -> Vec<MetricSummary>;

    /// List dimensions available when computing `metric`.
    fn list_dimensions(&self, metric: &str) -> Vec<DimensionSummary>;

    /// Batch-search metrics **and** their dimensions in a single call.
    ///
    /// For each query term the catalog runs `list_metrics` and then
    /// `list_dimensions` on every matched metric.  Results are deduplicated
    /// by name so the caller gets a single flat answer regardless of how
    /// many terms overlapped.
    ///
    /// **Three-tier fallback**:
    /// 1. Exact substring match via `list_metrics(query)`.
    /// 2. Token-fallback: split multi-word query into individual words.
    /// 3. Fuzzy match: Jaro-Winkler similarity against all metric names.
    fn search_catalog(&self, queries: &[&str]) -> CatalogSearchResult {
        use std::collections::HashSet;
        let mut seen_metrics = HashSet::new();
        let mut seen_dims = HashSet::new();
        let mut result = CatalogSearchResult::default();

        for &q in queries {
            let mut hits = self.list_metrics(q);

            // Tier 2 — token-fallback: split multi-word query into individual words.
            if hits.is_empty() && q.split_whitespace().count() > 1 {
                for word in q.split_whitespace() {
                    hits.extend(self.list_metrics(word));
                }
            }

            // Tier 3 — fuzzy match: Jaro-Winkler against all metric names.
            // For qualified names (view.member), also try the bare member name
            // so that fuzzy matching works against user queries that don't
            // include the view prefix.
            if hits.is_empty() {
                let all_metrics = self.list_metrics("");
                for m in &all_metrics {
                    let bare_name = m.name.split('.').next_back().unwrap_or(&m.name);
                    if fuzzy_matches(q, &m.name)
                        || fuzzy_matches(q, bare_name)
                        || fuzzy_matches(q, &m.description)
                    {
                        hits.push(m.clone());
                    }
                }
                // Also try individual tokens for fuzzy matching.
                if hits.is_empty() && q.split_whitespace().count() > 1 {
                    for word in q.split_whitespace() {
                        for m in &all_metrics {
                            let bare_name = m.name.split('.').next_back().unwrap_or(&m.name);
                            if fuzzy_matches(word, &m.name) || fuzzy_matches(word, bare_name) {
                                hits.push(m.clone());
                            }
                        }
                    }
                }
            }

            for m in &hits {
                if seen_metrics.insert(m.name.clone()) {
                    result.metrics.push(m.clone());
                }
                for d in self.list_dimensions(&m.name) {
                    if seen_dims.insert(d.name.clone()) {
                        result.dimensions.push(d.clone());
                    }
                }
            }
        }
        result
    }

    /// Get the full definition of a single metric.
    fn get_metric_definition(&self, metric: &str) -> Option<MetricDef>;

    // ── Specifying tools ─────────────────────────────────────────────────

    /// Return dimensions reachable from `metric` via known join paths.
    fn get_valid_dimensions(&self, metric: &str) -> Vec<DimensionSummary>;

    /// Return value range and sample values for a dimension column.
    fn get_column_range(&self, dimension: &str) -> Option<ColumnRange>;

    /// Return the join path between two entities/tables, if one is known.
    fn get_join_path(&self, from: &str, to: &str) -> Option<JoinPath>;

    /// Resolve a `(view_or_table, dimension_or_column)` pair to the physical
    /// database table and column expression needed by `sample_column`.
    ///
    /// Returns `None` when the catalog has no semantic mapping for the given
    /// names — the caller should fall back to using the raw names as-is.
    fn resolve_sample_target(&self, table: &str, column: &str) -> Option<SampleTarget> {
        let _ = (table, column);
        None
    }

    // ── Routing ───────────────────────────────────────────────────────────

    /// Try to compile `intent` directly to SQL without an LLM call.
    ///
    /// Returns `Ok(sql)` for simple group-by aggregations the catalog can
    /// handle deterministically.  Returns `Err(TooComplex)` for anything that
    /// requires LLM reasoning.  Returns `Err(Unresolvable*)` when a metric or
    /// dimension name is unknown to this catalog entirely.
    fn try_compile(&self, intent: &AnalyticsIntent) -> Result<String, CatalogError>;

    /// Return rich context for LLM-based SQL generation.
    ///
    /// Called when `try_compile` returns `TooComplex`.  The returned context
    /// includes metric formulas, dimension types, join paths, and a
    /// prompt-ready schema string.
    fn get_context(&self, intent: &AnalyticsIntent) -> QueryContext;

    // ── Schema metadata (used by dry_run and validation helpers) ──────────

    /// Return all table/view names known to this catalog (sorted).
    fn table_names(&self) -> Vec<String>;

    /// Return the logical connector name responsible for `table`, if known.
    ///
    /// Returns `None` when the table has no connector tag (use the default
    /// connector).  The default implementation always returns `None` so
    /// implementations that do not track connector provenance remain valid
    /// without change.
    fn connector_for_table(&self, table: &str) -> Option<&str> {
        let _ = table;
        None
    }
}

// ── Keyword heuristics for SchemaCatalog ─────────────────────────────────────

// ── SchemaInfo conversion helpers ────────────────────────────────────────────

/// Convert a [`CellValue`] from schema introspection to a JSON value.
fn cell_to_json(v: &agentic_core::result::CellValue) -> Option<serde_json::Value> {
    use agentic_core::result::CellValue;
    match v {
        CellValue::Text(s) => Some(serde_json::Value::String(s.clone())),
        CellValue::Number(f) => Some(serde_json::json!(f)),
        CellValue::Null => None,
    }
}

/// Map a database-native type string to the semantic type used by the catalog.
///
/// Returns `None` when unrecognised — the caller falls back to the
/// column-name heuristic ([`type_hint`]).
fn db_type_to_semantic(db_type: &str) -> Option<&'static str> {
    let t = db_type.to_uppercase();
    // ── Temporal (checked before numeric so "INTERVAL" doesn't match "INT") ─
    if t.starts_with("DATE")
        || t.starts_with("TIME")
        || t.starts_with("TIMESTAMP")
        || t.starts_with("DATETIME")
        || t.starts_with("INTERVAL")
    {
        return Some("date");
    }
    // ── Numeric ──────────────────────────────────────────────────────────
    if t.starts_with("INT")
        || t.starts_with("TINYINT")
        || t.starts_with("SMALLINT")
        || t.starts_with("BIGINT")
        || t.starts_with("HUGEINT")
        || t.starts_with("FLOAT")
        || t.starts_with("DOUBLE")
        || t.starts_with("DECIMAL")
        || t.starts_with("NUMERIC")
        || t.starts_with("REAL")
        || t.starts_with("NUMBER")
    {
        return Some("number");
    }
    // ── Boolean ───────────────────────────────────────────────────────────
    if t.starts_with("BOOL") {
        return Some("boolean");
    }
    // ── String ────────────────────────────────────────────────────────────
    if t.starts_with("VARCHAR")
        || t.starts_with("CHAR")
        || t.starts_with("TEXT")
        || t.starts_with("STRING")
        || t.starts_with("CLOB")
        || t.starts_with("ENUM")
        || t.starts_with("BLOB")
        || t.starts_with("BYTES")
    {
        return Some("string");
    }
    None
}

/// Column-name fragments that suggest a numeric metric.
const METRIC_KEYWORDS: &[&str] = &[
    "amount",
    "total",
    "count",
    "revenue",
    "cost",
    "price",
    "sum",
    "avg",
    "average",
    "qty",
    "quantity",
    "volume",
    "sales",
    "profit",
    "margin",
    "rate",
    "score",
    "weight",
    "calories",
    "reps",
    "sets",
    "duration",
    "distance",
    "speed",
    "heart",
    "fat",
    "protein",
    "carbs",
    "incline",
    "elevation",
    "rpe",
    "stiffness",
    "max",
    "min",
    "percent",
    "pct",
    "ratio",
];

/// Returns `true` when `col` looks like an identifier or foreign key.
fn is_id_col(col: &str) -> bool {
    let l = col.to_lowercase();
    l == "id" || l.ends_with("_id") || l.starts_with("id_")
}

/// Column-name fragments that suggest a date/time dimension.
const DATE_KEYWORDS: &[&str] = &[
    "date",
    "time",
    "day",
    "month",
    "year",
    "created_at",
    "updated_at",
    "timestamp",
];

fn is_metric_col(col: &str) -> bool {
    let l = col.to_lowercase();
    METRIC_KEYWORDS.iter().any(|kw| l.contains(kw))
}

fn is_date_col(col: &str) -> bool {
    let l = col.to_lowercase();
    DATE_KEYWORDS.iter().any(|kw| l.contains(kw))
}

fn type_hint(col: &str) -> &'static str {
    if is_date_col(col) {
        "date"
    } else if is_metric_col(col) {
        "number"
    } else {
        "string"
    }
}

// ── SchemaCatalog ─────────────────────────────────────────────────────────────

/// Error returned by [`SchemaCatalog::merge`] when two schemas share a table name.
#[derive(Debug)]
pub enum SchemaMergeError {
    /// A table with the same name exists in both catalogs.
    DuplicateTable(String),
}

impl std::fmt::Display for SchemaMergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaMergeError::DuplicateTable(t) => {
                write!(f, "table '{t}' exists in multiple configured databases")
            }
        }
    }
}

/// Lightweight table/column registry for raw database schemas.
///
/// Knows about tables, columns, and explicit foreign-key join relationships.
/// Uses column-name heuristics to distinguish metrics (numeric) from dimensions
/// (string/date), since no business logic is defined.
///
/// `try_compile` **always** returns [`CatalogError::TooComplex`] — without a
/// semantic layer every query must go through the LLM solver.
///
/// # Example
///
/// ```
/// use agentic_analytics::SchemaCatalog;
///
/// let catalog = SchemaCatalog::new()
///     .add_table("orders", &["order_id", "customer_id", "revenue", "date"])
///     .add_table("customers", &["customer_id", "region"])
///     .add_join_key("orders", "customers", "customer_id");
/// ```
#[derive(Debug, Default, Clone)]
pub struct SchemaCatalog {
    /// Lowercase table name → list of lowercase column names.
    tables: HashMap<String, Vec<String>>,
    /// Explicit join keys: sorted `(table_a, table_b)` → join column.
    join_keys: HashMap<(String, String), String>,
    /// Real column statistics gathered from a live database connection.
    ///
    /// Key is `"table.column"` in lowercase.  When an entry is present,
    /// [`get_column_range`] returns it verbatim instead of the heuristic
    /// type-only placeholder.
    ///
    /// [`get_column_range`]: SchemaCatalog::get_column_range
    column_stats: HashMap<String, ColumnRange>,
    /// Maps lowercase table name to its logical connector name.
    ///
    /// Populated when building from [`from_schema_info_named`] and preserved
    /// through [`merge`].  Empty for catalogs built via the builder pattern
    /// or the unnamed [`from_schema_info`] path.
    ///
    /// [`from_schema_info_named`]: SchemaCatalog::from_schema_info_named
    /// [`merge`]: SchemaCatalog::merge
    /// [`from_schema_info`]: SchemaCatalog::from_schema_info
    table_connector: HashMap<String, String>,
}

impl SchemaCatalog {
    /// Create an empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a table with its columns (builder-style).
    ///
    /// Table names are normalised to lowercase for case-insensitive lookup.
    /// Column names are stored with their **original case** so they can be
    /// used verbatim in generated SQL (e.g. `"Weight (lbs)"`).
    pub fn add_table(mut self, table: &str, columns: &[&str]) -> Self {
        self.tables.insert(
            table.to_lowercase(),
            columns.iter().map(|c| c.to_string()).collect(),
        );
        self
    }

    /// Register an explicit join relationship (builder-style).
    ///
    /// The pair `(a, b)` is stored in sorted order so lookup is
    /// order-independent.
    pub fn add_join_key(mut self, a: &str, b: &str, key: &str) -> Self {
        let mut pair = [a.to_lowercase(), b.to_lowercase()];
        pair.sort();
        let [a_s, b_s] = pair;
        self.join_keys.insert((a_s, b_s), key.to_lowercase());
        self
    }

    /// Return `true` if the catalog knows about `table`.
    pub fn table_exists(&self, table: &str) -> bool {
        self.tables.contains_key(&table.to_lowercase())
    }

    /// Return `true` if `column` exists in `table` (case-insensitive).
    pub fn column_exists(&self, table: &str, column: &str) -> bool {
        let col_lc = column.to_lowercase();
        self.tables
            .get(&table.to_lowercase())
            .map(|cols| cols.iter().any(|c| c.to_lowercase() == col_lc))
            .unwrap_or(false)
    }

    /// Return all table names known to the catalog (lowercase, sorted).
    pub fn table_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tables.keys().cloned().collect();
        names.sort();
        names
    }

    /// Return all table names (lowercase) that contain `column` (case-insensitive).
    pub fn column_tables(&self, column: &str) -> Vec<&str> {
        let col = column.to_lowercase();
        self.tables
            .iter()
            .filter(|(_, cols)| cols.iter().any(|c| c.to_lowercase() == col))
            .map(|(t, _)| t.as_str())
            .collect()
    }

    /// Return all column names for `table` (lowercase, sorted).
    ///
    /// Returns an empty `Vec` when the table is not known.
    pub fn columns_of(&self, table: &str) -> Vec<String> {
        let mut cols = self
            .tables
            .get(&table.to_lowercase())
            .cloned()
            .unwrap_or_default();
        cols.sort();
        cols
    }

    /// Insert pre-computed column statistics (builder-style).
    ///
    /// `dimension` must be `"table.column"` in **any** case; it is
    /// normalised to lowercase internally.
    pub fn with_column_stats(mut self, dimension: &str, stats: ColumnRange) -> Self {
        self.column_stats.insert(dimension.to_lowercase(), stats);
        self
    }

    /// Build a `SchemaCatalog` from a vendor-neutral [`SchemaInfo`] returned
    /// by [`DatabaseConnector::introspect_schema`].
    ///
    /// - Tables and columns are registered with their original case for correct
    ///   SQL generation.
    /// - Column statistics (MIN, MAX, sample values) are stored so
    ///   [`get_column_range`] returns real values instead of placeholders.
    /// - The semantic `data_type` is derived from the DB-native type string
    ///   when recognised; otherwise the column-name heuristic is used.
    /// - Declared join keys in [`SchemaInfo::join_keys`] are all registered.
    ///
    /// [`DatabaseConnector::introspect_schema`]: agentic_connector::DatabaseConnector::introspect_schema
    /// [`get_column_range`]: SchemaCatalog::get_column_range
    pub fn from_schema_info(info: &SchemaInfo) -> Self {
        let mut catalog = Self::default();

        for table in &info.tables {
            let col_names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
            catalog.tables.insert(
                table.name.to_lowercase(),
                col_names.iter().map(|s| s.to_string()).collect(),
            );

            for col in &table.columns {
                let semantic_type = db_type_to_semantic(&col.data_type)
                    .unwrap_or_else(|| type_hint(&col.name))
                    .to_string();

                let min = col.min.as_ref().and_then(cell_to_json);
                let max = col.max.as_ref().and_then(cell_to_json);
                let sample_values = col.sample_values.iter().filter_map(cell_to_json).collect();

                let key = format!("{}.{}", table.name.to_lowercase(), col.name.to_lowercase());
                catalog.column_stats.insert(
                    key,
                    ColumnRange {
                        min,
                        max,
                        sample_values,
                        data_type: semantic_type,
                    },
                );
            }
        }

        for (a, b, key) in &info.join_keys {
            let mut pair = [a.to_lowercase(), b.to_lowercase()];
            pair.sort();
            let [a_s, b_s] = pair;
            catalog.join_keys.insert((a_s, b_s), key.to_lowercase());
        }

        catalog
    }

    /// Build a `SchemaCatalog` from a [`SchemaInfo`], tagging every table with
    /// `connector_name` so the solver can route queries to the correct database.
    ///
    /// Behaves identically to [`from_schema_info`] but additionally records the
    /// connector name for every table registered.
    ///
    /// [`from_schema_info`]: SchemaCatalog::from_schema_info
    pub fn from_schema_info_named(info: &SchemaInfo, connector_name: &str) -> Self {
        let mut catalog = Self::from_schema_info(info);
        for table_name in catalog.tables.keys().cloned().collect::<Vec<_>>() {
            catalog
                .table_connector
                .insert(table_name, connector_name.to_string());
        }
        catalog
    }

    /// Return the logical connector name for `table`, if one was recorded.
    ///
    /// Returns `None` for tables registered without a connector name (e.g. via
    /// the builder pattern or plain [`from_schema_info`]).
    ///
    /// [`from_schema_info`]: SchemaCatalog::from_schema_info
    pub fn connector_for_table(&self, table: &str) -> Option<&str> {
        self.table_connector
            .get(&table.to_lowercase())
            .map(|s| s.as_str())
    }

    /// Merge `other` into `self`.
    ///
    /// All tables, join keys, column statistics, and connector tags from
    /// `other` are absorbed into `self`.  Returns
    /// `Err(SchemaMergeError::DuplicateTable)` if a table name already exists
    /// in `self` — checked before any mutation so `self` is left untouched on
    /// failure.
    pub fn merge(&mut self, other: SchemaCatalog) -> Result<(), SchemaMergeError> {
        // Check for collisions before mutating.
        for table in other.tables.keys() {
            if self.tables.contains_key(table) {
                return Err(SchemaMergeError::DuplicateTable(table.clone()));
            }
        }
        // Absorb all fields.
        self.tables.extend(other.tables);
        self.join_keys.extend(other.join_keys);
        self.column_stats.extend(other.column_stats);
        self.table_connector.extend(other.table_connector);
        Ok(())
    }

    /// Return the join key shared between `a` and `b`, if one is registered.
    ///
    /// Lookup is order-independent (same as [`add_join_key`]).
    ///
    /// [`add_join_key`]: SchemaCatalog::add_join_key
    pub fn join_key(&self, a: &str, b: &str) -> Option<&str> {
        let mut pair = [a.to_lowercase(), b.to_lowercase()];
        pair.sort();
        let [a_s, b_s] = pair;
        self.join_keys.get(&(a_s, b_s)).map(|k| k.as_str())
    }

    /// Render the catalog as a compact schema description suitable for LLM prompts.
    ///
    /// # Example output
    ///
    /// ```text
    /// Tables and columns:
    ///   customers: customer_id, region
    ///   orders: customer_id, date, order_id, revenue
    ///
    /// Join relationships:
    ///   customers <-> orders ON customer_id
    /// ```
    pub fn to_prompt_string(&self) -> String {
        let mut lines = vec!["Tables and columns:".to_string()];

        let mut tables: Vec<(&String, &Vec<String>)> = self.tables.iter().collect();
        tables.sort_by_key(|(k, _)| k.as_str());
        for (table, cols) in tables {
            let mut sorted_cols = cols.clone();
            sorted_cols.sort();
            lines.push(format!("  {table}: {}", sorted_cols.join(", ")));
        }

        if !self.join_keys.is_empty() {
            lines.push(String::new());
            lines.push("Join relationships:".to_string());
            let mut joins: Vec<(&(String, String), &String)> = self.join_keys.iter().collect();
            joins.sort_by_key(|((a, b), _)| (a.as_str(), b.as_str()));
            for ((a, b), key) in joins {
                lines.push(format!("  {a} <-> {b} ON {key}"));
            }
        }

        lines.join("\n")
    }

    /// Table-level summary: table names with column names, and join
    /// relationships.
    ///
    /// ```text
    /// Tables (5):
    ///   body_composition: date, weight, body_fat
    ///   cardio: date, type, duration, distance, pace, heart_rate, calories, notes
    ///   strength: date, exercise, sets, reps, weight, volume, muscle_group, notes
    ///
    /// Join relationships:
    ///   customers <-> orders ON customer_id
    /// ```
    pub fn to_table_summary(&self) -> String {
        let mut lines = vec![format!("Tables ({}):", self.tables.len())];

        let mut tables: Vec<(&String, &Vec<String>)> = self.tables.iter().collect();
        tables.sort_by_key(|(k, _)| k.as_str());
        for (table, cols) in tables {
            lines.push(format!("  {table}: {}", cols.join(", ")));
        }

        if !self.join_keys.is_empty() {
            lines.push(String::new());
            lines.push("Join relationships:".to_string());
            let mut joins: Vec<(&(String, String), &String)> = self.join_keys.iter().collect();
            joins.sort_by_key(|((a, b), _)| (a.as_str(), b.as_str()));
            for ((a, b), key) in joins {
                lines.push(format!("  {a} <-> {b} ON {key}"));
            }
        }

        lines.join("\n")
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Return `(table, column)` pairs where `column` looks like a metric.
    ///
    /// `metric` can be a bare name (`"revenue"`) or qualified (`"orders.revenue"`).
    fn metric_matches(&self, metric: &str) -> Vec<(String, String)> {
        let (table_hint, col_name) = if let Some(dot) = metric.find('.') {
            (
                Some(metric[..dot].to_lowercase()),
                metric[dot + 1..].to_lowercase(),
            )
        } else {
            (None, metric.to_lowercase())
        };

        self.tables
            .iter()
            .filter(|(t, cols)| {
                table_hint.as_deref().is_none_or(|h| h == t.as_str())
                    && cols.iter().any(|c| c.to_lowercase() == col_name)
            })
            .map(|(t, cols)| {
                // Return original-case column name for correct SQL generation.
                let orig = cols
                    .iter()
                    .find(|c| c.to_lowercase() == col_name)
                    .cloned()
                    .unwrap_or_else(|| col_name.clone());
                (t.clone(), orig)
            })
            .collect()
    }

    /// Return all tables reachable from `start` via foreign-key relationships
    /// (breadth-first, including `start` itself).
    fn reachable_from(&self, start: &str) -> Vec<String> {
        let start = start.to_lowercase();
        let mut visited = vec![start.clone()];
        let mut queue = vec![start];
        while let Some(current) = queue.first().cloned() {
            queue.remove(0);
            for (a, b) in self.join_keys.keys() {
                let neighbor = if a == &current {
                    Some(b.clone())
                } else if b == &current {
                    Some(a.clone())
                } else {
                    None
                };
                if let Some(n) = neighbor
                    && !visited.contains(&n)
                {
                    visited.push(n.clone());
                    queue.push(n);
                }
            }
        }
        visited
    }
}

impl Catalog for SchemaCatalog {
    fn list_metrics(&self, query: &str) -> Vec<MetricSummary> {
        let q = query.to_lowercase();
        let mut results = Vec::new();
        let mut tables: Vec<(&String, &Vec<String>)> = self.tables.iter().collect();
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
                    .column_stats
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
            if let Some(cols) = self.tables.get(t) {
                let mut sorted = cols.clone();
                sorted.sort();
                for col in sorted {
                    // Use column_stats type when available, else keyword heuristic.
                    let stats_key = format!("{}.{}", t, col.to_lowercase());
                    let is_numeric = self
                        .column_stats
                        .get(&stats_key)
                        .map(|r| r.data_type == "number")
                        .unwrap_or_else(|| is_metric_col(&col));
                    if is_numeric && !is_id_col(&col) {
                        continue; // metrics are not dimensions
                    }
                    let data_type = self
                        .column_stats
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
                data_source: self.table_connector.get(&table).cloned(),
                table,
                description: None,
            })
    }

    fn get_valid_dimensions(&self, metric: &str) -> Vec<DimensionSummary> {
        // Return only dimensions from the metric's own table (not FK-joined
        // tables).  This keeps token usage low — callers can use
        // `list_dimensions` via `search_catalog` for the full set.
        let table = metric.split('.').next().unwrap_or("").to_lowercase();
        if let Some(cols) = self.tables.get(&table) {
            cols.iter()
                .filter(|c| {
                    if is_id_col(c) {
                        return false;
                    }
                    let stats_key = format!("{}.{}", table, c.to_lowercase());
                    let is_numeric = self
                        .column_stats
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
        if let Some(range) = self.column_stats.get(&dimension.to_lowercase()) {
            return Some(range.clone());
        }

        // Fall back to an unqualified column name scan across all tables.
        let found_in: Vec<&str> = self
            .tables
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
            if let Some(range) = self.column_stats.get(&key) {
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
        let mut joins: Vec<(&(String, String), &String)> = self.join_keys.iter().collect();
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
        self.table_connector
            .get(&table.to_lowercase())
            .map(|s| s.as_str())
    }
}

// ── Backward-compat alias ─────────────────────────────────────────────────────

/// Kept for backward compatibility.
///
/// New code should use [`CatalogError::TooComplex`] instead.
#[derive(Debug)]
pub enum SemanticLayerError {
    /// The query is too complex for the semantic layer to compile directly;
    /// the caller should fall back to LLM-based specification.
    TooComplex,
}

impl From<SemanticLayerError> for CatalogError {
    fn from(_: SemanticLayerError) -> Self {
        CatalogError::TooComplex("semantic layer error".into())
    }
}
