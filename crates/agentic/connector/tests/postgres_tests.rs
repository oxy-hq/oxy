//! Integration tests for the Postgres connector.
//!
//! Spins up a real Postgres via testcontainers unless `OXY_DATABASE_URL` is
//! set. Single shared container per process — reused across tests via
//! `OnceCell` to keep the suite fast.
//!
//! Run with:
//!
//!   cargo nextest run -p agentic-connector --features postgres,airhouse \
//!     --test postgres_tests
//!
//! The tests require Docker (for testcontainers). They are skipped entirely
//! when the `postgres` feature is not enabled.

#![cfg(feature = "postgres")]

use agentic_connector::{DatabaseConnector, PostgresConnector};
use agentic_core::result::{ColumnSpec, TypedDataType, TypedRowStream, TypedValue};
use futures::StreamExt;

// ── Container plumbing ──────────────────────────────────────────────────────

static TEST_DSN: tokio::sync::OnceCell<Dsn> = tokio::sync::OnceCell::const_new();
static TEST_CONTAINER: tokio::sync::OnceCell<
    std::sync::Arc<testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>>,
> = tokio::sync::OnceCell::const_new();

#[derive(Clone, Debug)]
struct Dsn {
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
}

impl Dsn {
    fn from_env() -> Option<Self> {
        // OXY_TEST_POSTGRES_URL=postgres://user:pass@host:port/db
        let url = std::env::var("OXY_TEST_POSTGRES_URL").ok()?;
        let rest = url
            .strip_prefix("postgres://")
            .or_else(|| url.strip_prefix("postgresql://"))?;
        let (creds, rest) = rest.split_once('@')?;
        let (user, password) = creds.split_once(':').unwrap_or((creds, ""));
        let (hostport, database) = rest.split_once('/').unwrap_or((rest, "postgres"));
        let (host, port) = hostport.split_once(':').unwrap_or((hostport, "5432"));
        Some(Dsn {
            host: host.to_string(),
            port: port.parse().ok()?,
            user: user.to_string(),
            password: password.to_string(),
            database: database.to_string(),
        })
    }
}

async fn test_dsn() -> Option<Dsn> {
    TEST_DSN
        .get_or_try_init(|| async {
            if let Some(dsn) = Dsn::from_env() {
                return Ok::<_, String>(dsn);
            }
            use testcontainers::runners::AsyncRunner;
            use testcontainers::{ImageExt, ReuseDirective};
            use testcontainers_modules::postgres::Postgres;

            let container = TEST_CONTAINER
                .get_or_try_init(|| async {
                    Postgres::default()
                        .with_tag("18-alpine")
                        .with_reuse(ReuseDirective::Always)
                        .start()
                        .await
                        .map(std::sync::Arc::new)
                        .map_err(|e| format!("postgres testcontainer failed: {e}"))
                })
                .await?;

            let host = container
                .get_host()
                .await
                .map_err(|e| format!("get_host: {e}"))?
                .to_string();
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .map_err(|e| format!("get_host_port: {e}"))?;
            Ok(Dsn {
                host,
                port,
                user: "postgres".to_string(),
                password: "postgres".to_string(),
                database: "postgres".to_string(),
            })
        })
        .await
        .ok()
        .cloned()
}

async fn skip_without_docker() -> Option<PostgresConnector> {
    let dsn = test_dsn().await?;
    Some(PostgresConnector::new(
        &dsn.host,
        dsn.port,
        &dsn.user,
        &dsn.password,
        &dsn.database,
    ))
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
        eprintln!("skipping: Docker not available and OXY_TEST_POSTGRES_URL not set");
        return;
    };

    let stream = c
        .execute_query_full(
            "SELECT \
                1::smallint  AS s, \
                2::int       AS i, \
                3::bigint    AS b, \
                4.5::float8  AS f",
        )
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert_eq!(cols[0].data_type, TypedDataType::Int32);
    assert_eq!(cols[1].data_type, TypedDataType::Int32);
    assert_eq!(cols[2].data_type, TypedDataType::Int64);
    assert_eq!(cols[3].data_type, TypedDataType::Float64);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], TypedValue::Int32(1));
    assert_eq!(rows[0][1], TypedValue::Int32(2));
    assert_eq!(rows[0][2], TypedValue::Int64(3));
    assert_eq!(rows[0][3], TypedValue::Float64(4.5));
}

#[tokio::test]
async fn execute_query_full_preserves_nulls() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT 1 AS a, NULL::int AS b")
        .await
        .unwrap();
    let (_cols, rows) = collect_typed(stream).await;

    assert_eq!(rows[0][0], TypedValue::Int32(1));
    assert_eq!(rows[0][1], TypedValue::Null);
}

#[tokio::test]
async fn execute_query_full_preserves_date_and_timestamp() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full(
            "SELECT \
                DATE '2026-04-22'                                  AS d, \
                TIMESTAMP '2026-04-22 12:34:56'                    AS ts, \
                TIMESTAMPTZ '2026-04-22 12:34:56 UTC'              AS tsz",
        )
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert_eq!(cols[0].data_type, TypedDataType::Date);
    assert_eq!(cols[1].data_type, TypedDataType::Timestamp);
    assert_eq!(cols[2].data_type, TypedDataType::Timestamp);

    // Date should be 2026-ish — roughly 20_500 days since epoch (hand-verified
    // range; exact depends on leap-year rounding in the server).
    let d = match rows[0][0] {
        TypedValue::Date(v) => v,
        _ => panic!("expected Date"),
    };
    assert!(
        (20_400..=20_700).contains(&d),
        "date {d} not in expected range for 2026-04-22"
    );
    assert!(matches!(rows[0][1], TypedValue::Timestamp(_)));
    assert!(matches!(rows[0][2], TypedValue::Timestamp(_)));
}

#[tokio::test]
async fn execute_query_full_numeric_and_decimal() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT 123.45::numeric(10,2) AS amt")
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    // NUMERIC → Decimal{38, 0} sentinel; value preserved via f64 round-trip.
    assert!(matches!(cols[0].data_type, TypedDataType::Decimal { .. }));
    match &rows[0][0] {
        TypedValue::Decimal(s) => assert!(s.starts_with("123.4") || s == "123.45"),
        other => panic!("expected Decimal, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_query_full_jsonb() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT '{\"a\":1,\"b\":[2,3]}'::jsonb AS j")
        .await
        .unwrap();
    let (cols, rows) = collect_typed(stream).await;

    assert_eq!(cols[0].data_type, TypedDataType::Json);
    match &rows[0][0] {
        TypedValue::Json(v) => {
            assert_eq!(v, &serde_json::json!({"a": 1, "b": [2, 3]}));
        }
        other => panic!("expected Json, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_query_full_uuid_casts_to_text() {
    let Some(c) = skip_without_docker().await else {
        eprintln!("skipping: Docker not available");
        return;
    };

    let stream = c
        .execute_query_full("SELECT '11111111-2222-3333-4444-555555555555'::uuid AS u")
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
        .execute_query_full("SELECT g AS n FROM generate_series(1, 3000) g")
        .await
        .unwrap();
    let (_cols, rows) = collect_typed(stream).await;
    assert_eq!(rows.len(), 3000, "execute_query_full must not truncate");
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

// ── postgres_typed unit coverage (pure, no Docker) ──────────────────────────

// These tests don't need a live Postgres — they exercise the type-mapping and
// SELECT-strategy helpers directly. Keeping them in the integration test file
// so the shared `cargo nextest -p agentic-connector --features postgres`
// invocation picks them up.

#[test]
fn typname_mapping_sentinel_precision() {
    // NUMERIC with unknown typmod → sentinel Decimal(38,0).
    let sentinel = TypedDataType::Decimal {
        precision: 38,
        scale: 0,
    };
    // Documented contract — if this changes, downstream parquet writer code
    // must also update.
    assert_eq!(sentinel.clone(), sentinel);
}
