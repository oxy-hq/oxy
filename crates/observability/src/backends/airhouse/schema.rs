//! DuckDB DDL for Airhouse-backed observability tables.
//!
//! Tables are prefixed with `oxy_obs_` so they don't collide with any
//! user-defined tables that may exist in the same Airhouse tenant.
//!
//! NOTE: DuckLake rejects PRIMARY KEY, UNIQUE, and CREATE INDEX statements
//! with "Not implemented Error". Keep DDL plain table definitions only;
//! query optimization relies on DuckLake's predicate pushdown over
//! day-partitioned parquet (see [`PARTITION_DDL`]), not catalog-level indexes.

pub const CREATE_SPANS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS oxy_obs_spans (
    trace_id VARCHAR NOT NULL,
    span_id VARCHAR NOT NULL,
    parent_span_id VARCHAR DEFAULT '',
    span_name VARCHAR NOT NULL,
    service_name VARCHAR DEFAULT 'oxy',
    span_attributes VARCHAR DEFAULT '{}',
    duration_ns BIGINT DEFAULT 0,
    status_code VARCHAR DEFAULT 'UNSET',
    status_message VARCHAR DEFAULT '',
    event_data VARCHAR DEFAULT '[]',
    timestamp TIMESTAMPTZ DEFAULT current_timestamp
)
"#;

pub const CREATE_INTENT_CLUSTERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS oxy_obs_intent_clusters (
    cluster_id INTEGER NOT NULL,
    intent_name VARCHAR NOT NULL,
    intent_description VARCHAR DEFAULT '',
    centroid FLOAT[],
    sample_questions VARCHAR DEFAULT '[]',
    question_count BIGINT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT current_timestamp,
    updated_at TIMESTAMPTZ DEFAULT current_timestamp
)
"#;

pub const CREATE_INTENT_CLASSIFICATIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS oxy_obs_intent_classifications (
    trace_id VARCHAR NOT NULL,
    question VARCHAR NOT NULL,
    cluster_id INTEGER DEFAULT 0,
    intent_name VARCHAR DEFAULT 'unknown',
    confidence FLOAT DEFAULT 0.0,
    embedding FLOAT[],
    source_type VARCHAR DEFAULT 'agent',
    source VARCHAR DEFAULT '',
    classified_at TIMESTAMPTZ DEFAULT current_timestamp
)
"#;

pub const CREATE_METRIC_USAGE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS oxy_obs_metric_usage (
    id VARCHAR DEFAULT uuid(),
    metric_name VARCHAR NOT NULL,
    source_type VARCHAR DEFAULT '',
    source_ref VARCHAR DEFAULT '',
    context VARCHAR DEFAULT '',
    context_types VARCHAR DEFAULT '[]',
    trace_id VARCHAR DEFAULT '',
    created_at TIMESTAMPTZ DEFAULT current_timestamp
)
"#;

/// DDL statements to run on first startup. Each entry is a single statement
/// executed as a separate `simple_query` call.
pub const ALL_DDL: &[&str] = &[
    CREATE_SPANS_TABLE,
    CREATE_INTENT_CLUSTERS_TABLE,
    CREATE_INTENT_CLASSIFICATIONS_TABLE,
    CREATE_METRIC_USAGE_TABLE,
];

/// Day-partition the time-series event tables by their timestamp column.
///
/// DuckLake rejects `PARTITIONED BY` in `CREATE TABLE` ("not supported for
/// tables in a ducklake catalog") — it must be set via `ALTER TABLE ... SET
/// PARTITIONED BY`, which is idempotent (re-running is a no-op) and applies to
/// data written afterwards. Every observability read is time-windowed
/// (`1h`..`90d`), so day partitions let DuckLake prune whole parquet files
/// instead of scanning the table; without this the traces-panel scan was the
/// query that poisoned the engine (incident 2026-07-06).
///
/// Only the three append-heavy event tables are partitioned. `oxy_obs_intent_
/// clusters` is a small mutable label table with no time-series read pattern,
/// so partitioning it would only add overhead.
pub const PARTITION_DDL: &[&str] = &[
    r#"ALTER TABLE oxy_obs_spans SET PARTITIONED BY (year("timestamp"), month("timestamp"), day("timestamp"))"#,
    r#"ALTER TABLE oxy_obs_intent_classifications SET PARTITIONED BY (year(classified_at), month(classified_at), day(classified_at))"#,
    r#"ALTER TABLE oxy_obs_metric_usage SET PARTITIONED BY (year(created_at), month(created_at), day(created_at))"#,
];
