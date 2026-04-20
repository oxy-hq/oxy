//! ClickHouse DDL for observability tables.

pub const CREATE_SPANS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS observability_spans (
    trace_id String,
    span_id String,
    parent_span_id String DEFAULT '',
    span_name LowCardinality(String),
    service_name LowCardinality(String) DEFAULT 'oxy',
    span_attributes String DEFAULT '{}',
    duration_ns Int64 DEFAULT 0,
    status_code LowCardinality(String) DEFAULT 'UNSET',
    status_message String DEFAULT '',
    event_data String DEFAULT '[]',
    timestamp DateTime64(9) DEFAULT now64(9)
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (trace_id, span_id, timestamp)
"#;

pub const CREATE_INTENT_CLUSTERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS observability_intent_clusters (
    cluster_id Int32,
    intent_name String,
    intent_description String DEFAULT '',
    centroid Array(Float32),
    sample_questions String DEFAULT '[]',
    question_count Int64 DEFAULT 0,
    created_at DateTime64(3) DEFAULT now64(3),
    updated_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(updated_at)
ORDER BY cluster_id
"#;

pub const CREATE_INTENT_CLASSIFICATIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS observability_intent_classifications (
    trace_id String,
    question String,
    cluster_id Int32 DEFAULT 0,
    intent_name LowCardinality(String) DEFAULT 'unknown',
    confidence Float32 DEFAULT 0.0,
    embedding Array(Float32),
    source_type LowCardinality(String) DEFAULT 'agent',
    source LowCardinality(String) DEFAULT '',
    classified_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(classified_at)
ORDER BY (trace_id, question)
"#;

pub const CREATE_METRIC_USAGE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS observability_metric_usage (
    id UUID DEFAULT generateUUIDv4(),
    metric_name LowCardinality(String),
    source_type LowCardinality(String) DEFAULT '',
    source_ref String DEFAULT '',
    context String DEFAULT '',
    context_types String DEFAULT '[]',
    trace_id String DEFAULT '',
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY (metric_name, source_type, created_at)
"#;

pub const ALL_DDL: &[&str] = &[
    CREATE_SPANS_TABLE,
    CREATE_INTENT_CLUSTERS_TABLE,
    CREATE_INTENT_CLASSIFICATIONS_TABLE,
    CREATE_METRIC_USAGE_TABLE,
];
