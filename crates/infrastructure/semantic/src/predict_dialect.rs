//! The warehouse dialect a metric-tree `predict` must assume.
//!
//! airlayer's `predict_with_values` recomputes each composite from its parsed
//! expression in floating point, and takes the warehouse dialect to know
//! whether that reading can be trusted: where `/` between two integers
//! truncates (Postgres, Redshift, Presto, SQLite) — or where the dialect is
//! unknown — it refuses a move integer division would change. Passing `None`
//! therefore compiles, but makes a `0/50` ratio on DuckDB or Snowflake come back
//! `unquantifiable` for no reason. Every caller resolves it here instead.
//!
//! The dialect is read off the database each affected composite's queries
//! actually run on, by the same rule the metric-tree executor uses to pick
//! that database ([`view_database`]), so the values and the dialect cannot
//! drift apart.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use airlayer::engine::metric_tree::MetricTree;
use airlayer::{DatabaseConfig, Dialect, SemanticLayer};

/// The configured database a view's queries run on: its `datasource:`, else
/// the first database in `config.yml`.
///
/// The metric-tree executor routes every query through this, so a dialect
/// resolved with it names the warehouse that produced the values.
pub fn view_database(datasource: Option<&str>, databases: &[DatabaseConfig]) -> Option<String> {
    datasource
        .map(str::to_string)
        .or_else(|| databases.first().map(|d| d.name.clone()))
}

/// The dialect to hand `predict_with_values` for a move on `levers`.
///
/// Only the composites a move can reach are recomputed, so only their
/// warehouses count. When they span several, a truncating one wins: its
/// refusal is the honest answer for its composites, and a non-truncating
/// dialect would wave them through. `None` — the conservative path — when any
/// of them has no resolvable database or a `type:` airlayer cannot classify.
pub fn predict_dialect(
    layer: &SemanticLayer,
    databases: &[DatabaseConfig],
    tree: &MetricTree,
    levers: &[&str],
) -> Option<Dialect> {
    let views = reached_composite_views(tree, levers);
    let mut dialects = Vec::with_capacity(views.len());
    for view in views {
        let datasource = layer
            .views
            .iter()
            .find(|v| v.name == view)
            .and_then(|v| v.datasource.as_deref());
        let name = view_database(datasource, databases)?;
        let db = databases.iter().find(|d| d.name == name)?;
        dialects.push(Dialect::from_str(&db.db_type)?);
    }
    dialects
        .iter()
        .find(|d| d.truncates_integer_division())
        .or(dialects.first())
        .cloned()
}

/// Views of the composite measures forward-reachable from `levers`, the
/// levers themselves included.
fn reached_composite_views<'a>(tree: &'a MetricTree, levers: &[&'a str]) -> BTreeSet<&'a str> {
    let mut fwd: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &tree.edges {
        fwd.entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut seen: HashSet<&str> = levers.iter().copied().collect();
    let mut queue: VecDeque<&str> = levers.iter().copied().collect();
    while let Some(node) = queue.pop_front() {
        for &next in fwd.get(node).map(Vec::as_slice).unwrap_or(&[]) {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    tree.nodes
        .iter()
        .filter(|n| n.is_composite && seen.contains(n.id.as_str()))
        .map(|n| n.view.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database_config, parse_view_yaml};

    const SALES: &str = r#"
name: sales
table: public.sales
datasource: SALES_DB
measures:
  - name: converted
    type: count
    expr: id
  - name: visits
    type: count
    expr: visit_id
  - name: conversion
    type: number
    expr: "{{sales.converted}} / {{sales.visits}}"
"#;

    const STOCK: &str = r#"
name: stock
table: public.stock
measures:
  - name: units
    type: sum
    expr: qty
  - name: capacity
    type: sum
    expr: cap
  - name: fill
    type: number
    expr: "{{stock.units}} / {{stock.capacity}}"
"#;

    fn layer(yamls: &[&str]) -> SemanticLayer {
        let views = yamls.iter().map(|y| parse_view_yaml(y).unwrap()).collect();
        SemanticLayer::new(views, None)
    }

    fn dialect_for(yamls: &[&str], dbs: &[DatabaseConfig], levers: &[&str]) -> Option<Dialect> {
        let layer = layer(yamls);
        let tree = MetricTree::build(&layer);
        predict_dialect(&layer, dbs, &tree, levers)
    }

    #[test]
    fn reads_the_dialect_of_the_views_datasource() {
        let dbs = [
            database_config("OTHER", "postgres"),
            database_config("SALES_DB", "duckdb"),
        ];
        assert_eq!(
            dialect_for(&[SALES], &dbs, &["sales.converted"]),
            Some(Dialect::DuckDB)
        );
    }

    #[test]
    fn a_view_with_no_datasource_reads_the_first_database() {
        let dbs = [
            database_config("WH", "snowflake"),
            database_config("PG", "postgres"),
        ];
        assert_eq!(
            dialect_for(&[STOCK], &dbs, &["stock.units"]),
            Some(Dialect::Snowflake)
        );
    }

    #[test]
    fn an_unresolvable_database_is_unknown_not_guessed() {
        // `SALES_DB` is not configured, and an unclassifiable `type:` is no better.
        assert_eq!(
            dialect_for(
                &[SALES],
                &[database_config("PG", "duckdb")],
                &["sales.converted"]
            ),
            None
        );
        assert_eq!(
            dialect_for(
                &[SALES],
                &[database_config("SALES_DB", "mystery")],
                &["sales.converted"]
            ),
            None
        );
        assert_eq!(dialect_for(&[STOCK], &[], &["stock.units"]), None);
    }

    #[test]
    fn a_truncating_warehouse_among_the_reached_composites_wins() {
        let sales_fill = r#"
name: mix
table: public.mix
datasource: PG
measures:
  - name: ratio
    type: number
    expr: "{{sales.conversion}} / {{stock.fill}}"
"#;
        let dbs = [
            database_config("WH", "duckdb"),
            database_config("SALES_DB", "duckdb"),
            database_config("PG", "postgres"),
        ];
        assert_eq!(
            dialect_for(&[SALES, STOCK, sales_fill], &dbs, &["stock.units"]),
            Some(Dialect::Postgres)
        );
    }

    #[test]
    fn composites_the_move_cannot_reach_do_not_count() {
        // `sales` sits on Postgres, but a move on `stock` never recomputes it.
        let dbs = [
            database_config("WH", "duckdb"),
            database_config("SALES_DB", "postgres"),
        ];
        assert_eq!(
            dialect_for(&[SALES, STOCK], &dbs, &["stock.units"]),
            Some(Dialect::DuckDB)
        );
    }
}
