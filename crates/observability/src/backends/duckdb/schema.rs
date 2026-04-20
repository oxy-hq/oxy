//! DuckDB schema definitions for observability storage
//!
//! Table definitions for trace storage, intent classification, and
//! metric usage tracking. The schema uses
//! DuckDB-native types and stores complex/nested data as JSON strings
//! in VARCHAR columns for simplicity.

/// SQL to create the spans table
pub const CREATE_SPANS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS spans (
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
    timestamp TIMESTAMPTZ DEFAULT current_timestamp,
    PRIMARY KEY (trace_id, span_id)
);
"#;

/// SQL to create indexes on the spans table
pub const CREATE_SPANS_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_spans_timestamp ON spans(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_spans_name ON spans(span_name);
"#;

/// SQL to create the intent_clusters table
pub const CREATE_INTENT_CLUSTERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS intent_clusters (
    cluster_id INTEGER PRIMARY KEY,
    intent_name VARCHAR NOT NULL,
    intent_description VARCHAR DEFAULT '',
    centroid FLOAT[],
    sample_questions VARCHAR DEFAULT '[]',
    question_count BIGINT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT current_timestamp,
    updated_at TIMESTAMPTZ DEFAULT current_timestamp
);
"#;

/// SQL to create the intent_classifications table
pub const CREATE_INTENT_CLASSIFICATIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS intent_classifications (
    trace_id VARCHAR NOT NULL,
    question VARCHAR NOT NULL,
    cluster_id INTEGER DEFAULT 0,
    intent_name VARCHAR DEFAULT 'unknown',
    confidence FLOAT DEFAULT 0.0,
    embedding FLOAT[],
    source_type VARCHAR DEFAULT 'agent',
    source VARCHAR DEFAULT '',
    classified_at TIMESTAMPTZ DEFAULT current_timestamp,
    PRIMARY KEY (trace_id, question)
);
"#;

/// SQL to create indexes on the intent_classifications table
pub const CREATE_INTENT_CLASSIFICATIONS_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_ic_trace ON intent_classifications(trace_id);
CREATE INDEX IF NOT EXISTS idx_ic_classified_at ON intent_classifications(classified_at DESC);
"#;

/// SQL to create the metric_usage table
pub const CREATE_METRIC_USAGE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS metric_usage (
    id VARCHAR DEFAULT uuid(),
    metric_name VARCHAR NOT NULL,
    source_type VARCHAR DEFAULT '',
    source_ref VARCHAR DEFAULT '',
    context VARCHAR DEFAULT '',
    context_types VARCHAR DEFAULT '[]',
    trace_id VARCHAR DEFAULT '',
    created_at TIMESTAMPTZ DEFAULT current_timestamp
);
"#;

/// SQL to create indexes on the metric_usage table
pub const CREATE_METRIC_USAGE_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_mu_metric ON metric_usage(metric_name, created_at);
CREATE INDEX IF NOT EXISTS idx_mu_trace ON metric_usage(trace_id);
"#;

/// All DDL statements in order, for initializing the database schema.
pub const ALL_DDL: &[&str] = &[
    CREATE_SPANS_TABLE,
    CREATE_SPANS_INDEXES,
    CREATE_INTENT_CLUSTERS_TABLE,
    CREATE_INTENT_CLASSIFICATIONS_TABLE,
    CREATE_INTENT_CLASSIFICATIONS_INDEXES,
    CREATE_METRIC_USAGE_TABLE,
    CREATE_METRIC_USAGE_INDEXES,
];
