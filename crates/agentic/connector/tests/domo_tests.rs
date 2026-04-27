//! Integration tests for the DOMO connector.
//!
//! DOMO has no public container image, so we stand up a
//! [`wiremock`]-backed fake that speaks the same `POST
//! /query/v1/execute/{datasetId}` contract and assert the connector decodes
//! responses correctly.

#![cfg(feature = "domo")]

use agentic_connector::{DatabaseConnector, DomoConnector};
use agentic_core::result::{TypedDataType, TypedRowStream, TypedValue};
use futures::StreamExt;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DATASET_ID: &str = "ds_abc123";

async fn wire_server(body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/query/v1/execute/{DATASET_ID}")))
        .and(header_exists("X-DOMO-Developer-Token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;
    server
}

async fn build_connector(server: &MockServer) -> DomoConnector {
    DomoConnector::new(
        server.uri(),
        "fake-token".to_string(),
        DATASET_ID.to_string(),
    )
    .await
    .expect("connector builds")
}

async fn collect_typed(stream: TypedRowStream) -> Vec<Vec<TypedValue>> {
    let TypedRowStream { mut rows, .. } = stream;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await {
        out.push(row.expect("row decode"));
    }
    out
}

#[tokio::test]
async fn execute_query_full_decodes_scalars() {
    let server = wire_server(serde_json::json!({
        "columns":  ["id", "name", "amount", "active"],
        "metadata": [{"type": "LONG"}, {"type": "STRING"},
                     {"type": "DECIMAL"}, {"type": "BOOLEAN"}],
        "rows": [
            [1, "alpha", "10.50", true],
            [2, "beta",  "20.00", false],
            [3, null,    null,    null],
        ],
    }))
    .await;

    let c = build_connector(&server).await;
    let stream = c
        .execute_query_full("SELECT * FROM t")
        .await
        .expect("query succeeds");

    assert_eq!(stream.columns.len(), 4);
    assert_eq!(stream.columns[0].data_type, TypedDataType::Int64);
    assert_eq!(stream.columns[1].data_type, TypedDataType::Text);
    // DOMO doesn't expose precision/scale in type metadata, so DECIMAL maps to Text.
    assert_eq!(stream.columns[2].data_type, TypedDataType::Text);
    assert_eq!(stream.columns[3].data_type, TypedDataType::Bool);

    let rows = collect_typed(stream).await;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][0], TypedValue::Int64(1));
    assert_eq!(rows[0][1], TypedValue::Text("alpha".into()));
    assert_eq!(rows[0][2], TypedValue::Text("10.50".into()));
    assert_eq!(rows[0][3], TypedValue::Bool(true));
    assert_eq!(rows[2][1], TypedValue::Null);
    assert_eq!(rows[2][3], TypedValue::Null);
}

#[tokio::test]
async fn execute_query_full_decodes_date_and_datetime() {
    let server = wire_server(serde_json::json!({
        "columns":  ["d", "dt"],
        "metadata": [{"type": "DATE"}, {"type": "DATETIME"}],
        "rows": [
            ["2026-04-22", "2026-04-22 12:34:56"],
            [null, null],
        ],
    }))
    .await;

    let c = build_connector(&server).await;
    let stream = c
        .execute_query_full("SELECT d, dt FROM t")
        .await
        .expect("query succeeds");
    let rows = collect_typed(stream).await;

    assert_eq!(rows[0][0], TypedValue::Date(20_565));
    assert!(matches!(rows[0][1], TypedValue::Timestamp(_)));
    assert_eq!(rows[1][0], TypedValue::Null);
    assert_eq!(rows[1][1], TypedValue::Null);
}

#[tokio::test]
async fn execute_query_returns_bounded_sample_and_stats() {
    let server = wire_server(serde_json::json!({
        "columns":  ["id", "name"],
        "metadata": [{"type": "LONG"}, {"type": "STRING"}],
        "rows": [
            [1, "alpha"],
            [2, "beta"],
            [3, "gamma"],
            [4, null],
            [5, "epsilon"],
        ],
    }))
    .await;

    let c = build_connector(&server).await;
    let res = c
        .execute_query("SELECT * FROM t", 2)
        .await
        .expect("query succeeds");

    assert_eq!(res.result.total_row_count, 5);
    assert_eq!(res.result.rows.len(), 2);
    assert!(res.result.truncated);

    // Stats.
    let name_stats = res
        .summary
        .columns
        .iter()
        .find(|s| s.name == "name")
        .expect("name column stats");
    assert_eq!(name_stats.null_count, 1);
    assert_eq!(name_stats.distinct_count, Some(4));

    let id_stats = res
        .summary
        .columns
        .iter()
        .find(|s| s.name == "id")
        .expect("id column stats");
    assert_eq!(id_stats.null_count, 0);
    assert_eq!(id_stats.distinct_count, Some(5));
    assert!((id_stats.mean.unwrap() - 3.0).abs() < 1e-9);
}

#[tokio::test]
async fn execute_query_full_surfaces_http_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/query/v1/execute/{DATASET_ID}")))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let c = build_connector(&server).await;
    let err = c
        .execute_query_full("SELECT * FROM t")
        .await
        .expect_err("500 must surface as QueryFailed");
    match err {
        agentic_connector::ConnectorError::QueryFailed { message, .. } => {
            assert!(message.contains("500"));
        }
        other => panic!("expected QueryFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_query_full_empty_result() {
    let server = wire_server(serde_json::json!({
        "columns": [],
        "metadata": [],
        "rows": [],
    }))
    .await;

    let c = build_connector(&server).await;
    let stream = c
        .execute_query_full("SELECT * FROM empty_t")
        .await
        .expect("query succeeds");

    assert_eq!(stream.columns.len(), 0);
    let rows = collect_typed(stream).await;
    assert_eq!(rows.len(), 0);
}
