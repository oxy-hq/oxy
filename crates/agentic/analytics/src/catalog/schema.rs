//! [`SchemaCatalog`] — raw database schema catalog with no business logic.

use std::collections::HashMap;

use agentic_connector::SchemaInfo;

use super::helpers::{cell_to_json, db_type_to_semantic, type_hint};
use super::types::ColumnRange;

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

    // ── Accessors for sibling trait impl ──────────────────────────────────────

    pub(super) fn tables(&self) -> &HashMap<String, Vec<String>> {
        &self.tables
    }

    pub(super) fn join_keys(&self) -> &HashMap<(String, String), String> {
        &self.join_keys
    }

    pub(super) fn column_stats(&self) -> &HashMap<String, ColumnRange> {
        &self.column_stats
    }

    pub(super) fn table_connector(&self) -> &HashMap<String, String> {
        &self.table_connector
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Return `(table, column)` pairs where `column` looks like a metric.
    ///
    /// `metric` can be a bare name (`"revenue"`) or qualified (`"orders.revenue"`).
    pub(super) fn metric_matches(&self, metric: &str) -> Vec<(String, String)> {
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
    pub(super) fn reachable_from(&self, start: &str) -> Vec<String> {
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
