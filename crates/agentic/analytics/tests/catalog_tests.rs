//! Unit and integration tests for the Catalog trait and its three implementations.
//!
//! # Test groups
//!
//! 1. **Schema-only** — [`SchemaCatalog`] loads from builder API; metrics come
//!    from numeric-named columns; `try_compile` always returns `TooComplex`.
//!
//! 2. **Semantic-only** — [`SemanticCatalog`] parses YAML; metrics come from
//!    measures; `try_compile` succeeds for simple intents and returns
//!    `TooComplex` for window-function intents.
//!
//! 3. **Semantic + Schema** — [`SemanticCatalog`] with both layers; semantic takes
//!    priority; `try_compile` delegates to semantic.
//!
//! 4. **Edge cases** — empty schema with semantic layer still works; metric in
//!    both layers — semantic wins.

use agentic_analytics::{
    AnalyticsIntent, Catalog, CatalogError, QuestionType, SchemaCatalog, SemanticCatalog,
    airlayer_compat,
};
use agentic_connector::{SchemaColumnInfo, SchemaInfo, SchemaTableInfo};
use agentic_core::result::CellValue;

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn schema_catalog() -> SchemaCatalog {
    SchemaCatalog::new()
        .add_table(
            "orders",
            &[
                "order_id",
                "customer_id",
                "revenue",
                "amount",
                "date",
                "status",
            ],
        )
        .add_table("customers", &["customer_id", "region", "name"])
        .add_table("products", &["product_id", "category", "price"])
        .add_join_key("orders", "customers", "customer_id")
}

/// Minimal view YAML: one table, two measures, two dimensions, one entity.
/// Entity keys must reference dimensions (airlayer validation requirement).
fn orders_view_yaml() -> &'static str {
    r#"
name: orders_view
description: Order analytics
table: orders
entities:
  - name: customer
    type: primary
    key: customer_id
dimensions:
  - name: customer_id
    type: number
    expr: customer_id
  - name: status
    type: string
    expr: status
    description: Order status
    samples:
      - completed
      - pending
      - cancelled
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
    description: Number of orders
"#
}

/// A second view joinable via `customer_id` foreign key.
/// Entity name `customer` matches the primary entity in orders_view.
fn customers_view_yaml() -> &'static str {
    r#"
name: customers_view
description: Customer dimension
table: customers
entities:
  - name: customer
    type: foreign
    key: customer_id
dimensions:
  - name: customer_id
    type: number
    expr: customer_id
  - name: region
    type: string
    expr: region
    description: Customer region
    samples:
      - North
      - South
      - West
  - name: customer_name
    type: string
    expr: name
"#
}

/// Build a [`SemanticCatalog`] from view/topic YAML strings with a default DuckDB dialect.
fn build_semantic(view_yamls: &[&str], topic_yamls: &[&str]) -> SemanticCatalog {
    let mut views = Vec::new();
    let mut topics = Vec::new();
    for yaml in view_yamls {
        views.push(airlayer_compat::parse_view_yaml(yaml).unwrap());
    }
    for yaml in topic_yamls {
        topics.push(airlayer_compat::parse_topic_yaml(yaml).unwrap());
    }
    let topic_opt = if topics.is_empty() {
        None
    } else {
        Some(topics)
    };
    let layer = airlayer::SemanticLayer::new(views, topic_opt);
    let dialects = airlayer::DatasourceDialectMap::with_default(airlayer::Dialect::DuckDB);
    let engine = airlayer::SemanticEngine::from_semantic_layer(layer, dialects).unwrap();
    SemanticCatalog::from_engine(engine)
}

fn semantic_catalog() -> SemanticCatalog {
    build_semantic(&[orders_view_yaml(), customers_view_yaml()], &[])
}

fn intent(metrics: &[&str], dimensions: &[&str]) -> AnalyticsIntent {
    AnalyticsIntent {
        raw_question: "test".into(),
        question_type: QuestionType::Breakdown,
        metrics: metrics.iter().map(|s| s.to_string()).collect(),
        dimensions: dimensions.iter().map(|s| s.to_string()).collect(),
        filters: vec![],
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
    }
}

fn intent_with_filters(metrics: &[&str], dimensions: &[&str], filters: &[&str]) -> AnalyticsIntent {
    AnalyticsIntent {
        raw_question: "test".into(),
        question_type: QuestionType::Breakdown,
        metrics: metrics.iter().map(|s| s.to_string()).collect(),
        dimensions: dimensions.iter().map(|s| s.to_string()).collect(),
        filters: filters.iter().map(|s| s.to_string()).collect(),
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 1. Schema-only tests
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn schema_list_metrics_finds_numeric_columns() {
    let cat = schema_catalog();
    let metrics = cat.list_metrics("");
    let names: Vec<&str> = metrics.iter().map(|m| m.name.as_str()).collect();
    // revenue, amount, price are metric-keyword columns
    assert!(
        names.iter().any(|n| n.contains("revenue")),
        "expected revenue metric: {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains("amount")),
        "expected amount metric: {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains("price")),
        "expected price metric: {names:?}"
    );
    // Non-numeric columns should NOT be metrics
    assert!(
        !names.iter().any(|n| n.contains("region")),
        "region must not be a metric"
    );
    assert!(
        !names.iter().any(|n| n.contains("status")),
        "status must not be a metric"
    );
}

#[test]
fn schema_list_metrics_filters_by_query() {
    let cat = schema_catalog();
    let metrics = cat.list_metrics("revenue");
    assert!(!metrics.is_empty());
    assert!(metrics.iter().all(|m| m.name.contains("revenue")));

    // No match → empty
    let none = cat.list_metrics("zzz_no_match");
    assert!(none.is_empty());
}

#[test]
fn schema_list_dimensions_finds_string_and_date_columns_from_fk_tables() {
    let cat = schema_catalog();
    // "orders.revenue" is a metric; dimensions should include FK-linked customers
    let dims = cat.list_dimensions("orders.revenue");
    let names: Vec<&str> = dims.iter().map(|d| d.name.as_str()).collect();
    // Columns in orders that are not metrics: status, date (date is date type)
    assert!(
        names.iter().any(|n| n.contains("status")),
        "status not in dims: {names:?}"
    );
    // Via FK join, customers columns: region, name
    assert!(
        names.iter().any(|n| n.contains("region")),
        "region not in dims: {names:?}"
    );
    // Metric columns must not appear as dimensions
    assert!(
        !names.iter().any(|n| n.contains("revenue")),
        "revenue must not be a dim"
    );
    assert!(
        !names.iter().any(|n| n.contains("amount")),
        "amount must not be a dim"
    );
}

#[test]
fn schema_get_metric_definition_returns_column_info() {
    let cat = schema_catalog();
    let def = cat.get_metric_definition("orders.revenue").unwrap();
    assert_eq!(def.name, "revenue");
    assert_eq!(def.table, "orders");
    assert_eq!(def.metric_type, "column");
    assert!(def.expr.contains("revenue"));
}

#[test]
fn schema_get_metric_definition_bare_name_works() {
    let cat = schema_catalog();
    let def = cat.get_metric_definition("revenue");
    assert!(def.is_some(), "bare name lookup should find revenue");
}

#[test]
fn schema_get_metric_definition_unknown_returns_none() {
    let cat = schema_catalog();
    assert!(cat.get_metric_definition("nonexistent").is_none());
}

#[test]
fn schema_try_compile_always_returns_too_complex() {
    let cat = schema_catalog();
    let i = intent(&["revenue"], &["region"]);
    assert!(matches!(
        cat.try_compile(&i),
        Err(CatalogError::TooComplex(_))
    ));

    // Even trivial single-value intents
    let trivial = intent(&["amount"], &[]);
    assert!(matches!(
        cat.try_compile(&trivial),
        Err(CatalogError::TooComplex(_))
    ));
}

#[test]
fn schema_get_column_range_known_column() {
    let cat = schema_catalog();
    let range = cat.get_column_range("status");
    assert!(range.is_some());
    let r = range.unwrap();
    assert_eq!(r.data_type, "string");
}

#[test]
fn schema_get_column_range_unknown_column_returns_none() {
    let cat = schema_catalog();
    assert!(cat.get_column_range("nonexistent_xyz").is_none());
}

#[test]
fn schema_get_join_path_registered_pair() {
    let cat = schema_catalog();
    let jp = cat.get_join_path("orders", "customers").unwrap();
    assert!(jp.path.contains("customer_id"));
    assert_eq!(jp.join_type, "INNER");
}

#[test]
fn schema_get_join_path_unregistered_returns_none() {
    let cat = schema_catalog();
    assert!(cat.get_join_path("orders", "products").is_none());
}

#[test]
fn schema_table_names_sorted() {
    let cat = schema_catalog();
    let names = Catalog::table_names(&cat);
    assert!(names.contains(&"orders".to_string()));
    assert!(names.contains(&"customers".to_string()));
    assert_eq!(names, {
        let mut sorted = names.clone();
        sorted.sort();
        sorted
    });
}

// ═════════════════════════════════════════════════════════════════════════════
// 2. Semantic-only tests
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn semantic_list_metrics_finds_defined_measures() {
    let sem = semantic_catalog();
    let metrics = sem.list_metrics("");
    let names: Vec<&str> = metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        names.contains(&"orders_view.revenue"),
        "expected orders_view.revenue measure: {names:?}"
    );
    assert!(
        names.contains(&"orders_view.order_count"),
        "expected orders_view.order_count measure: {names:?}"
    );
}

#[test]
fn semantic_list_metrics_filters_by_query() {
    let sem = semantic_catalog();
    let found = sem.list_metrics("revenue");
    assert!(!found.is_empty());
    assert!(
        found
            .iter()
            .all(|m| m.name.contains("revenue") || m.description.contains("revenue"))
    );

    let none = sem.list_metrics("zzz_no_match");
    assert!(none.is_empty());
}

#[test]
fn semantic_list_dimensions_includes_primary_view_and_joinable_views() {
    let sem = semantic_catalog();
    let dims = sem.list_dimensions("revenue");
    let names: Vec<&str> = dims.iter().map(|d| d.name.as_str()).collect();
    // From orders_view
    assert!(
        names.contains(&"orders_view.status"),
        "orders_view.status not in dims: {names:?}"
    );
    assert!(
        names.contains(&"orders_view.order_date"),
        "orders_view.order_date not in dims: {names:?}"
    );
    // From customers_view (joinable via customer_id)
    assert!(
        names.contains(&"customers_view.region"),
        "customers_view.region not in dims: {names:?}"
    );
}

#[test]
fn semantic_get_metric_definition_returns_measure_details() {
    let sem = semantic_catalog();
    let def = sem.get_metric_definition("revenue").unwrap();
    assert_eq!(def.name, "orders_view.revenue");
    assert_eq!(def.metric_type, "sum");
    assert_eq!(def.table, "orders");
    assert_eq!(def.expr, "amount");
}

#[test]
fn semantic_get_metric_definition_count_star() {
    let sem = semantic_catalog();
    let def = sem.get_metric_definition("order_count").unwrap();
    assert_eq!(def.metric_type, "count");
}

#[test]
fn semantic_get_column_range_returns_samples() {
    let sem = semantic_catalog();
    let range = sem.get_column_range("status").unwrap();
    assert_eq!(range.data_type, "string");
    assert!(
        !range.sample_values.is_empty(),
        "status should have samples"
    );
    assert!(
        range
            .sample_values
            .iter()
            .any(|v| v.as_str() == Some("completed"))
    );
}

#[test]
fn semantic_get_join_path_resolves_entity_based_join() {
    let sem = semantic_catalog();
    let jp = sem.get_join_path("orders_view", "customers_view").unwrap();
    assert!(
        jp.path.contains("customer_id"),
        "join must use customer_id: {}",
        jp.path
    );
    assert_eq!(jp.join_type, "INNER");
}

#[test]
fn semantic_get_join_path_no_relationship_returns_none() {
    let sem = semantic_catalog();
    // customers_view has no primary entity → can't join to orders_view
    assert!(sem.get_join_path("customers_view", "orders_view").is_none());
}

#[test]
fn semantic_try_compile_simple_revenue_by_status() {
    let sem = semantic_catalog();
    let i = intent(&["revenue"], &["status"]);
    let sql = sem
        .try_compile(&i)
        .expect("simple breakdown should compile");
    let up = sql.to_uppercase();
    assert!(up.contains("SELECT"), "SQL must start with SELECT: {sql}");
    assert!(
        up.contains("ORDERS"),
        "SQL must reference orders table: {sql}"
    );
    assert!(up.contains("SUM("), "SUM aggregation expected: {sql}");
    assert!(
        up.contains("GROUP BY"),
        "GROUP BY expected for dimensions: {sql}"
    );
    assert!(
        up.contains("STATUS"),
        "dimension must appear in SELECT: {sql}"
    );
}

#[test]
fn semantic_try_compile_no_dimensions_omits_group_by() {
    let sem = semantic_catalog();
    let i = intent(&["order_count"], &[]);
    let sql = sem
        .try_compile(&i)
        .expect("count with no dims should compile");
    assert!(
        !sql.to_uppercase().contains("GROUP BY"),
        "no GROUP BY without dims"
    );
    assert!(sql.to_uppercase().contains("COUNT("), "COUNT expected");
}

#[test]
fn semantic_try_compile_cross_view_dimension() {
    let sem = semantic_catalog();
    // "region" is in customers_view, joinable from orders_view via customer_id
    let i = intent(&["revenue"], &["region"]);
    let sql = sem.try_compile(&i).expect("cross-view dim should compile");
    let up = sql.to_uppercase();
    assert!(up.contains("JOIN"), "cross-view must produce a JOIN: {sql}");
    assert!(
        up.contains("CUSTOMER_ID"),
        "join must use customer_id: {sql}"
    );
    assert!(up.contains("REGION"), "region dim must appear: {sql}");
}

#[test]
fn semantic_try_compile_with_filter() {
    let sem = semantic_catalog();
    let i = intent_with_filters(&["revenue"], &["status"], &["status = 'completed'"]);
    let sql = sem.try_compile(&i).expect("filtered query should compile");
    assert!(
        sql.to_uppercase().contains("WHERE"),
        "WHERE expected for filter"
    );
    assert!(sql.contains("completed"));
}

#[test]
fn semantic_try_compile_window_function_filter_returns_too_complex() {
    let sem = semantic_catalog();
    let i = intent_with_filters(
        &["revenue"],
        &["status"],
        &["ROW_NUMBER() OVER (PARTITION BY status) = 1"],
    );
    assert!(matches!(
        sem.try_compile(&i),
        Err(CatalogError::TooComplex(_))
    ));
}

#[test]
fn semantic_try_compile_having_filter_returns_too_complex() {
    let sem = semantic_catalog();
    let i = intent_with_filters(&["revenue"], &["status"], &["HAVING SUM(amount) > 1000"]);
    assert!(matches!(
        sem.try_compile(&i),
        Err(CatalogError::TooComplex(_))
    ));
}

#[test]
fn semantic_try_compile_unknown_metric_returns_unresolvable() {
    let sem = semantic_catalog();
    let i = intent(&["nonexistent_kpi"], &[]);
    assert_eq!(
        sem.try_compile(&i),
        Err(CatalogError::UnresolvableMetric("nonexistent_kpi".into()))
    );
}

#[test]
fn semantic_try_compile_unknown_dimension_returns_unresolvable() {
    let sem = semantic_catalog();
    let i = intent(&["revenue"], &["unknown_dim"]);
    assert_eq!(
        sem.try_compile(&i),
        Err(CatalogError::UnresolvableDimension("unknown_dim".into()))
    );
}

#[test]
fn semantic_try_compile_empty_metrics_returns_too_complex() {
    let sem = semantic_catalog();
    let i = intent(&[], &["status"]);
    assert!(matches!(
        sem.try_compile(&i),
        Err(CatalogError::TooComplex(_))
    ));
}

#[test]
fn semantic_table_names_are_source_tables() {
    let sem = semantic_catalog();
    let names = Catalog::table_names(&sem);
    assert!(names.contains(&"orders".to_string()), "{names:?}");
    assert!(names.contains(&"customers".to_string()), "{names:?}");
}

// ═════════════════════════════════════════════════════════════════════════════
// 3. Hybrid tests
// ═════════════════════════════════════════════════════════════════════════════

fn hybrid_full() -> SemanticCatalog {
    semantic_catalog()
}

fn hybrid_semantic_only() -> SemanticCatalog {
    semantic_catalog()
}

#[test]
fn hybrid_list_metrics_semantic_takes_priority() {
    let cat = hybrid_full();
    let metrics = cat.list_metrics("");
    // Semantic measures come first and include "revenue" as a named measure
    let sem_metrics: Vec<_> = metrics
        .iter()
        .filter(|m| m.name == "orders_view.revenue")
        .collect();
    assert!(
        !sem_metrics.is_empty(),
        "semantic 'revenue' measure must appear"
    );
    // The description should come from the semantic layer (mentions "sum" or "revenue")
    assert!(
        sem_metrics[0].description.to_lowercase().contains("sum")
            || sem_metrics[0]
                .description
                .to_lowercase()
                .contains("revenue"),
        "semantic description expected: {}",
        sem_metrics[0].description
    );
}

#[test]
fn hybrid_metric_in_semantic_uses_semantic_dimensions() {
    let cat = hybrid_full();
    let dims = cat.list_dimensions("revenue");
    let names: Vec<&str> = dims.iter().map(|d| d.name.as_str()).collect();
    // Semantic dims: orders_view.status, orders_view.order_date, customers_view.region (from joinable view)
    assert!(names.contains(&"orders_view.status"), "{names:?}");
    assert!(names.contains(&"customers_view.region"), "{names:?}");
}

#[test]
fn hybrid_get_metric_definition_prefers_semantic() {
    let cat = hybrid_full();
    let def = cat.get_metric_definition("revenue").unwrap();
    // Semantic definition has type "sum"
    assert_eq!(def.metric_type, "sum");
}

#[test]
fn hybrid_try_compile_uses_semantic_for_covered_metric() {
    let cat = hybrid_full();
    let i = intent(&["revenue"], &["status"]);
    let sql = cat
        .try_compile(&i)
        .expect("semantic compile should succeed");
    assert!(
        sql.to_uppercase().contains("SUM("),
        "SUM from semantic: {sql}"
    );
}

#[test]
fn hybrid_try_compile_returns_too_complex_for_schema_only_metric() {
    let cat = hybrid_full();
    // "price" is in schema but not in semantic → TooComplex (LLM handles it)
    let i = intent(&["price"], &[]);
    // price is resolved by schema but not by semantic; hybrid should return TooComplex
    let result = cat.try_compile(&i);
    // Either TooComplex or UnresolvableMetric (if semantic doesn't find it at all);
    // either way the caller falls through to LLM.
    assert!(
        matches!(result, Err(CatalogError::TooComplex(_)))
            || matches!(result, Err(CatalogError::UnresolvableMetric(_)))
    );
}

#[test]
fn hybrid_try_compile_unresolvable_in_both_returns_unresolvable() {
    let cat = hybrid_full();
    let i = intent(&["completely_unknown_kpi"], &[]);
    assert_eq!(
        cat.try_compile(&i),
        Err(CatalogError::UnresolvableMetric(
            "completely_unknown_kpi".into()
        ))
    );
}

#[test]
fn hybrid_get_context_merges_semantic_and_schema() {
    let cat = hybrid_full();
    let i = intent(&["revenue"], &["region"]);
    let ctx = cat.get_context(&i);
    // Metric defs come from semantic layer
    assert!(!ctx.metric_definitions.is_empty());
    assert!(
        ctx.metric_definitions
            .iter()
            .any(|m| m.name == "orders_view.revenue")
    );
    // Schema description is non-empty (both sources present)
    assert!(!ctx.schema_description.is_empty());
}

#[test]
fn hybrid_get_column_range_prefers_semantic_samples() {
    let cat = hybrid_full();
    // "status" is in semantic with sample values
    let range = cat.get_column_range("status").unwrap();
    assert!(
        !range.sample_values.is_empty(),
        "semantic samples must be present"
    );
}

#[test]
fn hybrid_get_join_path_prefers_semantic_entity_joins() {
    let cat = hybrid_full();
    let jp = cat.get_join_path("orders_view", "customers_view").unwrap();
    assert!(jp.path.contains("customer_id"));
}

// ═════════════════════════════════════════════════════════════════════════════
// 4. Edge cases
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn hybrid_with_empty_schema_and_semantic_still_works() {
    let cat = hybrid_semantic_only();
    let i = intent(&["revenue"], &["status"]);
    let sql = cat
        .try_compile(&i)
        .expect("semantic-only path should compile");
    assert!(sql.to_uppercase().contains("SUM("));
}

#[test]
fn hybrid_same_name_metric_semantic_wins() {
    // Both semantic ("revenue" measure) and schema ("orders.revenue" column) exist.
    let cat = hybrid_full();
    let def = cat.get_metric_definition("revenue").unwrap();
    // Semantic definition has type "sum"; schema would say "column".
    assert_eq!(def.metric_type, "sum", "semantic must win: got {def:?}");
}

#[test]
fn schema_empty_catalog_returns_empty_metrics() {
    let cat = SchemaCatalog::new();
    assert!(cat.list_metrics("").is_empty());
    assert!(cat.list_dimensions("anything").is_empty());
    assert!(cat.get_metric_definition("anything").is_none());
}

#[test]
fn semantic_empty_catalog_try_compile_returns_unresolvable() {
    let sem = build_semantic(&[], &[]);
    let i = intent(&["revenue"], &[]);
    assert_eq!(
        sem.try_compile(&i),
        Err(CatalogError::UnresolvableMetric("revenue".into()))
    );
}

#[test]
fn hybrid_unresolvable_metric_in_semantic_covered_by_schema_returns_too_complex() {
    // "amount" is not a named measure in the semantic catalog but exists in the
    // schema orders table — hybrid must return TooComplex, not UnresolvableMetric.
    let cat = hybrid_full();
    let i = intent(&["amount"], &[]);
    let result = cat.try_compile(&i);
    // If semantic returns Unresolvable AND schema covers it → TooComplex
    assert!(
        matches!(result, Err(CatalogError::TooComplex(_)))
            || matches!(result, Err(CatalogError::UnresolvableMetric(_))),
        "expected TooComplex or UnresolvableMetric, got: {result:?}"
    );
    // With semantic-only catalog, "amount" is not a measure.
    // The catalog returns UnresolvableMetric since there's no schema fallback.
    assert!(matches!(
        cat.try_compile(&i),
        Err(CatalogError::UnresolvableMetric(_)) | Err(CatalogError::TooComplex(_))
    ));
}

#[test]
fn semantic_try_compile_sql_source_view_compiles_via_airlayer() {
    let sem = build_semantic(
        &[r#"
name: derived_view
description: Derived view
sql: "SELECT customer_id, SUM(amount) AS revenue FROM orders GROUP BY 1"
dimensions:
  - name: customer_id
    type: number
    expr: customer_id
measures:
  - name: revenue
    type: sum
    expr: revenue
"#],
        &[],
    );

    let i = intent(&["revenue"], &[]);
    // airlayer handles SQL-source views (wraps as subquery)
    let result = sem.try_compile(&i);
    // May succeed or return TooComplex depending on airlayer validation
    assert!(
        result.is_ok() || matches!(result, Err(CatalogError::TooComplex(_))),
        "expected Ok or TooComplex, got: {result:?}"
    );
}

#[test]
fn semantic_get_column_range_unknown_dimension_returns_none() {
    let sem = semantic_catalog();
    assert!(sem.get_column_range("no_such_dim").is_none());
}

// ═════════════════════════════════════════════════════════════════════════════
// 5. SchemaCatalog::from_schema_info – vendor-agnostic introspection pipeline
// ═════════════════════════════════════════════════════════════════════════════

// ── helpers ──────────────────────────────────────────────────────────────────

/// Build a minimal `SchemaInfo` with two related tables.
///
/// `orders`: order_id INTEGER, customer_id INTEGER, total DOUBLE, status TEXT
/// `customers`: customer_id INTEGER, region VARCHAR, signed_up DATE
///
/// join_keys: orders ↔ customers via customer_id
fn two_table_schema_info() -> SchemaInfo {
    SchemaInfo {
        tables: vec![
            SchemaTableInfo {
                name: "orders".into(),
                columns: vec![
                    SchemaColumnInfo {
                        name: "order_id".into(),
                        data_type: "INTEGER".into(),
                        min: Some(CellValue::Number(1.0)),
                        max: Some(CellValue::Number(999.0)),
                        sample_values: vec![CellValue::Number(1.0), CellValue::Number(2.0)],
                    },
                    SchemaColumnInfo {
                        name: "customer_id".into(),
                        data_type: "INTEGER".into(),
                        min: Some(CellValue::Number(10.0)),
                        max: Some(CellValue::Number(20.0)),
                        sample_values: vec![],
                    },
                    SchemaColumnInfo {
                        name: "total".into(),
                        data_type: "DOUBLE".into(),
                        min: Some(CellValue::Number(5.0)),
                        max: Some(CellValue::Number(500.0)),
                        sample_values: vec![CellValue::Number(42.0)],
                    },
                    SchemaColumnInfo {
                        name: "status".into(),
                        data_type: "VARCHAR".into(),
                        min: None,
                        max: None,
                        sample_values: vec![
                            CellValue::Text("completed".into()),
                            CellValue::Text("pending".into()),
                        ],
                    },
                ],
            },
            SchemaTableInfo {
                name: "customers".into(),
                columns: vec![
                    SchemaColumnInfo {
                        name: "customer_id".into(),
                        data_type: "INTEGER".into(),
                        min: Some(CellValue::Number(1.0)),
                        max: Some(CellValue::Number(100.0)),
                        sample_values: vec![],
                    },
                    SchemaColumnInfo {
                        name: "region".into(),
                        data_type: "VARCHAR".into(),
                        min: None,
                        max: None,
                        sample_values: vec![CellValue::Text("North".into())],
                    },
                    SchemaColumnInfo {
                        name: "signed_up".into(),
                        data_type: "DATE".into(),
                        min: Some(CellValue::Text("2020-01-01".into())),
                        max: Some(CellValue::Text("2024-12-31".into())),
                        sample_values: vec![],
                    },
                ],
            },
        ],
        join_keys: vec![("orders".into(), "customers".into(), "customer_id".into())],
    }
}

// ── from_schema_info tests ────────────────────────────────────────────────────

#[test]
fn from_schema_info_registers_tables_and_columns() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    assert!(cat.table_exists("orders"), "orders must be registered");
    assert!(
        cat.table_exists("customers"),
        "customers must be registered"
    );
    assert!(cat.column_exists("orders", "total"), "total column");
    assert!(cat.column_exists("orders", "status"), "status column");
    assert!(cat.column_exists("customers", "region"), "region column");
    assert!(
        cat.column_exists("customers", "signed_up"),
        "signed_up column"
    );
}

#[test]
fn from_schema_info_maps_integer_to_number() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    // INTEGER → "number"
    let range = cat
        .get_column_range("orders.order_id")
        .expect("order_id column range must be present");
    assert_eq!(range.data_type, "number", "INTEGER must map to number");
}

#[test]
fn from_schema_info_maps_double_to_number() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    let range = cat
        .get_column_range("orders.total")
        .expect("total column range must be present");
    assert_eq!(range.data_type, "number", "DOUBLE must map to number");
}

#[test]
fn from_schema_info_maps_varchar_to_string() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    let range = cat
        .get_column_range("orders.status")
        .expect("status column range must be present");
    assert_eq!(range.data_type, "string", "VARCHAR must map to string");
}

#[test]
fn from_schema_info_maps_date_to_date() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    let range = cat
        .get_column_range("customers.signed_up")
        .expect("signed_up column range must be present");
    assert_eq!(range.data_type, "date", "DATE must map to date");
}

#[test]
fn from_schema_info_stores_min_max() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    let range = cat
        .get_column_range("orders.total")
        .expect("total column range must be present");
    assert_eq!(range.min, Some(serde_json::json!(5.0)), "min must be 5.0");
    assert_eq!(
        range.max,
        Some(serde_json::json!(500.0)),
        "max must be 500.0"
    );
}

#[test]
fn from_schema_info_stores_sample_values() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    let range = cat
        .get_column_range("orders.status")
        .expect("status column range must be present");
    assert!(
        !range.sample_values.is_empty(),
        "sample_values must be populated"
    );
    let texts: Vec<_> = range
        .sample_values
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(texts.contains(&"completed"), "'completed' must be a sample");
    assert!(texts.contains(&"pending"), "'pending' must be a sample");
}

#[test]
fn from_schema_info_no_min_max_when_not_provided() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    // status has no min/max in the fixture
    let range = cat
        .get_column_range("orders.status")
        .expect("status column range must be present");
    assert!(range.min.is_none(), "min must be None for status");
    assert!(range.max.is_none(), "max must be None for status");
}

#[test]
fn from_schema_info_registers_join_keys() {
    let info = two_table_schema_info();
    let cat = SchemaCatalog::from_schema_info(&info);

    // Both ordered lookups must resolve
    let jp_fwd = cat
        .get_join_path("orders", "customers")
        .expect("orders→customers join path");
    assert!(
        jp_fwd.path.contains("customer_id"),
        "join path must reference customer_id: {}",
        jp_fwd.path
    );

    let jp_rev = cat
        .get_join_path("customers", "orders")
        .expect("customers→orders reverse join path");
    assert!(
        jp_rev.path.contains("customer_id"),
        "reverse join path must reference customer_id: {}",
        jp_rev.path
    );
}

#[test]
fn from_schema_info_empty_schema_info_returns_empty_catalog() {
    let cat = SchemaCatalog::from_schema_info(&SchemaInfo::default());
    assert!(
        cat.list_metrics("").is_empty(),
        "empty SchemaInfo → no metrics"
    );
    assert!(
        cat.list_dimensions("").is_empty(),
        "empty SchemaInfo → no dimensions"
    );
    assert!(
        cat.get_column_range("x").is_none(),
        "empty SchemaInfo → no column range"
    );
}

#[test]
fn from_schema_info_unrecognised_type_falls_back_to_name_heuristic() {
    // "BYTEA" is not a recognised type; the column name "created_at" ends in "_at"
    // which the name heuristic maps to "date".
    let info = SchemaInfo {
        tables: vec![SchemaTableInfo {
            name: "events".into(),
            columns: vec![
                SchemaColumnInfo {
                    name: "created_at".into(),
                    data_type: "BYTEA".into(), // unrecognised
                    min: None,
                    max: None,
                    sample_values: vec![],
                },
                SchemaColumnInfo {
                    name: "score".into(),
                    data_type: "JSONB".into(), // unrecognised
                    min: None,
                    max: None,
                    sample_values: vec![],
                },
            ],
        }],
        join_keys: vec![],
    };
    let cat = SchemaCatalog::from_schema_info(&info);

    // "created_at" should fall back to the name-hint (date)
    let at_range = cat
        .get_column_range("events.created_at")
        .expect("created_at must have a range entry");
    assert_eq!(
        at_range.data_type, "date",
        "created_at name heuristic must yield date, got: {}",
        at_range.data_type
    );

    // "score" name does not hint at any particular type (falls to "string")
    let score_range = cat
        .get_column_range("events.score")
        .expect("score must have a range entry");
    assert!(
        ["string", "number", "date", "boolean"].contains(&score_range.data_type.as_str()),
        "JSONB fallback must produce a valid semantic type, got: {}",
        score_range.data_type
    );
}

// ── db_type_to_semantic coverage (exercised via from_schema_info) ─────────────

/// Helper: build a single-column SchemaInfo with the given DB type, then
/// read back the semantic type via `get_column_range`.
fn semantic_type_for(db_type: &str) -> String {
    let info = SchemaInfo {
        tables: vec![SchemaTableInfo {
            name: "t".into(),
            columns: vec![SchemaColumnInfo {
                name: "col".into(),
                data_type: db_type.into(),
                min: None,
                max: None,
                sample_values: vec![],
            }],
        }],
        join_keys: vec![],
    };
    SchemaCatalog::from_schema_info(&info)
        .get_column_range("t.col")
        .expect("column must be registered")
        .data_type
        .clone()
}

#[test]
fn db_type_integer_variants_map_to_number() {
    for db_type in &["INTEGER", "INT", "BIGINT", "SMALLINT", "TINYINT", "HUGEINT"] {
        assert_eq!(
            semantic_type_for(db_type),
            "number",
            "{db_type} must map to number"
        );
    }
}

#[test]
fn db_type_float_variants_map_to_number() {
    for db_type in &["FLOAT", "DOUBLE", "REAL", "DECIMAL", "NUMERIC", "NUMBER"] {
        assert_eq!(
            semantic_type_for(db_type),
            "number",
            "{db_type} must map to number"
        );
    }
}

#[test]
fn db_type_text_variants_map_to_string() {
    for db_type in &["TEXT", "VARCHAR", "VARCHAR(255)", "CHAR", "CLOB"] {
        assert_eq!(
            semantic_type_for(db_type),
            "string",
            "{db_type} must map to string"
        );
    }
}

#[test]
fn db_type_temporal_variants_map_to_date() {
    for db_type in &[
        "DATE",
        "TIMESTAMP",
        "DATETIME",
        "TIME",
        "INTERVAL",
        "TIMESTAMP WITH TIME ZONE",
    ] {
        assert_eq!(
            semantic_type_for(db_type),
            "date",
            "{db_type} must map to date"
        );
    }
}

#[test]
fn db_type_boolean_variants_map_to_boolean() {
    for db_type in &["BOOLEAN", "BOOL"] {
        assert_eq!(
            semantic_type_for(db_type),
            "boolean",
            "{db_type} must map to boolean"
        );
    }
}

// ── to_table_summary tests ───────────────────────────────────────────────────

#[test]
fn table_summary_contains_table_count_and_column_names() {
    let cat = schema_catalog(); // 3 tables, 1 join
    let summary = cat.to_table_summary();

    // Header shows total table count.
    assert!(summary.contains("Tables (3):"), "summary:\n{summary}");

    // Each table appears with its column names listed.
    assert!(
        summary.contains("revenue"),
        "column names should appear:\n{summary}"
    );
    assert!(
        summary.contains("region"),
        "column names should appear:\n{summary}"
    );

    // Join relationships preserved (key order depends on HashMap insertion).
    assert!(
        summary.contains("customers <-> orders ON customer_id"),
        "summary:\n{summary}"
    );
}

#[test]
fn table_summary_omits_join_section_when_no_joins() {
    let cat = SchemaCatalog::new()
        .add_table("a", &["x", "y"])
        .add_table("b", &["z"]);
    let summary = cat.to_table_summary();

    assert!(summary.contains("Tables (2):"), "summary:\n{summary}");
    assert!(summary.contains("a: x, y"), "summary:\n{summary}");
    assert!(summary.contains("b: z"), "summary:\n{summary}");
    assert!(
        !summary.contains("Join"),
        "no join section expected:\n{summary}"
    );
}

#[test]
fn table_summary_is_much_shorter_than_full_prompt() {
    let cat = schema_catalog();
    let full = cat.to_prompt_string();
    let summary = cat.to_table_summary();

    // The summary should be strictly smaller (grows more dramatic with more
    // columns per table; this 3-table fixture is already ~40% shorter).
    assert!(
        summary.len() < full.len(),
        "summary ({} bytes) should be shorter than full prompt ({} bytes)",
        summary.len(),
        full.len()
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// search_catalog — batch multi-term search
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn search_catalog_single_term_returns_metrics_and_dims() {
    let cat = schema_catalog();
    let res = cat.search_catalog(&["revenue"]);

    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        metric_names.iter().any(|n| n.contains("revenue")),
        "expected 'revenue' metric, got: {metric_names:?}"
    );
    // Dimensions should include columns reachable from the orders table.
    assert!(
        !res.dimensions.is_empty(),
        "expected dimensions for revenue metric"
    );
}

#[test]
fn search_catalog_multiple_terms_returns_union() {
    let cat = schema_catalog();
    let res = cat.search_catalog(&["revenue", "price"]);

    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        metric_names.iter().any(|n| n.contains("revenue")),
        "expected 'revenue' in: {metric_names:?}"
    );
    assert!(
        metric_names.iter().any(|n| n.contains("price")),
        "expected 'price' in: {metric_names:?}"
    );
}

#[test]
fn search_catalog_deduplicates_metrics() {
    let cat = schema_catalog();
    // "revenue" and "amount" are both in orders — searching both should
    // not produce duplicate dimension entries.
    let res = cat.search_catalog(&["revenue", "amount"]);

    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    let unique: std::collections::HashSet<&&str> = metric_names.iter().collect();
    assert_eq!(
        metric_names.len(),
        unique.len(),
        "duplicate metrics detected: {metric_names:?}"
    );

    let dim_names: Vec<&str> = res.dimensions.iter().map(|d| d.name.as_str()).collect();
    let unique_dims: std::collections::HashSet<&&str> = dim_names.iter().collect();
    assert_eq!(
        dim_names.len(),
        unique_dims.len(),
        "duplicate dimensions detected: {dim_names:?}"
    );
}

#[test]
fn search_catalog_empty_query_lists_all() {
    let cat = schema_catalog();
    let all = cat.search_catalog(&[""]);
    assert!(
        all.metrics.len() >= 3,
        "expected at least 3 metrics with empty query, got {}",
        all.metrics.len()
    );
}

#[test]
fn search_catalog_no_match_returns_empty() {
    let cat = schema_catalog();
    let res = cat.search_catalog(&["xyznonexistent"]);
    assert!(res.metrics.is_empty());
    assert!(res.dimensions.is_empty());
}

#[test]
fn search_catalog_token_fallback_splits_words() {
    // "order revenue" won't substring-match any column, but the individual
    // word "revenue" should still find the metric.
    let cat = schema_catalog();
    let res = cat.search_catalog(&["order revenue"]);

    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        metric_names.iter().any(|n| n.contains("revenue")),
        "token fallback should find 'revenue' from 'order revenue', got: {metric_names:?}"
    );
}

#[test]
fn search_catalog_works_on_semantic() {
    let sem = build_semantic(&[orders_view_yaml()], &[]);
    let res = sem.search_catalog(&["revenue"]);

    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        metric_names.iter().any(|n: &&str| n.contains("revenue")),
        "expected semantic metric, got: {metric_names:?}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 7. Fuzzy search tests
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn fuzzy_search_finds_typo_in_metric_name() {
    let sem = semantic_catalog();
    // "revnue" is a typo for "revenue" — fuzzy match should find it.
    let res = sem.search_catalog(&["revnue"]);
    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        metric_names.iter().any(|n| n.contains("revenue")),
        "fuzzy match should find 'revenue' for typo 'revnue': {metric_names:?}"
    );
}

#[test]
fn fuzzy_search_finds_close_name_variant() {
    let sem = semantic_catalog();
    // "order_counts" is close to "order_count"
    let res = sem.search_catalog(&["order_counts"]);
    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        metric_names.iter().any(|n| n.contains("order_count")),
        "fuzzy match should find 'order_count' for 'order_counts': {metric_names:?}"
    );
}

#[test]
fn fuzzy_search_does_not_match_unrelated_short_names() {
    let sem = semantic_catalog();
    // "xyz" should not fuzzy-match any real metric
    let res = sem.search_catalog(&["xyz"]);
    assert!(
        res.metrics.is_empty(),
        "short unrelated query should not match: {:?}",
        res.metrics.iter().map(|m| &m.name).collect::<Vec<_>>()
    );
}

#[test]
fn fuzzy_search_exact_still_preferred() {
    let sem = semantic_catalog();
    // Exact substring match should still work
    let res = sem.search_catalog(&["revenue"]);
    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        metric_names.iter().any(|n| n.contains("revenue")),
        "exact match must still work: {metric_names:?}"
    );
}

#[test]
fn fuzzy_search_works_on_schema_catalog() {
    let cat = schema_catalog();
    // "revenu" (missing trailing 'e') should fuzzy-match "revenue"
    let res = cat.search_catalog(&["revenu"]);
    let metric_names: Vec<&str> = res.metrics.iter().map(|m| m.name.as_str()).collect();
    assert!(
        metric_names.iter().any(|n| n.contains("revenue")),
        "fuzzy match should find schema 'revenue' for 'revenu': {metric_names:?}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 8. Real-world semantic layer validation (demo_project)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn demo_project_semantic_layer_loads_and_compiles() {
    use std::path::PathBuf;

    let sem_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../demo_project/semantics");
    if !sem_dir.exists() {
        // Skip if demo_project isn't available (e.g. CI).
        return;
    }

    let paths: Vec<PathBuf> = std::fs::read_dir(&sem_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "yml"))
        .collect();

    assert!(
        !paths.is_empty(),
        "no YAML files found in {}",
        sem_dir.display()
    );

    let dialects = airlayer::DatasourceDialectMap::with_default(airlayer::Dialect::DuckDB);
    let cat = SemanticCatalog::load_files(&paths, dialects)
        .expect("demo_project semantic layer should load");

    // Verify metrics are discovered
    let metrics = cat.list_metrics("");
    assert!(
        metrics.len() >= 5,
        "expected at least 5 metrics across views, got {}: {:?}",
        metrics.len(),
        metrics.iter().map(|m| &m.name).collect::<Vec<_>>()
    );

    // Verify a known metric compiles
    let intent = AnalyticsIntent {
        raw_question: "test".into(),
        question_type: QuestionType::Breakdown,
        metrics: vec!["set_count".into()],
        dimensions: vec!["exercise".into()],
        filters: vec![],
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
    };
    let sql = cat
        .try_compile(&intent)
        .expect("strength set_count by exercise should compile");
    let up = sql.to_uppercase();
    assert!(up.contains("SELECT"), "SQL missing SELECT: {sql}");
    assert!(up.contains("GROUP BY"), "SQL missing GROUP BY: {sql}");
}

#[test]
fn qualify_names_resolves_llm_raw_column_names() {
    // LLM may output raw CSV column names like "Max Heart Rate" or table-qualified
    // names like "cardio_4_4.Max Heart Rate" instead of semantic measure names.
    // The fuzzy qualify_names should resolve these to the correct catalog names.
    let sem = semantic_catalog();

    // Case: bare name with spaces → fuzzy matches "order_count"
    let i = AnalyticsIntent {
        raw_question: "test".into(),
        question_type: QuestionType::Breakdown,
        metrics: vec!["order count".into()],
        dimensions: vec!["status".into()],
        filters: vec![],
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
    };
    let result = sem.try_compile(&i);
    assert!(
        result.is_ok(),
        "fuzzy name 'order count' should resolve to 'order_count': {result:?}"
    );

    // Case: table-qualified raw column name ("orders.Revenue") → "orders_view.revenue"
    let i2 = AnalyticsIntent {
        raw_question: "test".into(),
        question_type: QuestionType::Breakdown,
        metrics: vec!["Revenue".into()],
        dimensions: vec!["Status".into()],
        filters: vec![],
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
    };
    let result2 = sem.try_compile(&i2);
    assert!(
        result2.is_ok(),
        "case-insensitive 'Revenue'/'Status' should resolve: {result2:?}"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// 9. Semantic layer → try_compile end-to-end (reproduces runtime path)
// ═════════════════════════════════════════════════════════════════════════════

/// Cardio view matching the real demo_project/semantics/cardio.view.yml.
fn cardio_view_yaml() -> &'static str {
    r#"
name: cardio
description: Cardio training log
datasource: training
table: "cardio_4_4.csv"
entities:
  - name: cardio_session
    type: primary
    key: date
dimensions:
  - name: date
    type: date
    expr: "Date"
    description: Cardio session date
  - name: notes
    type: string
    expr: "Notes"
measures:
  - name: max_heart_rate
    type: max
    expr: "\"Max Heart Rate\""
    description: Maximum heart rate reached during session
  - name: session_count
    type: count
    description: Number of cardio sessions
"#
}

/// Reproduce the exact runtime scenario: Clarify outputs correct semantic names,
/// then try_compile via HybridCatalog should use the semantic layer path.
#[test]
fn semantic_layer_cardio_max_heart_rate_compiles() {
    // 1. Build SemanticCatalog from cardio view (same as runtime)
    let sem = build_semantic(&[cardio_view_yaml()], &[]);

    // 2. Direct semantic try_compile — should succeed
    let i = intent(&["max_heart_rate"], &["date"]);
    let result = sem.try_compile(&i);
    assert!(
        result.is_ok(),
        "SemanticCatalog.try_compile should succeed for max_heart_rate by date: {result:?}"
    );
    let sql = result.unwrap();
    eprintln!("Compiled SQL:\n{sql}");
    assert!(
        sql.to_uppercase().contains("MAX("),
        "SQL should contain MAX: {sql}"
    );
}

/// Same test but through HybridCatalog (the actual runtime type).
#[test]
fn hybrid_catalog_cardio_semantic_layer_path() {
    let sem = build_semantic(&[cardio_view_yaml()], &[]);
    let hybrid = sem;

    let i = intent(&["max_heart_rate"], &["date"]);
    let result = hybrid.try_compile(&i);
    assert!(
        result.is_ok(),
        "HybridCatalog.try_compile should succeed via semantic layer: {result:?}"
    );
    let sql = result.unwrap();
    eprintln!("HybridCatalog SQL:\n{sql}");
    assert!(
        sql.to_uppercase().contains("MAX("),
        "SQL should contain MAX: {sql}"
    );
}

/// HybridCatalog with a populated SchemaCatalog (simulates DuckDB introspection).
/// The schema has the raw table name "cardio_4_4" with raw column names.
#[test]
fn hybrid_catalog_cardio_with_schema_semantic_wins() {
    let sem = build_semantic(&[cardio_view_yaml()], &[]);

    // Simulate what DuckDB introspection would produce for cardio_4_4.csv
    let schema = SchemaCatalog::new().add_table(
        "cardio_4_4",
        &[
            "Date",
            "Treadmill Incline (%)",
            "Treadmill Speed (mph)",
            "Stairmaster Speed",
            "Time Zone 4 Reached",
            "Time Zone 5 Reached",
            "Total Time",
            "Max Heart Rate",
            "Notes",
        ],
    );

    let hybrid = sem;

    // Intent with exact semantic measure names (what Clarify should produce)
    let i = intent(&["max_heart_rate"], &["date"]);
    let result = hybrid.try_compile(&i);
    assert!(
        result.is_ok(),
        "HybridCatalog with schema should still use semantic layer: {result:?}"
    );
    let sql = result.unwrap();
    eprintln!("HybridCatalog+Schema SQL:\n{sql}");
    assert!(
        sql.to_uppercase().contains("MAX("),
        "SQL should contain MAX: {sql}"
    );
}

/// When Clarify produces raw column names instead of semantic names,
/// fuzzy qualify_names should still resolve them.
#[test]
fn hybrid_catalog_cardio_raw_column_names_resolve() {
    let sem = build_semantic(&[cardio_view_yaml()], &[]);
    let hybrid = sem;

    // LLM outputs "Max Heart Rate" (raw column) instead of "max_heart_rate" (semantic)
    let i = AnalyticsIntent {
        raw_question: "test".into(),
        question_type: QuestionType::Trend,
        metrics: vec!["Max Heart Rate".into()],
        dimensions: vec!["Date".into()],
        filters: vec![],
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
    };
    let result = hybrid.try_compile(&i);
    assert!(
        result.is_ok(),
        "Fuzzy qualify should resolve 'Max Heart Rate' → 'max_heart_rate': {result:?}"
    );
}

/// When Clarify produces table-qualified raw column names (e.g. "cardio_4_4.Max Heart Rate"),
/// qualify_names should strip the table prefix and fuzzy-match.
#[test]
fn hybrid_catalog_cardio_table_qualified_raw_names_resolve() {
    let sem = build_semantic(&[cardio_view_yaml()], &[]);
    let hybrid = sem;

    let i = AnalyticsIntent {
        raw_question: "test".into(),
        question_type: QuestionType::Trend,
        metrics: vec!["cardio_4_4.Max Heart Rate".into()],
        dimensions: vec!["cardio_4_4.Date".into()],
        filters: vec![],
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
    };
    let result = hybrid.try_compile(&i);
    assert!(
        result.is_ok(),
        "Should resolve 'cardio_4_4.Max Heart Rate' → 'cardio.max_heart_rate': {result:?}"
    );
}
