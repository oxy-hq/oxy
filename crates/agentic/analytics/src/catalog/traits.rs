//! The `Catalog` trait — unified interface for all catalog implementations.

use crate::types::AnalyticsIntent;

use super::fuzzy_matches;
use super::types::{
    CatalogError, CatalogSearchResult, ColumnRange, DimensionSummary, JoinPath, MetricDef,
    MetricSummary, QueryContext, SampleTarget,
};

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
