use super::*;
use crate::airlayer_compat;
use crate::catalog::Catalog;

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
