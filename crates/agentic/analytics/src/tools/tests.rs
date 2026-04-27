//! Integration tests for the full `tools::*` public API.
//!
//! Kept as a single module so tool-scoping tests can mix calls across all
//! executor/spec modules without cross-module reimports.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentic_connector::{DatabaseConnector, SchemaInfo};
use agentic_core::result::CellValue;
use agentic_core::tools::{ToolDef, ToolError};
use serde_json::json;

use crate::SchemaCatalog;
use crate::types::ChartConfig;
use agentic_llm::validate_openai_strict_schema;

use super::{
    clarifying_tools, execute_clarifying_tool, execute_database_lookup_tool,
    execute_interpreting_tool, execute_specifying_tool, interpreting_tools, new_schema_cache,
    solving_tools, specifying_tools, suggest_chart_config, validate_chart_column_types,
};

// ── OpenAI strict-mode compliance ─────────────────────────────────────────

/// Every tool schema is sent to OpenAI with `"strict": true`.
/// That mode requires every key in `properties` to also appear in
/// `required`.  This test catches violations at compile time rather than
/// at runtime when the HTTP call fails with an opaque 400.
#[test]
fn all_tool_schemas_are_openai_strict_compatible() {
    let all: Vec<ToolDef> = clarifying_tools(false)
        .into_iter()
        .chain(specifying_tools(false))
        .chain(solving_tools())
        .chain(interpreting_tools())
        .collect();

    for tool in &all {
        let violations = validate_openai_strict_schema(&tool.parameters, tool.name);
        assert!(
            violations.is_empty(),
            "tool '{}' violates OpenAI strict mode:\n  {}",
            tool.name,
            violations.join("\n  ")
        );
    }
}

fn make_catalog() -> SchemaCatalog {
    SchemaCatalog::new()
        .add_table("orders", &["order_id", "customer_id", "revenue", "date"])
        .add_table("customers", &["customer_id", "region", "name"])
        .add_join_key("orders", "customers", "customer_id")
}

// ── Tool scoping ──────────────────────────────────────────────────────────

#[test]
fn clarifying_does_not_include_solving_tools() {
    let tools = clarifying_tools(false);
    let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
    assert!(
        !names.contains(&"execute_preview"),
        "execute_preview must not appear in clarifying"
    );
    assert!(
        !names.contains(&"update_chart_config"),
        "update_chart_config must not appear in clarifying"
    );
}

#[test]
fn solving_does_not_include_clarifying_tools() {
    let tools = solving_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
    assert!(
        !names.contains(&"search_catalog"),
        "search_catalog must not appear in solving"
    );
}

#[test]
fn specifying_does_not_include_solving_tools() {
    let tools = specifying_tools(false);
    let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
    assert!(!names.contains(&"execute_preview"));
    assert!(!names.contains(&"update_chart_config"));
}

#[test]
fn specifying_contains_expected_tools() {
    let names: Vec<&str> = specifying_tools(false).iter().map(|t| t.name).collect();
    assert!(names.contains(&"get_join_path"));
    assert!(names.contains(&"sample_columns"));
    // Catalog discovery tools moved from clarifying to specifying.
    assert!(names.contains(&"search_catalog"));
}

#[test]
fn solving_contains_execute_preview() {
    let names: Vec<&str> = solving_tools().iter().map(|t| t.name).collect();
    assert!(names.contains(&"execute_preview"));
}

// ── Clarifying tool execution ─────────────────────────────────────────────

#[test]
fn search_catalog_finds_metrics_and_dimensions() {
    let cat = make_catalog();
    let result = execute_clarifying_tool(
        "search_catalog",
        serde_json::json!({ "queries": ["revenue"] }),
        &cat,
    )
    .unwrap();
    let metrics = result["metrics"].as_array().unwrap();
    assert!(
        !metrics.is_empty(),
        "revenue should match at least one metric"
    );
    assert!(
        metrics
            .iter()
            .any(|m| m["name"].as_str().unwrap_or("").contains("revenue"))
    );
    let dims = result["dimensions"].as_array().unwrap();
    assert!(!dims.is_empty(), "matched metric should have dimensions");
}

#[test]
fn search_catalog_empty_query_returns_all() {
    let cat = make_catalog();
    let result = execute_clarifying_tool(
        "search_catalog",
        serde_json::json!({ "queries": [""] }),
        &cat,
    )
    .unwrap();
    let metrics = result["metrics"].as_array().unwrap();
    // orders has `revenue`; that's at least 1 metric
    assert!(!metrics.is_empty());
}

// ── Specifying tool execution ─────────────────────────────────────────────

/// Noop connector used in specifying tests that call `get_join_path`
/// (which never touches the connector).
struct NoopConnector;

#[async_trait::async_trait]
impl DatabaseConnector for NoopConnector {
    fn dialect(&self) -> agentic_connector::SqlDialect {
        agentic_connector::SqlDialect::DuckDb
    }

    async fn execute_query(
        &self,
        _sql: &str,
        _limit: u64,
    ) -> Result<agentic_connector::ExecutionResult, agentic_connector::ConnectorError> {
        panic!("NoopConnector: execute_query must not be called in this test")
    }
}

fn noop_connectors() -> std::collections::HashMap<String, std::sync::Arc<dyn DatabaseConnector>> {
    let mut map: std::collections::HashMap<String, std::sync::Arc<dyn DatabaseConnector>> =
        std::collections::HashMap::new();
    map.insert("default".to_string(), std::sync::Arc::new(NoopConnector));
    map
}

#[tokio::test]
async fn get_join_path_known_pair() {
    let cat = make_catalog();
    let conns = noop_connectors();
    let result = execute_specifying_tool(
        "get_join_path",
        serde_json::json!({ "from_entity": "orders", "to_entity": "customers" }),
        &cat,
        &conns,
        "default",
    )
    .await
    .unwrap();
    assert!(result["path"].as_str().unwrap().contains("customer_id"));
    assert_eq!(result["join_type"], "INNER");
}

#[tokio::test]
async fn get_join_path_unknown_pair_returns_error() {
    let cat = make_catalog();
    let conns = noop_connectors();
    let err = execute_specifying_tool(
        "get_join_path",
        serde_json::json!({ "from_entity": "orders", "to_entity": "products" }),
        &cat,
        &conns,
        "default",
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ToolError::Execution(_)));
}

// ── Interpreting tool execution ───────────────────────────────────────────

#[tokio::test]
async fn render_chart_emits_event_and_returns_ok() {
    let cols = vec!["region".to_string(), "revenue".to_string()];
    let rows: Vec<Vec<serde_json::Value>> = vec![];
    let result_sets = vec![(cols, rows)];
    let valid_charts = Arc::new(Mutex::new(Vec::new()));
    let result = execute_interpreting_tool(
        "render_chart",
        serde_json::json!({
            "chart_type": "bar_chart",
            "x": "region",
            "y": "revenue",
            "series": null,
            "name": null,
            "value": null,
            "x_axis_label": "Region",
            "y_axis_label": "Revenue",
            "result_index": null,
            "title": "Revenue by Region"
        }),
        &None,
        &result_sets,
        &valid_charts,
    )
    .await
    .unwrap();
    assert_eq!(result["ok"], true);
}

// ── suggest_chart_config ─────────────────────────────────────────────────

#[test]
fn suggest_trend_produces_line_chart() {
    let cols = vec!["date".to_string(), "revenue".to_string()];
    let cfg = suggest_chart_config(&crate::types::QuestionType::Trend, &cols).unwrap();
    assert_eq!(cfg.chart_type, "line_chart");
    assert_eq!(cfg.x.as_deref(), Some("date"));
    assert_eq!(cfg.y.as_deref(), Some("revenue"));
}

#[test]
fn suggest_breakdown_produces_bar_chart() {
    let cols = vec!["region".to_string(), "revenue".to_string()];
    let cfg = suggest_chart_config(&crate::types::QuestionType::Breakdown, &cols).unwrap();
    assert_eq!(cfg.chart_type, "bar_chart");
}

#[test]
fn suggest_single_value_returns_none() {
    let cols = vec!["total".to_string()];
    assert!(suggest_chart_config(&crate::types::QuestionType::SingleValue, &cols).is_none());
}

#[test]
fn suggest_fewer_than_two_columns_returns_none() {
    let cols = vec!["total".to_string()];
    assert!(suggest_chart_config(&crate::types::QuestionType::Breakdown, &cols).is_none());
}

// ── validate_chart_column_types ───────────────────────────────────────────

#[test]
fn column_type_check_passes_for_numeric_y() {
    let config = ChartConfig {
        chart_type: "bar_chart".to_string(),
        x: Some("region".to_string()),
        y: Some("revenue".to_string()),
        series: None,
        name: None,
        value: None,
        title: None,
        x_axis_label: None,
        y_axis_label: None,
    };
    let columns = vec!["region".to_string(), "revenue".to_string()];
    let rows = vec![vec![serde_json::json!("North"), serde_json::json!(42000.0)]];
    let errors = validate_chart_column_types(&config, &columns, &rows);
    assert!(
        errors.is_empty(),
        "numeric y column should pass: {errors:?}"
    );
}

#[test]
fn column_type_check_fails_for_stringified_numeric_y() {
    // Regression: to_2d_array stringifies Arrow numeric columns; the chart
    // renderer receives "42000.0" (a JSON string) instead of 42000.0.
    let config = ChartConfig {
        chart_type: "bar_chart".to_string(),
        x: Some("region".to_string()),
        y: Some("revenue".to_string()),
        series: None,
        name: None,
        value: None,
        title: None,
        x_axis_label: None,
        y_axis_label: None,
    };
    let columns = vec!["region".to_string(), "revenue".to_string()];
    let rows = vec![
        // Both values are JSON strings — simulates the stringification bug.
        vec![serde_json::json!("North"), serde_json::json!("42000.0")],
    ];
    let errors = validate_chart_column_types(&config, &columns, &rows);
    assert!(
        !errors.is_empty(),
        "stringified y column should produce an error"
    );
    assert!(errors[0].contains("revenue"));
}

#[test]
fn column_type_check_passes_for_pie_numeric_value() {
    let config = ChartConfig {
        chart_type: "pie_chart".to_string(),
        x: None,
        y: None,
        series: None,
        name: Some("category".to_string()),
        value: Some("share".to_string()),
        title: None,
        x_axis_label: None,
        y_axis_label: None,
    };
    let columns = vec!["category".to_string(), "share".to_string()];
    let rows = vec![vec![serde_json::json!("A"), serde_json::json!(0.4)]];
    assert!(validate_chart_column_types(&config, &columns, &rows).is_empty());
}

#[test]
fn column_type_check_skipped_when_no_rows() {
    let config = ChartConfig {
        chart_type: "bar_chart".to_string(),
        x: Some("region".to_string()),
        y: Some("revenue".to_string()),
        series: None,
        name: None,
        value: None,
        title: None,
        x_axis_label: None,
        y_axis_label: None,
    };
    let columns = vec!["region".to_string(), "revenue".to_string()];
    // Empty result set — no rows to inspect, should not error.
    let errors = validate_chart_column_types(&config, &columns, &[]);
    assert!(errors.is_empty());
}

// ── Unknown tools return ToolError::UnknownTool ───────────────────────────

#[test]
fn unknown_tool_in_clarifying_returns_error() {
    let cat = make_catalog();
    let err = execute_clarifying_tool(
        "explain_plan",
        serde_json::json!({ "sql": "SELECT 1" }),
        &cat,
    )
    .unwrap_err();
    assert!(matches!(err, ToolError::UnknownTool(_)));
}

// ── Database lookup tools ─────────────────────────────────────────────────

#[test]
fn list_tables_tool_in_clarifying_and_specifying() {
    let clar_names: Vec<&str> = clarifying_tools(false).iter().map(|t| t.name).collect();
    let spec_names: Vec<&str> = specifying_tools(false).iter().map(|t| t.name).collect();
    assert!(clar_names.contains(&"list_tables"));
    assert!(spec_names.contains(&"list_tables"));
}

#[test]
fn describe_table_tool_in_clarifying_and_specifying() {
    let clar_names: Vec<&str> = clarifying_tools(false).iter().map(|t| t.name).collect();
    let spec_names: Vec<&str> = specifying_tools(false).iter().map(|t| t.name).collect();
    assert!(clar_names.contains(&"describe_table"));
    assert!(spec_names.contains(&"describe_table"));
}

#[test]
fn list_tables_not_in_solving() {
    let names: Vec<&str> = solving_tools().iter().map(|t| t.name).collect();
    assert!(!names.contains(&"list_tables"));
    assert!(!names.contains(&"describe_table"));
}

#[test]
fn db_tools_excluded_when_has_semantic() {
    let clar = clarifying_tools(true);
    let spec = specifying_tools(true);
    let clar_names: Vec<&str> = clar.iter().map(|t| t.name).collect();
    let spec_names: Vec<&str> = spec.iter().map(|t| t.name).collect();
    assert!(!clar_names.contains(&"list_tables"));
    assert!(!clar_names.contains(&"describe_table"));
    assert!(!spec_names.contains(&"list_tables"));
    assert!(!spec_names.contains(&"describe_table"));
    // Core tools remain present.
    assert!(clar_names.contains(&"search_catalog"));
    assert!(spec_names.contains(&"sample_columns"));
}

/// Stub connector that returns a fixed schema for introspection.
struct IntrospectableStub {
    schema: SchemaInfo,
    call_count: std::sync::atomic::AtomicUsize,
}

impl IntrospectableStub {
    fn new(schema: SchemaInfo) -> Self {
        Self {
            schema,
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl DatabaseConnector for IntrospectableStub {
    fn dialect(&self) -> agentic_connector::SqlDialect {
        agentic_connector::SqlDialect::DuckDb
    }

    async fn execute_query(
        &self,
        _sql: &str,
        _limit: u64,
    ) -> Result<agentic_connector::ExecutionResult, agentic_connector::ConnectorError> {
        panic!("IntrospectableStub: execute_query must not be called")
    }

    fn introspect_schema(
        &self,
    ) -> Result<agentic_connector::SchemaInfo, agentic_connector::ConnectorError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(self.schema.clone())
    }
}

fn sample_schema() -> SchemaInfo {
    use agentic_connector::{SchemaColumnInfo, SchemaTableInfo};
    SchemaInfo {
        tables: vec![
            SchemaTableInfo {
                name: "orders".to_string(),
                columns: vec![
                    SchemaColumnInfo {
                        name: "order_id".to_string(),
                        data_type: "INTEGER".to_string(),
                        min: None,
                        max: None,
                        sample_values: vec![],
                    },
                    SchemaColumnInfo {
                        name: "revenue".to_string(),
                        data_type: "DOUBLE".to_string(),
                        min: None,
                        max: None,
                        sample_values: vec![CellValue::Number(100.0)],
                    },
                ],
            },
            SchemaTableInfo {
                name: "customers".to_string(),
                columns: vec![SchemaColumnInfo {
                    name: "name".to_string(),
                    data_type: "VARCHAR".to_string(),
                    min: None,
                    max: None,
                    sample_values: vec![CellValue::Text("Alice".to_string())],
                }],
            },
        ],
        join_keys: vec![],
    }
}

fn make_connectors(stub: Arc<IntrospectableStub>) -> HashMap<String, Arc<dyn DatabaseConnector>> {
    let mut map: HashMap<String, Arc<dyn DatabaseConnector>> = HashMap::new();
    map.insert("default".to_string(), stub);
    map
}

#[tokio::test]
async fn list_tables_returns_table_names() {
    let stub = Arc::new(IntrospectableStub::new(sample_schema()));
    let connectors = make_connectors(stub);
    let cache = new_schema_cache();
    let result = execute_database_lookup_tool(
        "list_tables",
        json!({ "database": null }),
        &connectors,
        "default",
        &cache,
    )
    .await
    .unwrap();
    let tables = result["tables"].as_array().unwrap();
    assert_eq!(tables.len(), 2);
    let names: Vec<&str> = tables.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"orders"));
    assert!(names.contains(&"customers"));
}

#[tokio::test]
async fn describe_table_returns_columns() {
    let stub = Arc::new(IntrospectableStub::new(sample_schema()));
    let connectors = make_connectors(stub);
    let cache = new_schema_cache();
    let result = execute_database_lookup_tool(
        "describe_table",
        json!({ "table": "orders", "database": null }),
        &connectors,
        "default",
        &cache,
    )
    .await
    .unwrap();
    assert_eq!(result["table"], "orders");
    let cols = result["columns"].as_array().unwrap();
    assert_eq!(cols.len(), 2);
    assert_eq!(cols[0]["name"], "order_id");
    assert_eq!(cols[0]["data_type"], "INTEGER");
    assert_eq!(cols[1]["name"], "revenue");
    // Check sample values are included.
    let samples = cols[1]["sample_values"].as_array().unwrap();
    assert!(!samples.is_empty());
}

#[tokio::test]
async fn describe_table_unknown_returns_error() {
    let stub = Arc::new(IntrospectableStub::new(sample_schema()));
    let connectors = make_connectors(stub);
    let cache = new_schema_cache();
    let err = execute_database_lookup_tool(
        "describe_table",
        json!({ "table": "nonexistent", "database": null }),
        &connectors,
        "default",
        &cache,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ToolError::Execution(_)));
}

#[tokio::test]
async fn list_tables_caches_schema() {
    let stub = Arc::new(IntrospectableStub::new(sample_schema()));
    let connectors = make_connectors(Arc::clone(&stub));
    let cache = new_schema_cache();
    // First call populates cache.
    execute_database_lookup_tool(
        "list_tables",
        json!({ "database": null }),
        &connectors,
        "default",
        &cache,
    )
    .await
    .unwrap();
    assert_eq!(stub.calls(), 1);
    // Second call uses cache.
    execute_database_lookup_tool(
        "list_tables",
        json!({ "database": null }),
        &connectors,
        "default",
        &cache,
    )
    .await
    .unwrap();
    assert_eq!(
        stub.calls(),
        1,
        "introspect_schema should be called only once"
    );
}

#[tokio::test]
async fn describe_table_case_insensitive() {
    let stub = Arc::new(IntrospectableStub::new(sample_schema()));
    let connectors = make_connectors(stub);
    let cache = new_schema_cache();
    let result = execute_database_lookup_tool(
        "describe_table",
        json!({ "table": "ORDERS", "database": null }),
        &connectors,
        "default",
        &cache,
    )
    .await
    .unwrap();
    assert_eq!(result["table"], "orders");
}
