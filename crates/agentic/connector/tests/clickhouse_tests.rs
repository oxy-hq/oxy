//! Integration tests for the ClickHouse connector.
//!
//! Spins up a real ClickHouse server via testcontainers (image
//! `clickhouse/clickhouse-server:24-alpine`). Runs one container per test
//! process, reused across tests via `OnceCell`. Skips gracefully if Docker
//! is unavailable.
//!
//! Run with:
//!
//!   cargo nextest run -p agentic-connector --features clickhouse \
//!     --test clickhouse_tests

#![cfg(feature = "clickhouse")]

use agentic_connector::{ClickHouseConnector, DatabaseConnector};
use agentic_core::result::{ColumnSpec, TypedDataType, TypedRowStream, TypedValue};
use futures::StreamExt;

// ── Container plumbing ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct Conn {
    url: String,
    user: String,
    password: String,
    database: String,
}

static TEST_CONN: tokio::sync::OnceCell<Conn> = tokio::sync::OnceCell::const_new();
static TEST_CONTAINER: tokio::sync::OnceCell<
    std::sync::Arc<testcontainers::ContainerAsync<testcontainers_modules::clickhouse::ClickHouse>>,
> = tokio::sync::OnceCell::const_new();

async fn test_connection() -> Option<Conn> {
    TEST_CONN
        .get_or_try_init(|| async {
            // Allow external CH via env for local iteration.
            if let Ok(url) = std::env::var("OXY_TEST_CLICKHOUSE_URL") {
                return Ok::<_, String>(Conn {
                    url,
                    user: std::env::var("OXY_TEST_CLICKHOUSE_USER")
                        .unwrap_or_else(|_| "default".into()),
                    password: std::env::var("OXY_TEST_CLICKHOUSE_PASSWORD").unwrap_or_default(),
                    database: std::env::var("OXY_TEST_CLICKHOUSE_DB")
                        .unwrap_or_else(|_| "default".into()),
                });
            }

            use testcontainers::runners::AsyncRunner;
            use testcontainers::{ImageExt, ReuseDirective};
            use testcontainers_modules::clickhouse::ClickHouse;

            let container = TEST_CONTAINER
                .get_or_try_init(|| async {
                    let img = ClickHouse::default().with_reuse(ReuseDirective::Always);
                    img.start()
                        .await
                        .map(std::sync::Arc::new)
                        .map_err(|e| format!("clickhouse testcontainer failed: {e}"))
                })
                .await?;

            let host = container
                .get_host()
                .await
                .map_err(|e| format!("get_host: {e}"))?
                .to_string();
            let port = container
                .get_host_port_ipv4(8123)
                .await
                .map_err(|e| format!("get_host_port: {e}"))?;

            Ok(Conn {
                url: format!("http://{host}:{port}"),
                user: "default".to_string(),
                password: String::new(),
                database: "default".to_string(),
            })
        })
        .await
        .ok()
        .cloned()
}

async fn skip_without_docker() -> Option<ClickHouseConnector> {
    let c = test_connection().await?;
    ClickHouseConnector::new(c.url, c.user, c.password, c.database)
        .await
        .ok()
}

// ── Helpers ─────────────────────────────────────────────────────────────────

async fn collect_typed(stream: TypedRowStream) -> (Vec<ColumnSpec>, Vec<Vec<TypedValue>>) {
    let TypedRowStream { columns, mut rows } = stream;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await {
        match row {
            Ok(cells) => out.push(cells),
            Err(e) => panic!("row stream error: {e}"),
        }
    }
    (columns, out)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn execute_query_full_preserves_int_types() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full(
            "SELECT \
                toInt32(1)   AS i32, \
                toInt64(2)   AS i64, \
                toFloat64(3.5) AS f, \
                toUInt32(4)  AS u32",
        )
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert_eq!(cols[0].data_type, TypedDataType::Int32);
    assert_eq!(cols[1].data_type, TypedDataType::Int64);
    assert_eq!(cols[2].data_type, TypedDataType::Float64);
    // UInt32 widens into Int64 on the Rust side.
    assert_eq!(cols[3].data_type, TypedDataType::Int64);

    assert_eq!(rows[0][0], TypedValue::Int32(1));
    assert_eq!(rows[0][1], TypedValue::Int64(2));
    assert_eq!(rows[0][2], TypedValue::Float64(3.5));
    assert_eq!(rows[0][3], TypedValue::Int64(4));
}

#[tokio::test]
async fn execute_query_full_preserves_nulls_via_nullable() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full(
            "SELECT toNullable(1)::Nullable(Int32) AS a, CAST(NULL AS Nullable(Int32)) AS b",
        )
        .await
        .unwrap();
    let (_cols, rows) = collect_typed(stream).await;

    assert_eq!(rows[0][0], TypedValue::Int32(1));
    assert_eq!(rows[0][1], TypedValue::Null);
}

#[tokio::test]
async fn execute_query_full_date_and_datetime() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full(
            "SELECT \
                toDate('2026-04-22')              AS d, \
                toDateTime('2026-04-22 12:34:56') AS dt",
        )
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert_eq!(cols[0].data_type, TypedDataType::Date);
    assert_eq!(cols[1].data_type, TypedDataType::Timestamp);

    let d = match rows[0][0] {
        TypedValue::Date(v) => v,
        _ => panic!("expected Date"),
    };
    assert!(
        (20_400..=20_700).contains(&d),
        "date {d} out of expected 2026-ish range"
    );
    assert!(matches!(rows[0][1], TypedValue::Timestamp(_)));
}

#[tokio::test]
async fn execute_query_full_decimal_preserves_string() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT toDecimal64(123.45, 2) AS amt")
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert!(matches!(cols[0].data_type, TypedDataType::Decimal { .. }));
    match &rows[0][0] {
        TypedValue::Decimal(s) => {
            // Exact textual representation is stable across CH versions.
            assert_eq!(s, "123.45");
        }
        other => panic!("expected Decimal, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_query_full_array_as_json() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT [1, 2, 3] AS arr")
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert_eq!(cols[0].data_type, TypedDataType::Json);
    match &rows[0][0] {
        TypedValue::Json(j) => {
            // ClickHouse JSONCompact serialises Array(Int64) elements as
            // JSON strings (e.g. `"1"`) because 64-bit integers can exceed
            // the safe-integer range of JavaScript. Assert the structural
            // shape, not the per-element encoding.
            let arr = j.as_array().expect("array value");
            assert_eq!(arr.len(), 3);
        }
        other => panic!("expected Json, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_query_full_lowcardinality_string_is_text() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT CAST('hello' AS LowCardinality(String)) AS s")
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert_eq!(cols[0].data_type, TypedDataType::Text);
    assert_eq!(rows[0][0], TypedValue::Text("hello".into()));
}

#[tokio::test]
async fn execute_query_full_uuid_as_text() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT toUUID('11111111-2222-3333-4444-555555555555') AS u")
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert_eq!(cols[0].data_type, TypedDataType::Text);
    assert_eq!(
        rows[0][0],
        TypedValue::Text("11111111-2222-3333-4444-555555555555".into())
    );
}

#[tokio::test]
async fn execute_query_full_no_truncation() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT number FROM system.numbers LIMIT 3000")
        .await
        .unwrap();
    let (_cols, rows) = collect_typed(stream).await;
    assert_eq!(rows.len(), 3000);
}

#[tokio::test]
async fn execute_query_full_reports_query_errors() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let err = c
        .execute_query_full("SELECT * FROM does_not_exist_xyz")
        .await
        .expect_err("unknown table must error");
    match err {
        agentic_connector::ConnectorError::QueryFailed { .. } => {}
        other => panic!("expected QueryFailed, got {other:?}"),
    }
}
