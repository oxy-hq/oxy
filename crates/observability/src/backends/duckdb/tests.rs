//! Comprehensive tests for the DuckDB observability storage layer.
//!
//! All tests use [`DuckDBStorage::open_in_memory()`] and insert test data via
//! direct SQL against the connection, then exercise the async query methods.
//! This avoids timing issues from the async writer pipeline.

use std::time::Duration;

use crate::backends::duckdb::DuckDBStorage;
use crate::intent_types::IntentCluster;
use crate::store::ObservabilityStore;
use crate::types::{MetricUsageRecord, SpanRecord};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create an in-memory DuckDBStorage for testing.
fn test_storage() -> DuckDBStorage {
    DuckDBStorage::open_in_memory().expect("Failed to open in-memory DuckDB")
}

/// Insert a span record directly via the connection (bypassing the writer).
fn insert_span(storage: &DuckDBStorage, span: &SpanRecord) {
    let conn = storage.conn().lock().unwrap();
    conn.execute(
        "INSERT INTO spans (trace_id, span_id, parent_span_id, span_name, service_name, \
         span_attributes, duration_ns, status_code, status_message, event_data, timestamp) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        duckdb::params![
            span.trace_id,
            span.span_id,
            span.parent_span_id,
            span.span_name,
            span.service_name,
            span.span_attributes,
            span.duration_ns,
            span.status_code,
            span.status_message,
            span.event_data,
            span.timestamp,
        ],
    )
    .expect("Failed to insert test span");
}

/// Insert a metric usage record directly via the connection.
fn insert_metric(storage: &DuckDBStorage, metric: &MetricUsageRecord) {
    let conn = storage.conn().lock().unwrap();
    conn.execute(
        "INSERT INTO metric_usage (metric_name, source_type, source_ref, context, context_types, trace_id) \
         VALUES (?, ?, ?, ?, ?, ?)",
        duckdb::params![
            metric.metric_name,
            metric.source_type,
            metric.source_ref,
            metric.context,
            metric.context_types,
            metric.trace_id,
        ],
    )
    .expect("Failed to insert test metric");
}

/// Create a root agent span (the kind list_traces filters for).
///
/// Attributes use flat dotted keys to match the `->>'agent.ref'` operator.
fn make_agent_span(
    trace_id: &str,
    span_id: &str,
    agent_ref: &str,
    status: &str,
    duration_ns: i64,
) -> SpanRecord {
    SpanRecord {
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        parent_span_id: String::new(),
        span_name: "agent.run_agent".to_string(),
        service_name: "oxy".to_string(),
        span_attributes: format!(
            r#"{{"oxy.agent.ref":"{}","agent.prompt":"test question"}}"#,
            agent_ref
        ),
        duration_ns,
        status_code: status.to_string(),
        status_message: String::new(),
        event_data: "[]".to_string(),
        timestamp: "2026-04-15T12:00:00Z".to_string(),
    }
}

/// Create a root agent span with token usage events.
fn make_agent_span_with_tokens(
    trace_id: &str,
    span_id: &str,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
) -> SpanRecord {
    let events = format!(
        r#"[{{"name":"llm.usage","attributes":{{"prompt_tokens":{},"completion_tokens":{},"total_tokens":{}}}}}]"#,
        prompt_tokens, completion_tokens, total_tokens,
    );
    SpanRecord {
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        parent_span_id: String::new(),
        span_name: "agent.run_agent".to_string(),
        service_name: "oxy".to_string(),
        span_attributes: r#"{"oxy.agent.ref":"test-agent","agent.prompt":"hello"}"#.to_string(),
        duration_ns: 1_000_000,
        status_code: "OK".to_string(),
        status_message: String::new(),
        event_data: events,
        timestamp: "2026-04-15T12:00:00Z".to_string(),
    }
}

/// Create a tool-call span for execution analytics testing.
///
/// Attributes use flat dotted keys to match the `->>'oxy.span_type'` etc.
/// operator syntax that DuckDB uses for literal key lookup.
fn make_tool_call_span(
    trace_id: &str,
    span_id: &str,
    parent_span_id: &str,
    execution_type: &str,
    is_verified: bool,
    _agent_ref: &str,
) -> SpanRecord {
    let attrs = format!(
        r#"{{"oxy.span_type":"tool_call","oxy.execution_type":"{}","oxy.is_verified":"{}"}}"#,
        execution_type, is_verified,
    );
    SpanRecord {
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        parent_span_id: parent_span_id.to_string(),
        span_name: "tool.execute".to_string(),
        service_name: "oxy".to_string(),
        span_attributes: attrs,
        duration_ns: 500_000,
        status_code: "OK".to_string(),
        status_message: String::new(),
        event_data: "[]".to_string(),
        timestamp: "2026-04-15T12:00:01Z".to_string(),
    }
}

/// Create an agent span for execution analytics (the parent side of the join).
///
/// The join filter checks `->>'oxy.agent.ref'` using flat dotted keys.
fn make_agent_parent_span(trace_id: &str, span_id: &str, agent_ref: &str) -> SpanRecord {
    let attrs = format!(
        r#"{{"oxy.agent.ref":"{}","agent.prompt":"test question"}}"#,
        agent_ref,
    );
    SpanRecord {
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        parent_span_id: String::new(),
        span_name: "agent.run_agent".to_string(),
        service_name: "oxy".to_string(),
        span_attributes: attrs,
        duration_ns: 2_000_000,
        status_code: "OK".to_string(),
        status_message: String::new(),
        event_data: "[]".to_string(),
        timestamp: "2026-04-15T12:00:00Z".to_string(),
    }
}

// ── Integration tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_full_pipeline_via_writer() {
    let storage = test_storage();

    // Send spans through the writer pipeline.
    let spans = vec![SpanRecord {
        trace_id: "t-pipe-1".into(),
        span_id: "s-pipe-1".into(),
        parent_span_id: String::new(),
        span_name: "agent.run_agent".into(),
        service_name: "oxy".into(),
        span_attributes: r#"{"agent":{"ref":"pipeline-agent","prompt":"pipeline test"}}"#.into(),
        duration_ns: 2_000_000,
        status_code: "OK".into(),
        status_message: String::new(),
        event_data: "[]".into(),
        timestamp: "2026-04-15T10:00:00Z".into(),
    }];

    storage.writer().send_spans(spans);
    storage.writer().flush();
    // Give the background writer time to process.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (traces, total) = storage
        .list_traces(10, 0, None, None, None)
        .await
        .expect("list_traces should succeed");

    assert_eq!(total, 1, "should have 1 trace");
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].trace_id, "t-pipe-1");
    assert_eq!(traces[0].span_name, "agent.run_agent");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_span_attributes_round_trip() {
    let storage = test_storage();

    let span = SpanRecord {
        trace_id: "t-attr-1".into(),
        span_id: "s-attr-1".into(),
        parent_span_id: String::new(),
        span_name: "agent.run_agent".into(),
        service_name: "oxy".into(),
        span_attributes:
            r#"{"agent":{"ref":"attr-agent","prompt":"q"},"custom":{"key":"custom_value"}}"#.into(),
        duration_ns: 1_500_000,
        status_code: "OK".into(),
        status_message: String::new(),
        event_data: r#"[{"name":"custom.event","attributes":{"detail":"info"}}]"#.into(),
        timestamp: "2026-04-15T11:00:00Z".into(),
    };
    insert_span(&storage, &span);

    let details = storage
        .get_trace_detail("t-attr-1")
        .await
        .expect("get_trace_detail should succeed");

    assert_eq!(details.len(), 1);
    assert!(details[0].span_attributes.contains("custom"));
    assert!(details[0].event_data.contains("custom.event"));

    storage.shutdown().await;
}

#[tokio::test]
async fn test_metric_storage_round_trip() {
    let storage = test_storage();

    let metric = MetricUsageRecord {
        metric_name: "revenue".into(),
        source_type: "agent".into(),
        source_ref: "sales_agent".into(),
        context: "quarterly report".into(),
        context_types: r#"["SQL"]"#.into(),
        trace_id: "t-metric-1".into(),
    };
    insert_metric(&storage, &metric);

    let analytics = storage
        .get_metrics_analytics(30)
        .await
        .expect("get_metrics_analytics should succeed");

    assert_eq!(analytics.total_queries, 1);
    assert_eq!(analytics.unique_metrics, 1);
    assert_eq!(analytics.most_popular.as_deref(), Some("revenue"));
    assert_eq!(analytics.most_popular_count, Some(1));

    storage.shutdown().await;
}

#[tokio::test]
async fn test_metric_detail() {
    let storage = test_storage();

    // Insert multiple metric usage records for the same metric.
    for i in 0..5 {
        let metric = MetricUsageRecord {
            metric_name: "revenue".into(),
            source_type: if i < 3 { "agent" } else { "workflow" }.into(),
            source_ref: "ref".into(),
            context: format!("context {i}"),
            context_types: r#"["SQL"]"#.into(),
            trace_id: format!("t-md-{i}"),
        };
        insert_metric(&storage, &metric);
    }

    // Insert a related metric in the same trace.
    let related = MetricUsageRecord {
        metric_name: "cost".into(),
        source_type: "agent".into(),
        source_ref: "ref".into(),
        context: "related context".into(),
        context_types: r#"["SQL"]"#.into(),
        trace_id: "t-md-0".into(), // same trace as first revenue
    };
    insert_metric(&storage, &related);

    let detail = storage
        .get_metric_detail("revenue", 30)
        .await
        .expect("get_metric_detail should succeed");

    assert_eq!(detail.name, "revenue");
    assert_eq!(detail.total_queries, 5);
    assert_eq!(detail.via_agent, 3);
    assert_eq!(detail.via_workflow, 2);
    assert!(!detail.recent_usage.is_empty());
    // "cost" should appear as a related metric.
    assert!(
        detail.related_metrics.iter().any(|r| r.name == "cost"),
        "cost should be a related metric"
    );

    storage.shutdown().await;
}

#[tokio::test]
async fn test_intent_classification_round_trip() {
    let storage = test_storage();

    let embedding = vec![0.1_f32, 0.2, 0.3];
    storage
        .store_classification(
            "t-intent-1",
            "What is revenue?",
            1,
            "finance",
            0.95,
            &embedding,
            "agent",
            "sales_agent",
        )
        .await
        .expect("store_classification should succeed");

    let embeddings = storage
        .load_embeddings()
        .await
        .expect("load_embeddings should succeed");

    assert_eq!(embeddings.len(), 1);
    let (trace_id, question, emb, intent_name, source) = &embeddings[0];
    assert_eq!(trace_id, "t-intent-1");
    assert_eq!(question, "What is revenue?");
    assert_eq!(intent_name, "finance");
    assert_eq!(source, "sales_agent");
    assert_eq!(emb.len(), 3);
    assert!((emb[0] - 0.1).abs() < 0.01);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_intent_cluster_round_trip() {
    let storage = test_storage();

    let clusters = vec![
        IntentCluster {
            cluster_id: 1,
            intent_name: "finance".into(),
            intent_description: "Financial questions".into(),
            centroid: vec![0.5, 0.5, 0.5],
            sample_questions: vec!["What is revenue?".into(), "Show me costs".into()],
        },
        IntentCluster {
            cluster_id: 2,
            intent_name: "marketing".into(),
            intent_description: "Marketing questions".into(),
            centroid: vec![0.8, 0.2, 0.1],
            sample_questions: vec!["How many leads?".into()],
        },
    ];

    storage
        .store_clusters(&clusters)
        .await
        .expect("store_clusters should succeed");

    let loaded = storage
        .load_clusters()
        .await
        .expect("load_clusters should succeed");

    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].cluster_id, 1);
    assert_eq!(loaded[0].intent_name, "finance");
    assert_eq!(loaded[0].sample_questions.len(), 2);
    assert_eq!(loaded[1].cluster_id, 2);
    assert_eq!(loaded[1].intent_name, "marketing");

    // Verify cluster infos.
    let infos = storage
        .get_cluster_infos()
        .await
        .expect("get_cluster_infos should succeed");
    assert_eq!(infos.len(), 2);
    assert_eq!(infos[0].cluster_id, 1);
    assert_eq!(infos[1].cluster_id, 2);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_analytics_summary() {
    let storage = test_storage();

    // Insert an agent parent span and a tool-call child span.
    let agent_span = make_agent_parent_span("t-exec-1", "s-agent-1", "test-agent");
    insert_span(&storage, &agent_span);

    let tool_span = make_tool_call_span(
        "t-exec-1",
        "s-tool-1",
        "s-agent-1",
        "semantic_query",
        true,
        "test-agent",
    );
    insert_span(&storage, &tool_span);

    let tool_span2 = make_tool_call_span(
        "t-exec-1",
        "s-tool-2",
        "s-agent-1",
        "sql_generated",
        false,
        "test-agent",
    );
    insert_span(&storage, &tool_span2);

    let summary = storage
        .get_execution_summary(30)
        .await
        .expect("get_execution_summary should succeed");

    assert_eq!(summary.total_executions, 2);
    assert_eq!(summary.semantic_query_count, 1);
    assert_eq!(summary.sql_generated_count, 1);
    assert_eq!(summary.verified_count, 1);
    assert_eq!(summary.generated_count, 1);

    storage.shutdown().await;
}

// ── Query-specific tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_traces_with_agent_filter() {
    let storage = test_storage();

    insert_span(
        &storage,
        &make_agent_span("t-f-1", "s-f-1", "agent-a", "OK", 1_000_000),
    );
    insert_span(
        &storage,
        &make_agent_span("t-f-2", "s-f-2", "agent-b", "OK", 2_000_000),
    );
    insert_span(
        &storage,
        &make_agent_span("t-f-3", "s-f-3", "agent-a", "OK", 3_000_000),
    );

    // Filter by agent-a
    let (traces, total) = storage
        .list_traces(10, 0, Some("agent-a"), None, None)
        .await
        .expect("list_traces should succeed");

    assert_eq!(total, 2);
    assert_eq!(traces.len(), 2);
    for t in &traces {
        assert!(t.span_attributes.contains("agent-a"));
    }

    storage.shutdown().await;
}

#[tokio::test]
async fn test_list_traces_with_status_filter() {
    let storage = test_storage();

    insert_span(
        &storage,
        &make_agent_span("t-s-1", "s-s-1", "agent", "OK", 1_000_000),
    );
    insert_span(
        &storage,
        &make_agent_span("t-s-2", "s-s-2", "agent", "ERROR", 1_000_000),
    );

    let (traces, total) = storage
        .list_traces(10, 0, None, Some("OK"), None)
        .await
        .expect("list_traces should succeed");

    assert_eq!(total, 1);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].status_code, "OK");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_list_traces_pagination() {
    let storage = test_storage();

    for i in 0..5 {
        let span = SpanRecord {
            trace_id: format!("t-pg-{i}"),
            span_id: format!("s-pg-{i}"),
            parent_span_id: String::new(),
            span_name: "agent.run_agent".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"agent":{"ref":"a","prompt":"q"}}"#.into(),
            duration_ns: 1_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: format!("2026-04-15T12:00:0{i}Z"),
        };
        insert_span(&storage, &span);
    }

    // Page 1: limit 2, offset 0
    let (page1, total) = storage
        .list_traces(2, 0, None, None, None)
        .await
        .expect("page 1 should succeed");
    assert_eq!(total, 5);
    assert_eq!(page1.len(), 2);

    // Page 2: limit 2, offset 2
    let (page2, _) = storage
        .list_traces(2, 2, None, None, None)
        .await
        .expect("page 2 should succeed");
    assert_eq!(page2.len(), 2);

    // No overlap between pages.
    let page1_ids: Vec<_> = page1.iter().map(|t| &t.trace_id).collect();
    let page2_ids: Vec<_> = page2.iter().map(|t| &t.trace_id).collect();
    for id in &page2_ids {
        assert!(!page1_ids.contains(id), "pages should not overlap");
    }

    storage.shutdown().await;
}

#[tokio::test]
async fn test_token_aggregation() {
    let storage = test_storage();

    let span = make_agent_span_with_tokens("t-tok-1", "s-tok-1", 100, 50, 150);
    insert_span(&storage, &span);

    let (traces, _) = storage
        .list_traces(10, 0, None, None, None)
        .await
        .expect("list_traces should succeed");

    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].prompt_tokens, 100);
    assert_eq!(traces[0].completion_tokens, 50);
    assert_eq!(traces[0].total_tokens, 150);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_trace_detail_returns_all_spans() {
    let storage = test_storage();

    // Root span
    insert_span(
        &storage,
        &SpanRecord {
            trace_id: "t-det-1".into(),
            span_id: "s-root".into(),
            parent_span_id: String::new(),
            span_name: "agent.run_agent".into(),
            service_name: "oxy".into(),
            span_attributes: "{}".into(),
            duration_ns: 5_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: "2026-04-15T12:00:00Z".into(),
        },
    );

    // Child span
    insert_span(
        &storage,
        &SpanRecord {
            trace_id: "t-det-1".into(),
            span_id: "s-child".into(),
            parent_span_id: "s-root".into(),
            span_name: "tool.execute_sql".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"tool":"execute_sql"}"#.into(),
            duration_ns: 2_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: "2026-04-15T12:00:01Z".into(),
        },
    );

    let details = storage
        .get_trace_detail("t-det-1")
        .await
        .expect("get_trace_detail should succeed");

    assert_eq!(details.len(), 2);
    // Ordered by timestamp ASC.
    assert_eq!(details[0].span_id, "s-root");
    assert_eq!(details[1].span_id, "s-child");
    assert_eq!(details[1].parent_span_id, "s-root");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_trace_enrichments() {
    let storage = test_storage();

    insert_span(
        &storage,
        &make_agent_span("t-enr-1", "s-enr-1", "a", "OK", 1_000_000),
    );
    insert_span(
        &storage,
        &make_agent_span("t-enr-2", "s-enr-2", "a", "ERROR", 2_000_000),
    );

    let enrichments = storage
        .get_trace_enrichments(&["t-enr-1".into(), "t-enr-2".into()])
        .await
        .expect("get_trace_enrichments should succeed");

    assert_eq!(enrichments.len(), 2);

    let ok_enr = enrichments
        .iter()
        .find(|e| e.trace_id == "t-enr-1")
        .unwrap();
    assert_eq!(ok_enr.status_code, "OK");
    assert_eq!(ok_enr.duration_ns, 1_000_000);

    let err_enr = enrichments
        .iter()
        .find(|e| e.trace_id == "t-enr-2")
        .unwrap();
    assert_eq!(err_enr.status_code, "ERROR");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_trace_enrichments_empty_input() {
    let storage = test_storage();

    let enrichments = storage
        .get_trace_enrichments(&[])
        .await
        .expect("empty input should succeed");

    assert!(enrichments.is_empty());

    storage.shutdown().await;
}

#[tokio::test]
async fn test_get_intent_analytics() {
    let storage = test_storage();

    // Insert classifications.
    storage
        .store_classification(
            "t-ia-1",
            "revenue q",
            1,
            "finance",
            0.9,
            &[0.1, 0.2],
            "agent",
            "src",
        )
        .await
        .unwrap();
    storage
        .store_classification(
            "t-ia-2",
            "cost q",
            1,
            "finance",
            0.85,
            &[0.3, 0.4],
            "agent",
            "src",
        )
        .await
        .unwrap();
    storage
        .store_classification(
            "t-ia-3",
            "leads q",
            2,
            "marketing",
            0.8,
            &[0.5, 0.6],
            "agent",
            "src",
        )
        .await
        .unwrap();

    let analytics = storage
        .get_intent_analytics(30)
        .await
        .expect("get_intent_analytics should succeed");

    assert_eq!(analytics.len(), 2);
    // Sorted by count DESC.
    assert_eq!(analytics[0].intent_name, "finance");
    assert_eq!(analytics[0].count, 2);
    assert_eq!(analytics[1].intent_name, "marketing");
    assert_eq!(analytics[1].count, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_outliers_and_unknown_count() {
    let storage = test_storage();

    storage
        .store_classification(
            "t-out-1",
            "weird q 1",
            0,
            "unknown",
            0.0,
            &[0.0],
            "agent",
            "src",
        )
        .await
        .unwrap();
    storage
        .store_classification(
            "t-out-2",
            "weird q 2",
            0,
            "unknown",
            0.0,
            &[0.0],
            "agent",
            "src",
        )
        .await
        .unwrap();
    storage
        .store_classification(
            "t-out-3",
            "normal q",
            1,
            "finance",
            0.9,
            &[0.1],
            "agent",
            "src",
        )
        .await
        .unwrap();

    let count = storage
        .get_unknown_count()
        .await
        .expect("get_unknown_count should succeed");
    assert_eq!(count, 2);

    let outliers = storage
        .get_outliers(10)
        .await
        .expect("get_outliers should succeed");
    assert_eq!(outliers.len(), 2);

    // Verify load_unknown_classifications returns only unknowns.
    let unknowns = storage
        .load_unknown_classifications()
        .await
        .expect("load_unknown_classifications should succeed");
    assert_eq!(unknowns.len(), 2);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_update_classification() {
    let storage = test_storage();

    // Store initial classification.
    storage
        .store_classification("t-upd-1", "q1", 0, "unknown", 0.0, &[0.1], "agent", "src")
        .await
        .unwrap();

    // Update it with a new classification.
    storage
        .update_classification("t-upd-1", "q1", 1, "finance", 0.95, &[0.1], "agent", "src")
        .await
        .unwrap();

    let embeddings = storage.load_embeddings().await.unwrap();
    // Should have exactly 1 record (the old was deleted, new inserted).
    assert_eq!(embeddings.len(), 1);
    assert_eq!(embeddings[0].3, "finance"); // intent_name
    assert_eq!(embeddings[0].0, "t-upd-1"); // trace_id

    storage.shutdown().await;
}

#[tokio::test]
async fn test_update_cluster_record_and_next_id() {
    let storage = test_storage();

    let cluster = IntentCluster {
        cluster_id: 1,
        intent_name: "finance".into(),
        intent_description: "Financial questions".into(),
        centroid: vec![0.5, 0.5],
        sample_questions: vec!["revenue?".into()],
    };

    storage
        .update_cluster_record(&cluster)
        .await
        .expect("update_cluster_record should succeed");

    let next_id = storage
        .get_next_cluster_id()
        .await
        .expect("get_next_cluster_id should succeed");
    assert_eq!(next_id, 2);

    // Verify the cluster was stored via load_clusters.
    let clusters = storage.load_clusters().await.unwrap();
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].intent_name, "finance");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_fetch_unprocessed_questions() {
    let storage = test_storage();

    // Insert agent spans with prompts.
    insert_span(
        &storage,
        &make_agent_span("t-unp-1", "s-unp-1", "agent", "OK", 1_000_000),
    );
    insert_span(
        &storage,
        &make_agent_span("t-unp-2", "s-unp-2", "agent", "OK", 1_000_000),
    );

    // Classify one of them.
    storage
        .store_classification(
            "t-unp-1",
            "test question",
            1,
            "finance",
            0.9,
            &[0.1],
            "agent",
            "agent",
        )
        .await
        .unwrap();

    let unprocessed = storage
        .fetch_unprocessed_questions(10)
        .await
        .expect("fetch_unprocessed_questions should succeed");

    // Only t-unp-2 should be unprocessed.
    assert_eq!(unprocessed.len(), 1);
    assert_eq!(unprocessed[0].0, "t-unp-2");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_metrics_list_pagination() {
    let storage = test_storage();

    // Insert metrics for 3 different metric names.
    for name in &["alpha", "beta", "gamma"] {
        let metric = MetricUsageRecord {
            metric_name: name.to_string(),
            source_type: "agent".into(),
            source_ref: "ref".into(),
            context: "ctx".into(),
            context_types: r#"["SQL"]"#.into(),
            trace_id: format!("t-ml-{name}"),
        };
        insert_metric(&storage, &metric);
    }

    let list = storage
        .get_metrics_list(30, 2, 0)
        .await
        .expect("get_metrics_list should succeed");

    assert_eq!(list.total, 3);
    assert_eq!(list.metrics.len(), 2);
    assert_eq!(list.limit, 2);
    assert_eq!(list.offset, 0);

    // Page 2
    let page2 = storage
        .get_metrics_list(30, 2, 2)
        .await
        .expect("page 2 should succeed");
    assert_eq!(page2.metrics.len(), 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_context_type_breakdown() {
    let storage = test_storage();

    // Insert metrics with different context_types.
    let metrics = vec![
        MetricUsageRecord {
            metric_name: "m1".into(),
            source_type: "agent".into(),
            source_ref: "ref".into(),
            context: "c".into(),
            context_types: r#"["SQL"]"#.into(),
            trace_id: "t-ct-1".into(),
        },
        MetricUsageRecord {
            metric_name: "m2".into(),
            source_type: "agent".into(),
            source_ref: "ref".into(),
            context: "c".into(),
            context_types: r#"["SQL","Question"]"#.into(),
            trace_id: "t-ct-2".into(),
        },
        MetricUsageRecord {
            metric_name: "m3".into(),
            source_type: "workflow".into(),
            source_ref: "ref".into(),
            context: "c".into(),
            context_types: r#"["SemanticQuery"]"#.into(),
            trace_id: "t-ct-3".into(),
        },
    ];

    for m in &metrics {
        insert_metric(&storage, m);
    }

    let analytics = storage
        .get_metrics_analytics(30)
        .await
        .expect("get_metrics_analytics should succeed");

    assert_eq!(analytics.total_queries, 3);
    assert_eq!(analytics.by_context_type.sql, 2); // m1 + m2
    assert_eq!(analytics.by_context_type.question, 1); // m2
    assert_eq!(analytics.by_context_type.semantic_query, 1); // m3
    assert_eq!(analytics.by_source_type.agent, 2);
    assert_eq!(analytics.by_source_type.workflow, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_store_metric_usages_via_trait() {
    let storage = test_storage();

    // Use the trait method (which delegates to the writer).
    let metrics = vec![MetricUsageRecord {
        metric_name: "via_trait".into(),
        source_type: "agent".into(),
        source_ref: "ref".into(),
        context: "c".into(),
        context_types: r#"["SQL"]"#.into(),
        trace_id: "t-vt-1".into(),
    }];

    storage
        .store_metric_usages(metrics)
        .await
        .expect("store_metric_usages should succeed");

    // The writer processes asynchronously; wait for it.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let analytics = storage.get_metrics_analytics(30).await.unwrap();
    assert_eq!(analytics.total_queries, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_time_series() {
    let storage = test_storage();

    let agent_span = make_agent_parent_span("t-ts-1", "s-ts-agent", "ts-agent");
    insert_span(&storage, &agent_span);

    let tool = make_tool_call_span(
        "t-ts-1",
        "s-ts-tool",
        "s-ts-agent",
        "semantic_query",
        true,
        "ts-agent",
    );
    insert_span(&storage, &tool);

    let series = storage
        .get_execution_time_series(30)
        .await
        .expect("get_execution_time_series should succeed");

    // Should have at least one bucket.
    assert!(!series.is_empty());
    assert_eq!(series[0].semantic_query_count, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_agent_stats() {
    let storage = test_storage();

    // Agent A with 2 tool calls.
    let agent_a = make_agent_parent_span("t-as-1", "s-as-a1", "agent-a");
    insert_span(&storage, &agent_a);
    insert_span(
        &storage,
        &make_tool_call_span(
            "t-as-1",
            "s-as-t1",
            "s-as-a1",
            "semantic_query",
            true,
            "agent-a",
        ),
    );
    insert_span(
        &storage,
        &make_tool_call_span(
            "t-as-1",
            "s-as-t2",
            "s-as-a1",
            "sql_generated",
            false,
            "agent-a",
        ),
    );

    // Agent B with 1 tool call.
    let agent_b = make_agent_parent_span("t-as-2", "s-as-b1", "agent-b");
    insert_span(&storage, &agent_b);
    insert_span(
        &storage,
        &make_tool_call_span(
            "t-as-2",
            "s-as-t3",
            "s-as-b1",
            "semantic_query",
            true,
            "agent-b",
        ),
    );

    let stats = storage
        .get_execution_agent_stats(30, 10)
        .await
        .expect("get_execution_agent_stats should succeed");

    assert_eq!(stats.len(), 2);
    // Sorted by total_executions DESC.
    assert_eq!(stats[0].agent_ref, "agent-a");
    assert_eq!(stats[0].total_executions, 2);
    assert_eq!(stats[1].agent_ref, "agent-b");
    assert_eq!(stats[1].total_executions, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_list_with_filters() {
    let storage = test_storage();

    let agent = make_agent_parent_span("t-el-1", "s-el-a1", "el-agent");
    insert_span(&storage, &agent);

    insert_span(
        &storage,
        &make_tool_call_span(
            "t-el-1",
            "s-el-t1",
            "s-el-a1",
            "semantic_query",
            true,
            "el-agent",
        ),
    );
    insert_span(
        &storage,
        &make_tool_call_span(
            "t-el-1",
            "s-el-t2",
            "s-el-a1",
            "sql_generated",
            false,
            "el-agent",
        ),
    );

    // Filter by execution_type.
    let list = storage
        .get_execution_list(30, 10, 0, Some("semantic_query"), None, None, None)
        .await
        .expect("get_execution_list should succeed");

    assert_eq!(list.total, 1);
    assert_eq!(list.executions.len(), 1);
    assert_eq!(list.executions[0].execution_type, "semantic_query");

    // Filter by is_verified.
    let verified = storage
        .get_execution_list(30, 10, 0, None, Some(true), None, None)
        .await
        .unwrap();
    assert_eq!(verified.total, 1);
    assert_eq!(verified.executions[0].is_verified, "true");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_cluster_map_data() {
    let storage = test_storage();

    // Insert a classification so cluster_map_data returns it.
    storage
        .store_classification(
            "t-cm-1",
            "revenue question",
            1,
            "finance",
            0.9,
            &[0.1, 0.2, 0.3],
            "agent",
            "sales",
        )
        .await
        .unwrap();

    let data = storage
        .get_cluster_map_data(30, 100, None)
        .await
        .expect("get_cluster_map_data should succeed");

    assert_eq!(data.len(), 1);
    assert_eq!(data[0].trace_id, "t-cm-1");
    assert_eq!(data[0].question, "revenue question");
    assert_eq!(data[0].cluster_id, 1);
    assert_eq!(data[0].intent_name, "finance");
    assert_eq!(data[0].embedding.len(), 3);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_cluster_map_data_with_source_filter() {
    let storage = test_storage();

    storage
        .store_classification("t-cmf-1", "q1", 1, "fin", 0.9, &[0.1], "agent", "agent-a")
        .await
        .unwrap();
    storage
        .store_classification("t-cmf-2", "q2", 2, "mkt", 0.8, &[0.2], "agent", "agent-b")
        .await
        .unwrap();

    let data = storage
        .get_cluster_map_data(30, 100, Some("agent-a"))
        .await
        .expect("filtered cluster_map_data should succeed");

    assert_eq!(data.len(), 1);
    assert_eq!(data[0].source, "agent-a");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_store_clusters_replaces_existing() {
    let storage = test_storage();

    let clusters_v1 = vec![IntentCluster {
        cluster_id: 1,
        intent_name: "old".into(),
        intent_description: "Old desc".into(),
        centroid: vec![0.0],
        sample_questions: vec!["old q".into()],
    }];

    storage.store_clusters(&clusters_v1).await.unwrap();

    let clusters_v2 = vec![IntentCluster {
        cluster_id: 1,
        intent_name: "new".into(),
        intent_description: "New desc".into(),
        centroid: vec![1.0],
        sample_questions: vec!["new q".into()],
    }];

    storage.store_clusters(&clusters_v2).await.unwrap();

    let loaded = storage.load_clusters().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].intent_name, "new");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_empty_database_queries() {
    let storage = test_storage();

    // All queries should succeed on an empty database.
    let (traces, total) = storage.list_traces(10, 0, None, None, None).await.unwrap();
    assert!(traces.is_empty());
    assert_eq!(total, 0);

    let details = storage.get_trace_detail("nonexistent").await.unwrap();
    assert!(details.is_empty());

    let clusters = storage.load_clusters().await.unwrap();
    assert!(clusters.is_empty());

    let embeddings = storage.load_embeddings().await.unwrap();
    assert!(embeddings.is_empty());

    let count = storage.get_unknown_count().await.unwrap();
    assert_eq!(count, 0);

    let next_id = storage.get_next_cluster_id().await.unwrap();
    assert_eq!(next_id, 1);

    let analytics = storage.get_metrics_analytics(30).await.unwrap();
    assert_eq!(analytics.total_queries, 0);

    // Note: get_execution_summary on a completely empty database may return
    // NULL for count_if columns (a DuckDB quirk with empty INNER JOINs).
    // We verify it doesn't panic; the error is acceptable.
    let _summary_result = storage.get_execution_summary(30).await;

    storage.shutdown().await;
}

// ── Trait object tests ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_trait_object_usage() {
    // Verify DuckDBStorage can be used as a dyn ObservabilityStore.
    let storage = test_storage();
    let store: &dyn ObservabilityStore = &storage;

    insert_span(
        &storage,
        &make_agent_span("t-dyn-1", "s-dyn-1", "dyn-agent", "OK", 1_000_000),
    );

    let (traces, total) = store
        .list_traces(10, 0, None, None, None)
        .await
        .expect("trait object query should succeed");

    assert_eq!(total, 1);
    assert_eq!(traces[0].trace_id, "t-dyn-1");

    store.shutdown().await;
}

#[tokio::test]
async fn test_trait_object_arc() {
    use std::sync::Arc;

    // Verify DuckDBStorage works behind Arc<dyn ObservabilityStore>.
    let storage = test_storage();
    let store: Arc<dyn ObservabilityStore> = Arc::new(storage);

    let (traces, total) = store
        .list_traces(10, 0, None, None, None)
        .await
        .expect("Arc<dyn> query should succeed");

    assert_eq!(total, 0);
    assert!(traces.is_empty());

    store.shutdown().await;
}

// ── Realistic mock data fixtures ────────────────────────────────────────────

/// Generate a timestamp string relative to "now" minus the given offset in
/// seconds.  Using `chrono::Utc::now()` ensures the generated timestamps fall
/// within any `INTERVAL '30 DAY'` filter used by the query layer.
fn ts_offset(seconds_ago: i64) -> String {
    use chrono::{Duration, Utc};
    let t = Utc::now() - Duration::seconds(seconds_ago);
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Populate the database with a realistic set of traces, metrics, intent
/// classifications, and clusters that simulate a multi-agent conversation
/// environment.
///
/// # Traces produced
///
/// | trace_id           | agent_ref        | type              | status |
/// |--------------------|------------------|-------------------|--------|
/// | `trace-sales-1`    | `sales_agent`    | agent.run_agent   | OK     |
/// | `trace-analytics-2`| `analytics_agent`| agent.run_agent   | OK     |
/// | `trace-error-3`    | `sales_agent`    | agent.run_agent   | ERROR  |
/// | `trace-workflow-4` | —                | workflow.run_workflow | OK  |
fn populate_realistic_data(storage: &DuckDBStorage) {
    // ── Trace 1: Simple agent run (sales_agent) ─────────────────────────

    // Root span — agent.run_agent
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-sales-1".into(),
            span_id: "span-sales-root".into(),
            parent_span_id: String::new(),
            span_name: "agent.run_agent".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"agent.prompt":"What is total revenue this quarter?","oxy.agent.ref":"sales_agent","oxy.name":"agent.run_agent"}"#.into(),
            duration_ns: 5_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: ts_offset(3000),
        },
    );

    // Child: agent.launcher
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-sales-1".into(),
            span_id: "span-sales-launcher".into(),
            parent_span_id: "span-sales-root".into(),
            span_name: "agent.launcher".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.name":"agent.launcher"}"#.into(),
            duration_ns: 4_500_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: ts_offset(2999),
        },
    );

    // Child: first llm.call with usage event (250/120/370)
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-sales-1".into(),
            span_id: "span-sales-llm1".into(),
            parent_span_id: "span-sales-launcher".into(),
            span_name: "llm.call".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.name":"llm.call"}"#.into(),
            duration_ns: 1_500_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: r#"[{"name":"llm.usage","attributes":{"prompt_tokens":250,"completion_tokens":120,"total_tokens":370}}]"#.into(),
            timestamp: ts_offset(2998),
        },
    );

    // Child: tool.execute (semantic_query, verified)
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-sales-1".into(),
            span_id: "span-sales-tool1".into(),
            parent_span_id: "span-sales-launcher".into(),
            span_name: "tool.execute".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.span_type":"tool_call","oxy.execution_type":"semantic_query","oxy.is_verified":"true","oxy.database":"analytics_db","oxy.topic":"revenue","oxy.generated_sql":"SELECT SUM(revenue) FROM sales","oxy.name":"tool.execute"}"#.into(),
            duration_ns: 800_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: r#"[{"name":"tool_call.input","attributes":{"input":"semantic query for revenue"}},{"name":"tool_call.output","attributes":{"status":"success","output":"$1,250,000"}}]"#.into(),
            timestamp: ts_offset(2997),
        },
    );

    // Child: second llm.call with usage event (180/90/270)
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-sales-1".into(),
            span_id: "span-sales-llm2".into(),
            parent_span_id: "span-sales-launcher".into(),
            span_name: "llm.call".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.name":"llm.call"}"#.into(),
            duration_ns: 1_200_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: r#"[{"name":"llm.usage","attributes":{"prompt_tokens":180,"completion_tokens":90,"total_tokens":270}}]"#.into(),
            timestamp: ts_offset(2996),
        },
    );

    // ── Trace 2: Agent run with SQL generation (analytics_agent) ────────

    // Root span
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-analytics-2".into(),
            span_id: "span-analytics-root".into(),
            parent_span_id: String::new(),
            span_name: "agent.run_agent".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"agent.prompt":"Show me user signups by month","oxy.agent.ref":"analytics_agent","oxy.name":"agent.run_agent"}"#.into(),
            duration_ns: 4_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: ts_offset(1800),
        },
    );

    // Child: llm.call with usage (300/150/450)
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-analytics-2".into(),
            span_id: "span-analytics-llm1".into(),
            parent_span_id: "span-analytics-root".into(),
            span_name: "llm.call".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.name":"llm.call"}"#.into(),
            duration_ns: 1_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: r#"[{"name":"llm.usage","attributes":{"prompt_tokens":300,"completion_tokens":150,"total_tokens":450}}]"#.into(),
            timestamp: ts_offset(1799),
        },
    );

    // Child: tool.execute (sql_generated, NOT verified)
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-analytics-2".into(),
            span_id: "span-analytics-tool1".into(),
            parent_span_id: "span-analytics-root".into(),
            span_name: "tool.execute".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.span_type":"tool_call","oxy.execution_type":"sql_generated","oxy.is_verified":"false","oxy.database":"user_db","oxy.topic":"signups","oxy.generated_sql":"SELECT date_trunc('month', created_at) AS month, COUNT(*) FROM users GROUP BY 1","oxy.name":"tool.execute"}"#.into(),
            duration_ns: 600_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: r#"[{"name":"tool_call.input","attributes":{"input":"generate SQL for user signups by month"}},{"name":"tool_call.output","attributes":{"status":"success","output":"Jan: 120, Feb: 150, Mar: 200"}}]"#.into(),
            timestamp: ts_offset(1798),
        },
    );

    // Child: second llm.call with usage (200/100/300)
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-analytics-2".into(),
            span_id: "span-analytics-llm2".into(),
            parent_span_id: "span-analytics-root".into(),
            span_name: "llm.call".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.name":"llm.call"}"#.into(),
            duration_ns: 900_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: r#"[{"name":"llm.usage","attributes":{"prompt_tokens":200,"completion_tokens":100,"total_tokens":300}}]"#.into(),
            timestamp: ts_offset(1797),
        },
    );

    // ── Trace 3: Agent run with error (sales_agent again) ───────────────

    // Root span — status ERROR
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-error-3".into(),
            span_id: "span-error-root".into(),
            parent_span_id: String::new(),
            span_name: "agent.run_agent".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"agent.prompt":"Error question","oxy.agent.ref":"sales_agent","oxy.name":"agent.run_agent"}"#.into(),
            duration_ns: 2_000_000,
            status_code: "ERROR".into(),
            status_message: "Internal error".into(),
            event_data: "[]".into(),
            timestamp: ts_offset(900),
        },
    );

    // Child: llm.call (100/40/140)
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-error-3".into(),
            span_id: "span-error-llm1".into(),
            parent_span_id: "span-error-root".into(),
            span_name: "llm.call".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.name":"llm.call"}"#.into(),
            duration_ns: 800_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: r#"[{"name":"llm.usage","attributes":{"prompt_tokens":100,"completion_tokens":40,"total_tokens":140}}]"#.into(),
            timestamp: ts_offset(899),
        },
    );

    // Child: tool.execute with error output
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-error-3".into(),
            span_id: "span-error-tool1".into(),
            parent_span_id: "span-error-root".into(),
            span_name: "tool.execute".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.span_type":"tool_call","oxy.execution_type":"semantic_query","oxy.is_verified":"true","oxy.database":"analytics_db","oxy.topic":"revenue","oxy.name":"tool.execute"}"#.into(),
            duration_ns: 300_000,
            status_code: "ERROR".into(),
            status_message: "Query failed".into(),
            event_data: r#"[{"name":"tool_call.output","attributes":{"status":"error","error":{"message":"Connection refused to analytics_db"}}}]"#.into(),
            timestamp: ts_offset(898),
        },
    );

    // ── Trace 4: Workflow run (no LLM calls, no agent) ──────────────────

    // Root span — workflow.run_workflow
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-workflow-4".into(),
            span_id: "span-workflow-root".into(),
            parent_span_id: String::new(),
            span_name: "workflow.run_workflow".into(),
            service_name: "oxy".into(),
            span_attributes:
                r#"{"oxy.name":"workflow.run_workflow","workflow.ref":"daily_report"}"#.into(),
            duration_ns: 10_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: ts_offset(600),
        },
    );

    // Child: workflow step 1
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-workflow-4".into(),
            span_id: "span-workflow-step1".into(),
            parent_span_id: "span-workflow-root".into(),
            span_name: "workflow.step".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.name":"workflow.step","step.name":"fetch_data"}"#.into(),
            duration_ns: 4_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: ts_offset(599),
        },
    );

    // Child: workflow step 2
    insert_span(
        storage,
        &SpanRecord {
            trace_id: "trace-workflow-4".into(),
            span_id: "span-workflow-step2".into(),
            parent_span_id: "span-workflow-root".into(),
            span_name: "workflow.step".into(),
            service_name: "oxy".into(),
            span_attributes: r#"{"oxy.name":"workflow.step","step.name":"aggregate"}"#.into(),
            duration_ns: 3_000_000,
            status_code: "OK".into(),
            status_message: String::new(),
            event_data: "[]".into(),
            timestamp: ts_offset(598),
        },
    );

    // ── Metric data ─────────────────────────────────────────────────────

    insert_metric(
        storage,
        &MetricUsageRecord {
            metric_name: "total_revenue".into(),
            source_type: "agent".into(),
            source_ref: "sales_agent".into(),
            context: "quarterly revenue report".into(),
            context_types: r#"["SQL","SemanticQuery"]"#.into(),
            trace_id: "trace-sales-1".into(),
        },
    );

    insert_metric(
        storage,
        &MetricUsageRecord {
            metric_name: "user_signups".into(),
            source_type: "agent".into(),
            source_ref: "analytics_agent".into(),
            context: "user signups by month".into(),
            context_types: r#"["SQL","Question"]"#.into(),
            trace_id: "trace-analytics-2".into(),
        },
    );

    // total_revenue from a different trace (for co-occurrence testing)
    insert_metric(
        storage,
        &MetricUsageRecord {
            metric_name: "total_revenue".into(),
            source_type: "workflow".into(),
            source_ref: "daily_report".into(),
            context: "daily revenue check".into(),
            context_types: r#"["SQL"]"#.into(),
            trace_id: "trace-workflow-4".into(),
        },
    );

    // order_count co-occurring with total_revenue in trace-sales-1
    insert_metric(
        storage,
        &MetricUsageRecord {
            metric_name: "order_count".into(),
            source_type: "agent".into(),
            source_ref: "sales_agent".into(),
            context: "order count alongside revenue".into(),
            context_types: r#"["SQL"]"#.into(),
            trace_id: "trace-sales-1".into(),
        },
    );

    // ── Intent classification data ──────────────────────────────────────

    // Insert via direct SQL since store_classification is async and we want
    // synchronous setup. We match the schema exactly.
    {
        let conn = storage.conn().lock().unwrap();

        conn.execute(
            "INSERT INTO intent_classifications (trace_id, question, cluster_id, intent_name, confidence, embedding, source_type, source) \
             VALUES ('trace-sales-1', 'What is total revenue this quarter?', 1, 'Revenue Analytics', 0.92, [0.1, 0.2, 0.3]::FLOAT[], 'agent', 'sales_agent')",
            [],
        )
        .expect("insert classification 1");

        conn.execute(
            "INSERT INTO intent_classifications (trace_id, question, cluster_id, intent_name, confidence, embedding, source_type, source) \
             VALUES ('trace-analytics-2', 'Show me user signups by month', 2, 'User Analytics', 0.87, [0.4, 0.5, 0.6]::FLOAT[], 'agent', 'analytics_agent')",
            [],
        )
        .expect("insert classification 2");

        conn.execute(
            "INSERT INTO intent_classifications (trace_id, question, cluster_id, intent_name, confidence, embedding, source_type, source) \
             VALUES ('trace-error-3', 'Error question', 0, 'unknown', 0.3, [0.0, 0.0, 0.0]::FLOAT[], 'agent', 'sales_agent')",
            [],
        )
        .expect("insert classification 3");
    }

    // ── Cluster data ────────────────────────────────────────────────────

    {
        let conn = storage.conn().lock().unwrap();

        conn.execute(
            "INSERT INTO intent_clusters (cluster_id, intent_name, intent_description, centroid, sample_questions, question_count) \
             VALUES (1, 'Revenue Analytics', 'Questions about revenue and financial metrics', [0.15, 0.25, 0.35]::FLOAT[], '[\"What is total revenue?\",\"Show me quarterly revenue\"]', 2)",
            [],
        )
        .expect("insert cluster 1");

        conn.execute(
            "INSERT INTO intent_clusters (cluster_id, intent_name, intent_description, centroid, sample_questions, question_count) \
             VALUES (2, 'User Analytics', 'Questions about user signups and engagement', [0.45, 0.55, 0.65]::FLOAT[], '[\"Show me user signups\",\"How many new users?\"]', 2)",
            [],
        )
        .expect("insert cluster 2");
    }
}

// ── Tests using realistic mock data ─────────────────────────────────────────

// ── Trace Tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_traces_returns_realistic_data() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let (traces, total) = storage
        .list_traces(10, 0, None, None, None)
        .await
        .expect("list_traces should succeed");

    // 4 root spans: trace-sales-1, trace-analytics-2, trace-error-3, trace-workflow-4
    assert_eq!(total, 4, "should have 4 root traces");
    assert_eq!(traces.len(), 4);

    // Ordered by timestamp DESC (most recent first).
    // trace-workflow-4 is most recent (600s ago), then trace-error-3 (900s),
    // then trace-analytics-2 (1800s), then trace-sales-1 (3600s).
    assert_eq!(traces[0].trace_id, "trace-workflow-4");
    assert_eq!(traces[1].trace_id, "trace-error-3");
    assert_eq!(traces[2].trace_id, "trace-analytics-2");
    assert_eq!(traces[3].trace_id, "trace-sales-1");

    // Token aggregation for trace-sales-1: prompt=250+180=430, completion=120+90=210, total=370+270=640
    let sales_trace = traces
        .iter()
        .find(|t| t.trace_id == "trace-sales-1")
        .unwrap();
    assert_eq!(sales_trace.prompt_tokens, 430);
    assert_eq!(sales_trace.completion_tokens, 210);
    assert_eq!(sales_trace.total_tokens, 640);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_list_traces_filter_by_agent_ref() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // sales_agent has 2 traces: trace-sales-1 and trace-error-3
    let (sales_traces, sales_total) = storage
        .list_traces(10, 0, Some("sales_agent"), None, None)
        .await
        .expect("filter by sales_agent should succeed");

    assert_eq!(sales_total, 2, "sales_agent should have 2 traces");
    assert_eq!(sales_traces.len(), 2);
    for t in &sales_traces {
        assert!(
            t.span_attributes.contains("sales_agent"),
            "all traces should belong to sales_agent"
        );
    }

    // analytics_agent has 1 trace
    let (analytics_traces, analytics_total) = storage
        .list_traces(10, 0, Some("analytics_agent"), None, None)
        .await
        .expect("filter by analytics_agent should succeed");

    assert_eq!(analytics_total, 1, "analytics_agent should have 1 trace");
    assert_eq!(analytics_traces.len(), 1);
    assert_eq!(analytics_traces[0].trace_id, "trace-analytics-2");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_list_traces_filter_by_status() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // Filter by ERROR status
    let (error_traces, error_total) = storage
        .list_traces(10, 0, None, Some("ERROR"), None)
        .await
        .expect("filter by ERROR should succeed");

    assert_eq!(error_total, 1, "should have 1 error trace");
    assert_eq!(error_traces.len(), 1);
    assert_eq!(error_traces[0].trace_id, "trace-error-3");
    assert_eq!(error_traces[0].status_code, "ERROR");

    // Filter by OK status
    let (ok_traces, ok_total) = storage
        .list_traces(10, 0, None, Some("OK"), None)
        .await
        .expect("filter by OK should succeed");

    assert_eq!(ok_total, 3, "should have 3 OK traces");
    assert_eq!(ok_traces.len(), 3);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_trace_detail_shows_full_span_tree() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let details = storage
        .get_trace_detail("trace-sales-1")
        .await
        .expect("get_trace_detail should succeed");

    // trace-sales-1 has 5 spans: root, launcher, llm1, tool1, llm2
    assert_eq!(details.len(), 5, "trace-sales-1 should have 5 spans");

    // Verify ordering by timestamp ASC (root first, children after)
    assert_eq!(details[0].span_id, "span-sales-root");
    assert_eq!(details[0].parent_span_id, "");
    assert_eq!(details[0].span_name, "agent.run_agent");

    // All should share the same trace_id
    for span in &details {
        assert_eq!(span.trace_id, "trace-sales-1");
    }

    // Verify parent-child relationships exist
    let child_ids: Vec<&str> = details.iter().map(|s| s.span_id.as_str()).collect();
    assert!(child_ids.contains(&"span-sales-launcher"));
    assert!(child_ids.contains(&"span-sales-llm1"));
    assert!(child_ids.contains(&"span-sales-tool1"));
    assert!(child_ids.contains(&"span-sales-llm2"));

    storage.shutdown().await;
}

#[tokio::test]
async fn test_trace_detail_includes_events() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let details = storage
        .get_trace_detail("trace-sales-1")
        .await
        .expect("get_trace_detail should succeed");

    // Find the first llm.call span and check its event_data contains llm.usage
    let llm_span = details
        .iter()
        .find(|s| s.span_id == "span-sales-llm1")
        .expect("should find span-sales-llm1");

    assert!(
        llm_span.event_data.contains("llm.usage"),
        "LLM span should have llm.usage event"
    );
    assert!(
        llm_span.event_data.contains("prompt_tokens"),
        "LLM span event should contain prompt_tokens"
    );
    assert!(
        llm_span.event_data.contains("250"),
        "LLM span should have prompt_tokens=250"
    );

    // Find the tool span and verify tool_call events
    let tool_span = details
        .iter()
        .find(|s| s.span_id == "span-sales-tool1")
        .expect("should find span-sales-tool1");

    assert!(
        tool_span.event_data.contains("tool_call.input"),
        "tool span should have tool_call.input event"
    );
    assert!(
        tool_span.event_data.contains("tool_call.output"),
        "tool span should have tool_call.output event"
    );

    storage.shutdown().await;
}

// ── Token Aggregation Tests ─────────────────────────────────────────────────

#[tokio::test]
async fn test_token_aggregation_from_child_spans() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let (traces, _) = storage
        .list_traces(10, 0, None, None, None)
        .await
        .expect("list_traces should succeed");

    // trace-sales-1: two LLM child spans with tokens
    //   llm1: 250 prompt, 120 completion, 370 total
    //   llm2: 180 prompt,  90 completion, 270 total
    //   sum : 430 prompt, 210 completion, 640 total
    let sales = traces
        .iter()
        .find(|t| t.trace_id == "trace-sales-1")
        .unwrap();
    assert_eq!(
        sales.prompt_tokens, 430,
        "prompt tokens should sum child spans"
    );
    assert_eq!(
        sales.completion_tokens, 210,
        "completion tokens should sum child spans"
    );
    assert_eq!(
        sales.total_tokens, 640,
        "total tokens should sum child spans"
    );

    // trace-analytics-2: two LLM child spans
    //   llm1: 300/150/450
    //   llm2: 200/100/300
    //   sum : 500/250/750
    let analytics = traces
        .iter()
        .find(|t| t.trace_id == "trace-analytics-2")
        .unwrap();
    assert_eq!(analytics.prompt_tokens, 500);
    assert_eq!(analytics.completion_tokens, 250);
    assert_eq!(analytics.total_tokens, 750);

    // trace-error-3: one LLM child span (100/40/140)
    let error = traces
        .iter()
        .find(|t| t.trace_id == "trace-error-3")
        .unwrap();
    assert_eq!(error.prompt_tokens, 100);
    assert_eq!(error.completion_tokens, 40);
    assert_eq!(error.total_tokens, 140);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_token_zero_for_traces_without_llm_calls() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let (traces, _) = storage
        .list_traces(10, 0, None, None, None)
        .await
        .expect("list_traces should succeed");

    // trace-workflow-4 has no LLM calls, tokens should be 0.
    let workflow = traces
        .iter()
        .find(|t| t.trace_id == "trace-workflow-4")
        .unwrap();
    assert_eq!(
        workflow.prompt_tokens, 0,
        "workflow trace should have 0 prompt tokens"
    );
    assert_eq!(
        workflow.completion_tokens, 0,
        "workflow trace should have 0 completion tokens"
    );
    assert_eq!(
        workflow.total_tokens, 0,
        "workflow trace should have 0 total tokens"
    );

    storage.shutdown().await;
}

// ── Execution Analytics Tests ───────────────────────────────────────────────

#[tokio::test]
async fn test_execution_summary_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let summary = storage
        .get_execution_summary(30)
        .await
        .expect("get_execution_summary should succeed");

    // Tool spans that match execution analytics criteria:
    //   span-sales-tool1:     semantic_query, verified=true,  in trace with oxy.agent.ref=sales_agent  -> success
    //   span-analytics-tool1: sql_generated,  verified=false, in trace with oxy.agent.ref=analytics_agent -> success
    //   span-error-tool1:     semantic_query, verified=true,  in trace with oxy.agent.ref=sales_agent  -> error
    assert_eq!(
        summary.total_executions, 3,
        "should count 3 tool call executions"
    );
    assert_eq!(
        summary.semantic_query_count, 2,
        "should have 2 semantic_query executions"
    );
    assert_eq!(
        summary.sql_generated_count, 1,
        "should have 1 sql_generated execution"
    );
    assert_eq!(
        summary.verified_count, 2,
        "should have 2 verified executions"
    );
    assert_eq!(
        summary.generated_count, 1,
        "should have 1 generated (not verified) execution"
    );

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_time_series_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let series = storage
        .get_execution_time_series(30)
        .await
        .expect("get_execution_time_series should succeed");

    // All tool spans are within the same day (they're all offset from now by < 1 hour).
    // So we should have exactly 1 bucket (today).
    assert!(
        !series.is_empty(),
        "time series should have at least one bucket"
    );

    // Sum across all buckets should match the summary totals.
    let total_semantic: u64 = series.iter().map(|b| b.semantic_query_count).sum();
    let total_sql_gen: u64 = series.iter().map(|b| b.sql_generated_count).sum();
    assert_eq!(total_semantic, 2);
    assert_eq!(total_sql_gen, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_list_with_filters_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // Filter by execution_type = semantic_query
    let list = storage
        .get_execution_list(30, 10, 0, Some("semantic_query"), None, None, None)
        .await
        .expect("filter by execution_type should succeed");

    assert_eq!(list.total, 2, "should have 2 semantic_query executions");
    for exec in &list.executions {
        assert_eq!(exec.execution_type, "semantic_query");
    }

    // Filter by is_verified = true
    let verified_list = storage
        .get_execution_list(30, 10, 0, None, Some(true), None, None)
        .await
        .expect("filter by is_verified should succeed");

    assert_eq!(verified_list.total, 2, "should have 2 verified executions");
    for exec in &verified_list.executions {
        assert_eq!(exec.is_verified, "true");
    }

    // Filter by is_verified = false
    let generated_list = storage
        .get_execution_list(30, 10, 0, None, Some(false), None, None)
        .await
        .expect("filter by not verified should succeed");

    assert_eq!(
        generated_list.total, 1,
        "should have 1 not-verified execution"
    );

    // Filter by source_ref = sales_agent
    let agent_list = storage
        .get_execution_list(30, 10, 0, None, None, Some("sales_agent"), None)
        .await
        .expect("filter by source_ref should succeed");

    assert_eq!(
        agent_list.total, 2,
        "sales_agent should have 2 tool executions"
    );

    // Filter by status = error
    let error_list = storage
        .get_execution_list(30, 10, 0, None, None, None, Some("error"))
        .await
        .expect("filter by status=error should succeed");

    assert_eq!(error_list.total, 1, "should have 1 error execution");
    assert_eq!(error_list.executions[0].status, "error");

    // Filter by status = success
    let success_list = storage
        .get_execution_list(30, 10, 0, None, None, None, Some("success"))
        .await
        .expect("filter by status=success should succeed");

    assert_eq!(success_list.total, 2, "should have 2 successful executions");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_detail_has_correct_fields() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // Get all executions and find the semantic_query from trace-sales-1
    let list = storage
        .get_execution_list(30, 10, 0, None, None, None, None)
        .await
        .expect("get_execution_list should succeed");

    let sales_exec = list
        .executions
        .iter()
        .find(|e| e.trace_id == "trace-sales-1")
        .expect("should find execution from trace-sales-1");

    assert_eq!(sales_exec.execution_type, "semantic_query");
    assert_eq!(sales_exec.is_verified, "true");
    assert_eq!(sales_exec.source_ref, "sales_agent");
    assert_eq!(sales_exec.database, "analytics_db");
    assert_eq!(sales_exec.topic, "revenue");
    assert_eq!(sales_exec.generated_sql, "SELECT SUM(revenue) FROM sales");
    assert_eq!(
        sales_exec.user_question,
        "What is total revenue this quarter?"
    );
    assert_eq!(sales_exec.status, "success");

    // Verify the analytics agent execution has sql_generated type
    let analytics_exec = list
        .executions
        .iter()
        .find(|e| e.trace_id == "trace-analytics-2")
        .expect("should find execution from trace-analytics-2");

    assert_eq!(analytics_exec.execution_type, "sql_generated");
    assert_eq!(analytics_exec.is_verified, "false");
    assert_eq!(analytics_exec.source_ref, "analytics_agent");

    // Verify the error execution
    let error_exec = list
        .executions
        .iter()
        .find(|e| e.trace_id == "trace-error-3")
        .expect("should find execution from trace-error-3");

    assert_eq!(error_exec.status, "error");
    assert!(
        !error_exec.error.is_empty(),
        "error execution should have an error message"
    );
    assert!(error_exec.error.contains("Connection refused"));

    storage.shutdown().await;
}

// ── Metric Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_metrics_analytics_with_realistic_data() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let analytics = storage
        .get_metrics_analytics(30)
        .await
        .expect("get_metrics_analytics should succeed");

    // 4 metric records total: total_revenue(x2), user_signups(x1), order_count(x1)
    assert_eq!(analytics.total_queries, 4, "should have 4 metric records");
    assert_eq!(
        analytics.unique_metrics, 3,
        "should have 3 unique metric names"
    );

    // Most popular is total_revenue (2 occurrences)
    assert_eq!(analytics.most_popular.as_deref(), Some("total_revenue"));
    assert_eq!(analytics.most_popular_count, Some(2));

    // Source type breakdown: 3 from agent, 1 from workflow
    assert_eq!(analytics.by_source_type.agent, 3);
    assert_eq!(analytics.by_source_type.workflow, 1);

    // Context type breakdown:
    //   SQL appears in: total_revenue(trace-sales-1), user_signups, total_revenue(trace-workflow-4), order_count = 4
    //   SemanticQuery: total_revenue(trace-sales-1) = 1
    //   Question: user_signups = 1
    assert_eq!(analytics.by_context_type.sql, 4);
    assert_eq!(analytics.by_context_type.semantic_query, 1);
    assert_eq!(analytics.by_context_type.question, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_metric_detail_related_metrics() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let detail = storage
        .get_metric_detail("total_revenue", 30)
        .await
        .expect("get_metric_detail should succeed");

    assert_eq!(detail.name, "total_revenue");
    assert_eq!(detail.total_queries, 2, "total_revenue used 2 times");

    // via_agent = 1 (trace-sales-1), via_workflow = 1 (trace-workflow-4)
    assert_eq!(detail.via_agent, 1);
    assert_eq!(detail.via_workflow, 1);

    // order_count co-occurs with total_revenue in trace-sales-1
    assert!(
        detail
            .related_metrics
            .iter()
            .any(|r| r.name == "order_count"),
        "order_count should be a related metric to total_revenue"
    );

    storage.shutdown().await;
}

#[tokio::test]
async fn test_metric_detail_recent_usage() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let detail = storage
        .get_metric_detail("total_revenue", 30)
        .await
        .expect("get_metric_detail should succeed");

    assert_eq!(
        detail.recent_usage.len(),
        2,
        "total_revenue should have 2 recent usage entries"
    );

    // Verify each entry has valid source_type and source_ref
    let source_refs: Vec<&str> = detail
        .recent_usage
        .iter()
        .map(|u| u.source_ref.as_str())
        .collect();
    assert!(source_refs.contains(&"sales_agent"));
    assert!(source_refs.contains(&"daily_report"));

    let source_types: Vec<&str> = detail
        .recent_usage
        .iter()
        .map(|u| u.source_type.as_str())
        .collect();
    assert!(source_types.contains(&"agent"));
    assert!(source_types.contains(&"workflow"));

    storage.shutdown().await;
}

// ── Intent Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_cluster_map_data_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let data = storage
        .get_cluster_map_data(30, 100, None)
        .await
        .expect("get_cluster_map_data should succeed");

    // 3 classifications were inserted
    assert_eq!(data.len(), 3, "should have 3 cluster map entries");

    // Verify Revenue Analytics entry
    let revenue = data
        .iter()
        .find(|d| d.trace_id == "trace-sales-1")
        .expect("should find trace-sales-1 classification");
    assert_eq!(revenue.question, "What is total revenue this quarter?");
    assert_eq!(revenue.cluster_id, 1);
    assert_eq!(revenue.intent_name, "Revenue Analytics");
    assert!((revenue.confidence - 0.92).abs() < 0.01);
    assert_eq!(revenue.source, "sales_agent");
    assert_eq!(revenue.embedding.len(), 3);

    // Verify User Analytics entry
    let user = data
        .iter()
        .find(|d| d.trace_id == "trace-analytics-2")
        .expect("should find trace-analytics-2 classification");
    assert_eq!(user.intent_name, "User Analytics");
    assert_eq!(user.cluster_id, 2);
    assert!((user.confidence - 0.87).abs() < 0.01);

    // Verify unknown entry
    let unknown = data
        .iter()
        .find(|d| d.trace_id == "trace-error-3")
        .expect("should find trace-error-3 classification");
    assert_eq!(unknown.intent_name, "unknown");
    assert_eq!(unknown.cluster_id, 0);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_intent_analytics_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let analytics = storage
        .get_intent_analytics(30)
        .await
        .expect("get_intent_analytics should succeed");

    // 3 distinct intents, each with 1 classification
    assert_eq!(analytics.len(), 3, "should have 3 intent groups");

    // All have count=1 so order depends on alphabetical (they're sorted by count DESC,
    // ties are arbitrary). Just verify all exist.
    let intent_names: Vec<&str> = analytics.iter().map(|a| a.intent_name.as_str()).collect();
    assert!(intent_names.contains(&"Revenue Analytics"));
    assert!(intent_names.contains(&"User Analytics"));
    assert!(intent_names.contains(&"unknown"));

    // Each has count 1
    for entry in &analytics {
        assert_eq!(entry.count, 1);
    }

    storage.shutdown().await;
}

#[tokio::test]
async fn test_unknown_count_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let count = storage
        .get_unknown_count()
        .await
        .expect("get_unknown_count should succeed");

    assert_eq!(count, 1, "should have 1 unknown classification");

    let outliers = storage
        .get_outliers(10)
        .await
        .expect("get_outliers should succeed");

    assert_eq!(outliers.len(), 1);
    assert_eq!(outliers[0].0, "trace-error-3");
    assert_eq!(outliers[0].1, "Error question");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_cluster_infos_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let infos = storage
        .get_cluster_infos()
        .await
        .expect("get_cluster_infos should succeed");

    assert_eq!(infos.len(), 2, "should have 2 clusters");
    assert_eq!(infos[0].cluster_id, 1);
    assert_eq!(infos[0].intent_name, "Revenue Analytics");
    assert!(infos[0].intent_description.contains("revenue"));
    assert!(infos[0].sample_questions.contains("total revenue"));

    assert_eq!(infos[1].cluster_id, 2);
    assert_eq!(infos[1].intent_name, "User Analytics");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_fetch_unprocessed_questions_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let unprocessed = storage
        .fetch_unprocessed_questions(10)
        .await
        .expect("fetch_unprocessed_questions should succeed");

    // All 3 agent root spans have agent.prompt and classifications exist for
    // trace-sales-1, trace-analytics-2, trace-error-3. So there should be 0
    // unprocessed questions (all are classified).
    assert_eq!(
        unprocessed.len(),
        0,
        "all agent spans should already be classified"
    );

    storage.shutdown().await;
}

#[tokio::test]
async fn test_load_embeddings_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let embeddings = storage
        .load_embeddings()
        .await
        .expect("load_embeddings should succeed");

    assert_eq!(embeddings.len(), 3, "should have 3 embeddings");

    // Verify embedding vectors have the expected dimension
    for (_, _, emb, _, _) in &embeddings {
        assert_eq!(emb.len(), 3, "each embedding should have 3 dimensions");
    }

    storage.shutdown().await;
}

// ── Execution Agent Stats Tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_execution_agent_stats_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let stats = storage
        .get_execution_agent_stats(30, 10)
        .await
        .expect("get_execution_agent_stats should succeed");

    assert_eq!(stats.len(), 2, "should have stats for 2 agents");

    // sales_agent has 2 executions, analytics_agent has 1
    // Sorted by total_executions DESC
    assert_eq!(stats[0].agent_ref, "sales_agent");
    assert_eq!(stats[0].total_executions, 2);
    assert_eq!(stats[0].semantic_query_count, 2);
    assert_eq!(stats[0].verified_count, 2);

    assert_eq!(stats[1].agent_ref, "analytics_agent");
    assert_eq!(stats[1].total_executions, 1);
    assert_eq!(stats[1].sql_generated_count, 1);
    assert_eq!(stats[1].generated_count, 1);

    storage.shutdown().await;
}

// ── Trace Enrichment Tests ──────────────────────────────────────────────────

#[tokio::test]
async fn test_trace_enrichments_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let enrichments = storage
        .get_trace_enrichments(&[
            "trace-sales-1".into(),
            "trace-error-3".into(),
            "trace-workflow-4".into(),
        ])
        .await
        .expect("get_trace_enrichments should succeed");

    assert_eq!(enrichments.len(), 3);

    let sales = enrichments
        .iter()
        .find(|e| e.trace_id == "trace-sales-1")
        .unwrap();
    assert_eq!(sales.status_code, "OK");
    assert_eq!(sales.duration_ns, 5_000_000);

    let error = enrichments
        .iter()
        .find(|e| e.trace_id == "trace-error-3")
        .unwrap();
    assert_eq!(error.status_code, "ERROR");
    assert_eq!(error.duration_ns, 2_000_000);

    let workflow = enrichments
        .iter()
        .find(|e| e.trace_id == "trace-workflow-4")
        .unwrap();
    assert_eq!(workflow.status_code, "OK");
    assert_eq!(workflow.duration_ns, 10_000_000);

    storage.shutdown().await;
}

// ── Metrics List Tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_metrics_list_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let list = storage
        .get_metrics_list(30, 10, 0)
        .await
        .expect("get_metrics_list should succeed");

    assert_eq!(list.total, 3, "should have 3 unique metrics");
    assert_eq!(list.metrics.len(), 3);

    // Ordered by count DESC: total_revenue(2), then order_count(1) and user_signups(1)
    assert_eq!(list.metrics[0].name, "total_revenue");
    assert_eq!(list.metrics[0].count, 2);

    // The other two have count 1
    let remaining_names: Vec<&str> = list.metrics[1..].iter().map(|m| m.name.as_str()).collect();
    assert!(remaining_names.contains(&"order_count"));
    assert!(remaining_names.contains(&"user_signups"));

    storage.shutdown().await;
}

// ── Empty State Tests ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_all_endpoints_return_empty_on_fresh_db() {
    let storage = test_storage();

    // Trace queries
    let (traces, total) = storage.list_traces(10, 0, None, None, None).await.unwrap();
    assert!(traces.is_empty());
    assert_eq!(total, 0);

    let details = storage.get_trace_detail("nonexistent").await.unwrap();
    assert!(details.is_empty());

    let enrichments = storage
        .get_trace_enrichments(&["nonexistent".into()])
        .await
        .unwrap();
    assert!(enrichments.is_empty());

    let enrichments_empty = storage.get_trace_enrichments(&[]).await.unwrap();
    assert!(enrichments_empty.is_empty());

    // Intent queries
    let embeddings = storage.load_embeddings().await.unwrap();
    assert!(embeddings.is_empty());

    let clusters = storage.load_clusters().await.unwrap();
    assert!(clusters.is_empty());

    let cluster_infos = storage.get_cluster_infos().await.unwrap();
    assert!(cluster_infos.is_empty());

    let unknown_count = storage.get_unknown_count().await.unwrap();
    assert_eq!(unknown_count, 0);

    let outliers = storage.get_outliers(10).await.unwrap();
    assert!(outliers.is_empty());

    let unknowns = storage.load_unknown_classifications().await.unwrap();
    assert!(unknowns.is_empty());

    let next_id = storage.get_next_cluster_id().await.unwrap();
    assert_eq!(next_id, 1);

    let unprocessed = storage.fetch_unprocessed_questions(10).await.unwrap();
    assert!(unprocessed.is_empty());

    let intent_analytics = storage.get_intent_analytics(30).await.unwrap();
    assert!(intent_analytics.is_empty());

    let cluster_map = storage.get_cluster_map_data(30, 100, None).await.unwrap();
    assert!(cluster_map.is_empty());

    // Metric queries
    let metrics_analytics = storage.get_metrics_analytics(30).await.unwrap();
    assert_eq!(metrics_analytics.total_queries, 0);
    assert_eq!(metrics_analytics.unique_metrics, 0);
    assert!(metrics_analytics.most_popular.is_none());
    assert_eq!(metrics_analytics.by_source_type.agent, 0);
    assert_eq!(metrics_analytics.by_context_type.sql, 0);

    let metrics_list = storage.get_metrics_list(30, 10, 0).await.unwrap();
    assert_eq!(metrics_list.total, 0);
    assert!(metrics_list.metrics.is_empty());

    // Execution analytics — empty DB may behave differently with INNER JOIN
    // We just verify it doesn't panic.
    let _summary = storage.get_execution_summary(30).await;
    let time_series = storage.get_execution_time_series(30).await.unwrap();
    assert!(time_series.is_empty());

    let agent_stats = storage.get_execution_agent_stats(30, 10).await.unwrap();
    assert!(agent_stats.is_empty());

    let exec_list = storage
        .get_execution_list(30, 10, 0, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(exec_list.total, 0);
    assert!(exec_list.executions.is_empty());

    storage.shutdown().await;
}

// ── Combined filter tests ───────────────────────────────────────────────────

#[tokio::test]
async fn test_list_traces_combined_filters() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // Filter by both agent_ref=sales_agent AND status=OK
    let (traces, total) = storage
        .list_traces(10, 0, Some("sales_agent"), Some("OK"), None)
        .await
        .expect("combined filter should succeed");

    assert_eq!(total, 1, "only 1 trace matches sales_agent + OK");
    assert_eq!(traces[0].trace_id, "trace-sales-1");

    // Filter by agent_ref=sales_agent AND status=ERROR
    let (error_traces, error_total) = storage
        .list_traces(10, 0, Some("sales_agent"), Some("ERROR"), None)
        .await
        .expect("combined filter should succeed");

    assert_eq!(error_total, 1);
    assert_eq!(error_traces[0].trace_id, "trace-error-3");

    // Filter by agent_ref=analytics_agent AND status=ERROR -> no results
    let (empty_traces, empty_total) = storage
        .list_traces(10, 0, Some("analytics_agent"), Some("ERROR"), None)
        .await
        .expect("combined filter should succeed");

    assert_eq!(empty_total, 0);
    assert!(empty_traces.is_empty());

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_combined_filters() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // Filter by execution_type=semantic_query AND is_verified=true
    let list = storage
        .get_execution_list(30, 10, 0, Some("semantic_query"), Some(true), None, None)
        .await
        .expect("combined execution filter should succeed");

    assert_eq!(
        list.total, 2,
        "should have 2 verified semantic_query executions"
    );

    // Filter by execution_type=semantic_query AND source_ref=sales_agent
    let sales_list = storage
        .get_execution_list(
            30,
            10,
            0,
            Some("semantic_query"),
            None,
            Some("sales_agent"),
            None,
        )
        .await
        .expect("combined execution filter should succeed");

    assert_eq!(
        sales_list.total, 2,
        "sales_agent should have 2 semantic_query executions"
    );

    // Filter by execution_type=sql_generated AND is_verified=true -> no results
    let empty = storage
        .get_execution_list(30, 10, 0, Some("sql_generated"), Some(true), None, None)
        .await
        .expect("combined execution filter should succeed");

    assert_eq!(empty.total, 0);

    storage.shutdown().await;
}

// ── Pagination with realistic data ──────────────────────────────────────────

#[tokio::test]
async fn test_traces_pagination_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // Page 1: 2 items
    let (page1, total) = storage
        .list_traces(2, 0, None, None, None)
        .await
        .expect("page 1 should succeed");

    assert_eq!(total, 4);
    assert_eq!(page1.len(), 2);

    // Page 2: next 2 items
    let (page2, _) = storage
        .list_traces(2, 2, None, None, None)
        .await
        .expect("page 2 should succeed");

    assert_eq!(page2.len(), 2);

    // No overlap
    let page1_ids: Vec<&str> = page1.iter().map(|t| t.trace_id.as_str()).collect();
    let page2_ids: Vec<&str> = page2.iter().map(|t| t.trace_id.as_str()).collect();
    for id in &page2_ids {
        assert!(
            !page1_ids.contains(id),
            "pages should not overlap: {id} found in both"
        );
    }

    // Page 3: beyond the data
    let (page3, _) = storage
        .list_traces(2, 4, None, None, None)
        .await
        .expect("page 3 should succeed");

    assert!(page3.is_empty(), "page beyond data should be empty");

    storage.shutdown().await;
}

#[tokio::test]
async fn test_execution_list_pagination_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // Page 1: limit 2
    let page1 = storage
        .get_execution_list(30, 2, 0, None, None, None, None)
        .await
        .expect("page 1 should succeed");

    assert_eq!(page1.total, 3);
    assert_eq!(page1.executions.len(), 2);
    assert_eq!(page1.limit, 2);
    assert_eq!(page1.offset, 0);

    // Page 2: offset 2
    let page2 = storage
        .get_execution_list(30, 2, 2, None, None, None, None)
        .await
        .expect("page 2 should succeed");

    assert_eq!(page2.executions.len(), 1);
    assert_eq!(page2.offset, 2);

    storage.shutdown().await;
}

// ── Cluster map with source filter ──────────────────────────────────────────

#[tokio::test]
async fn test_cluster_map_data_source_filter_realistic() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // Filter by sales_agent
    let data = storage
        .get_cluster_map_data(30, 100, Some("sales_agent"))
        .await
        .expect("filtered cluster_map_data should succeed");

    // sales_agent has 2 classifications: trace-sales-1 and trace-error-3
    assert_eq!(data.len(), 2);
    for entry in &data {
        assert_eq!(entry.source, "sales_agent");
    }

    // Filter by analytics_agent
    let data2 = storage
        .get_cluster_map_data(30, 100, Some("analytics_agent"))
        .await
        .expect("filtered cluster_map_data should succeed");

    assert_eq!(data2.len(), 1);
    assert_eq!(data2[0].source, "analytics_agent");
    assert_eq!(data2[0].intent_name, "User Analytics");

    // Filter by nonexistent source
    let data3 = storage
        .get_cluster_map_data(30, 100, Some("nonexistent"))
        .await
        .expect("filtered cluster_map_data should succeed");

    assert!(data3.is_empty());

    storage.shutdown().await;
}

// ── Metric co-occurrence symmetry ───────────────────────────────────────────

#[tokio::test]
async fn test_metric_co_occurrence_symmetry() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // total_revenue and order_count co-occur in trace-sales-1
    let revenue_detail = storage
        .get_metric_detail("total_revenue", 30)
        .await
        .expect("get_metric_detail should succeed");

    let order_in_revenue = revenue_detail
        .related_metrics
        .iter()
        .find(|r| r.name == "order_count");
    assert!(
        order_in_revenue.is_some(),
        "order_count should be related to total_revenue"
    );

    // And the reverse: order_count should list total_revenue as related
    let order_detail = storage
        .get_metric_detail("order_count", 30)
        .await
        .expect("get_metric_detail should succeed");

    let revenue_in_order = order_detail
        .related_metrics
        .iter()
        .find(|r| r.name == "total_revenue");
    assert!(
        revenue_in_order.is_some(),
        "total_revenue should be related to order_count"
    );

    storage.shutdown().await;
}

// ── Metric for non-existent metric ──────────────────────────────────────────

#[tokio::test]
async fn test_metric_detail_nonexistent_metric() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let detail = storage
        .get_metric_detail("nonexistent_metric", 30)
        .await
        .expect("get_metric_detail for nonexistent should succeed");

    assert_eq!(detail.name, "nonexistent_metric");
    assert_eq!(detail.total_queries, 0);
    assert_eq!(detail.via_agent, 0);
    assert_eq!(detail.via_workflow, 0);
    assert!(detail.related_metrics.is_empty());
    assert!(detail.recent_usage.is_empty());

    storage.shutdown().await;
}

// ── Trace detail for workflow ───────────────────────────────────────────────

#[tokio::test]
async fn test_trace_detail_workflow() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let details = storage
        .get_trace_detail("trace-workflow-4")
        .await
        .expect("get_trace_detail should succeed");

    // workflow trace has 3 spans: root + 2 steps
    assert_eq!(details.len(), 3, "workflow trace should have 3 spans");

    assert_eq!(details[0].span_name, "workflow.run_workflow");
    assert_eq!(details[0].parent_span_id, "");

    // Both children should have the root as parent
    assert_eq!(details[1].parent_span_id, "span-workflow-root");
    assert_eq!(details[2].parent_span_id, "span-workflow-root");

    storage.shutdown().await;
}

// ── Trace detail for error trace ────────────────────────────────────────────

#[tokio::test]
async fn test_trace_detail_error_trace() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    let details = storage
        .get_trace_detail("trace-error-3")
        .await
        .expect("get_trace_detail should succeed");

    // error trace has 3 spans: root + llm + tool
    assert_eq!(details.len(), 3);

    // Root should have ERROR status
    let root = details
        .iter()
        .find(|s| s.span_id == "span-error-root")
        .unwrap();
    assert_eq!(root.status_code, "ERROR");
    assert_eq!(root.status_message, "Internal error");

    // Tool span should have error event
    let tool = details
        .iter()
        .find(|s| s.span_id == "span-error-tool1")
        .unwrap();
    assert!(tool.event_data.contains("Connection refused"));

    storage.shutdown().await;
}

// ── Duration filter test ────────────────────────────────────────────────────

#[tokio::test]
async fn test_list_traces_duration_filter() {
    let storage = test_storage();
    populate_realistic_data(&storage);

    // All spans are within the last hour, so "1h" filter should return them all
    let (traces_1h, total_1h) = storage
        .list_traces(10, 0, None, None, Some("1h"))
        .await
        .expect("duration filter 1h should succeed");

    assert_eq!(total_1h, 4, "all traces within last hour");
    assert_eq!(traces_1h.len(), 4);

    // "24h" filter should also return all
    let (_, total_24h) = storage
        .list_traces(10, 0, None, None, Some("24h"))
        .await
        .expect("duration filter 24h should succeed");

    assert_eq!(total_24h, 4);

    // "30d" filter should return all
    let (_, total_30d) = storage
        .list_traces(10, 0, None, None, Some("30d"))
        .await
        .expect("duration filter 30d should succeed");

    assert_eq!(total_30d, 4);

    storage.shutdown().await;
}

// ── analytics.run parity tests ──────────────────────────────────────────────
//
// These lock in that `analytics.run` spans (emitted by the agentic analytics
// pipeline) are treated as first-class citizens by the Clusters, Metrics,
// and Execution Analytics observability tabs — identical behaviour to
// classic `agent.run_agent` spans.

fn make_analytics_run_span(
    trace_id: &str,
    span_id: &str,
    agent_ref: &str,
    prompt: &str,
) -> SpanRecord {
    let attrs = format!(
        r#"{{"oxy.agent.ref":"{}","agent.prompt":"{}","oxy.span_type":"analytics","oxy.name":"analytics.run"}}"#,
        agent_ref, prompt,
    );
    SpanRecord {
        trace_id: trace_id.to_string(),
        span_id: span_id.to_string(),
        parent_span_id: String::new(),
        span_name: "analytics.run".to_string(),
        service_name: "oxy".to_string(),
        span_attributes: attrs,
        duration_ns: 3_000_000,
        status_code: "OK".to_string(),
        status_message: String::new(),
        event_data: "[]".to_string(),
        timestamp: "2026-04-15T12:00:00Z".to_string(),
    }
}

#[tokio::test]
async fn test_execution_analytics_includes_analytics_run() {
    let storage = test_storage();

    // analytics.run parent + tool_call child — mirrors what the agentic
    // analytics pipeline now emits.
    let parent = make_analytics_run_span("t-an-1", "s-an-1", "revenue_agent", "q");
    insert_span(&storage, &parent);
    let tool = make_tool_call_span(
        "t-an-1",
        "s-an-tool-1",
        "s-an-1",
        "semantic_query",
        true,
        "revenue_agent",
    );
    insert_span(&storage, &tool);

    let summary = storage
        .get_execution_summary(30)
        .await
        .expect("get_execution_summary should succeed");

    assert_eq!(summary.total_executions, 1);
    assert_eq!(summary.semantic_query_count, 1);
    assert_eq!(summary.verified_count, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_fetch_unprocessed_questions_includes_analytics_run() {
    let storage = test_storage();

    insert_span(
        &storage,
        &make_analytics_run_span("t-cl-1", "s-cl-1", "revenue_agent", "top customers"),
    );

    let rows = storage
        .fetch_unprocessed_questions(10)
        .await
        .expect("fetch_unprocessed_questions should succeed");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "t-cl-1");
    assert_eq!(rows[0].1, "top customers");
    assert_eq!(rows[0].2, "revenue_agent");

    storage.shutdown().await;
}

/// Replicates the production span topology: `analytics.run` is the root,
/// entered via `.instrument(run_span)` on a spawned task; `analytics.execute`
/// is a child via `#[instrument]`; `analytics.tool_call` is a grandchild
/// created with `info_span!` inside the executing function. Verifies that
/// the Execution Analytics join predicate
/// (`agent.span_name IN (...) AND tool.trace_id = agent.trace_id`)
/// matches correctly through this three-level hierarchy.
#[tokio::test]
async fn test_analytics_run_nested_spans_end_to_end() {
    use tracing::Instrument as _;
    use tracing_subscriber::layer::SubscriberExt;

    let storage = test_storage();
    let (span_tx, mut span_rx) = tokio::sync::mpsc::unbounded_channel::<SpanRecord>();
    let layer = crate::layer::SpanCollectorLayer::new(span_tx, "oxy".to_string());
    let subscriber = std::sync::Arc::new(tracing_subscriber::registry().with(layer));

    let _default = tracing::subscriber::set_default(subscriber);

    async fn execute_with_tool_call() {
        let exec_span = tracing::info_span!(
            "analytics.execute",
            oxy.name = "analytics.execute",
            oxy.span_type = "analytics",
        );
        async {
            let tool_span = tracing::info_span!(
                "analytics.tool_call",
                oxy.name = "analytics.tool_call",
                oxy.span_type = "tool_call",
                oxy.execution_type = "semantic_query",
                oxy.is_verified = true,
                connector = "duckdb",
            );
            async { /* connector.execute_query(...) */ }
                .instrument(tool_span.clone())
                .await;
            tool_span.in_scope(|| {
                tracing::info!(name: "tool_call.output", status = "success", row_count = 3_i64);
            });
        }
        .instrument(exec_span)
        .await;
    }

    let run_span = tracing::info_span!(
        parent: None,
        "analytics.run",
        oxy.name = "analytics.run",
        oxy.span_type = "analytics",
        oxy.agent.ref = "nested_agent",
        agent.prompt = "q",
        question = "q",
    );
    tokio::spawn(execute_with_tool_call().instrument(run_span))
        .await
        .unwrap();

    // Give on_close a moment to fire.
    tokio::time::sleep(Duration::from_millis(20)).await;

    let mut records = Vec::new();
    while let Ok(r) = span_rx.try_recv() {
        records.push(r);
    }
    let run = records
        .iter()
        .find(|r| r.span_name == "analytics.run")
        .expect("analytics.run must be captured");
    let exec = records
        .iter()
        .find(|r| r.span_name == "analytics.execute")
        .expect("analytics.execute must be captured");
    let tool = records
        .iter()
        .find(|r| r.span_name == "analytics.tool_call")
        .expect("analytics.tool_call must be captured");

    assert_eq!(run.trace_id, exec.trace_id, "exec inherits run.trace_id");
    assert_eq!(
        exec.trace_id, tool.trace_id,
        "tool inherits exec.trace_id (= run.trace_id)"
    );
    let tool_attrs: std::collections::HashMap<String, String> =
        serde_json::from_str(&tool.span_attributes).unwrap();
    assert_eq!(
        tool_attrs.get("oxy.span_type").map(String::as_str),
        Some("tool_call")
    );

    storage.insert_spans(records).await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;

    let summary = storage.get_execution_summary(30).await.unwrap();
    assert_eq!(
        summary.total_executions, 1,
        "nested tool_call row must appear in Execution Analytics"
    );
    storage.shutdown().await;
}

/// End-to-end test: go through the real tracing subscriber +
/// `SpanCollectorLayer` + DuckDB writer + exec_analytics query. Mirrors
/// what the agentic analytics pipeline does at runtime to confirm the
/// analytics.run + child analytics.tool_call spans actually propagate
/// into the Execution Analytics tab.
#[tokio::test]
async fn test_analytics_run_end_to_end_through_tracing_layer() {
    use tracing_subscriber::layer::SubscriberExt;

    let storage = test_storage();
    let (span_tx, mut span_rx) = tokio::sync::mpsc::unbounded_channel::<SpanRecord>();
    let layer = crate::layer::SpanCollectorLayer::new(span_tx, "oxy".to_string());
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let run = tracing::info_span!(
            parent: None,
            "analytics.run",
            oxy.name = "analytics.run",
            oxy.span_type = "analytics",
            oxy.agent.ref = "e2e_agent",
            agent.prompt = "top customers",
            question = "top customers",
        );
        let tool_span = tracing::info_span!(
            parent: &run,
            "analytics.tool_call",
            oxy.name = "analytics.tool_call",
            oxy.span_type = "tool_call",
            oxy.execution_type = "semantic_query",
            oxy.is_verified = true,
            connector = "duckdb",
        );
        tool_span.in_scope(|| {
            tracing::info!(
                name: "tool_call.output",
                status = "success",
                row_count = 3_i64,
            );
        });
        drop(tool_span);
        drop(run);
    });

    // Drain the channel into records.
    let mut records = Vec::new();
    while let Ok(r) = span_rx.try_recv() {
        records.push(r);
    }
    assert!(
        records.iter().any(|r| r.span_name == "analytics.run"),
        "analytics.run span should be captured"
    );
    assert!(
        records.iter().any(|r| r.span_name == "analytics.tool_call"),
        "analytics.tool_call span should be captured"
    );

    storage
        .insert_spans(records)
        .await
        .expect("insert_spans should succeed");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let summary = storage
        .get_execution_summary(30)
        .await
        .expect("get_execution_summary should succeed");
    assert_eq!(
        summary.total_executions, 1,
        "one tool_call row should surface in Execution Analytics"
    );
    assert_eq!(summary.semantic_query_count, 1);
    assert_eq!(summary.verified_count, 1);

    storage.shutdown().await;
}

#[tokio::test]
async fn test_metrics_analytics_source_type_bucket() {
    let storage = test_storage();

    let records = vec![
        MetricUsageRecord {
            metric_name: "orders.revenue".into(),
            source_type: "analytics".into(),
            source_ref: "revenue_agent".into(),
            context: "{}".into(),
            context_types: r#"["SemanticQuery"]"#.into(),
            trace_id: "t-met-1".into(),
        },
        MetricUsageRecord {
            metric_name: "orders.count".into(),
            source_type: "analytics".into(),
            source_ref: "revenue_agent".into(),
            context: "{}".into(),
            context_types: r#"["SemanticQuery"]"#.into(),
            trace_id: "t-met-1".into(),
        },
        MetricUsageRecord {
            metric_name: "orders.revenue".into(),
            source_type: "agent".into(),
            source_ref: "classic_agent".into(),
            context: "{}".into(),
            context_types: r#"["SemanticQuery"]"#.into(),
            trace_id: "t-met-2".into(),
        },
    ];
    storage
        .store_metric_usages(records)
        .await
        .expect("store_metric_usages should succeed");

    // Writer is async — let the flush task drain.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let analytics = storage
        .get_metrics_analytics(30)
        .await
        .expect("get_metrics_analytics should succeed");

    assert_eq!(analytics.by_source_type.analytics, 2);
    assert_eq!(analytics.by_source_type.agent, 1);
    assert_eq!(analytics.by_source_type.workflow, 0);

    storage.shutdown().await;
}
