//! Channel-based batched writer for DuckDB observability storage.
//!
//! DuckDB has a single-writer limitation, so all writes are funnelled through
//! a single background task that owns the connection mutex. High-volume span
//! data is buffered and flushed periodically (every 1 second or when the
//! buffer reaches 100 items). These values match the telemetry bridge so the
//! two buffers compose rather than stack latency. Low-volume writes (metrics,
//! classifications, clusters) are executed immediately upon receipt.

use std::sync::{Arc, Mutex};

use duckdb::Connection;
use tokio::sync::mpsc;

use crate::types::{ClassificationRecord, ClusterRecord, MetricUsageRecord, SpanRecord};

// ── Write operations ────────────────────────────────────────────────────────

/// Operations that can be sent to the writer background task.
pub enum WriteOp {
    /// Buffer spans for batched insert.
    Spans(Vec<SpanRecord>),
    /// Insert metric usage records immediately.
    Metrics(Vec<MetricUsageRecord>),
    /// Insert a classification record immediately.
    Classification(ClassificationRecord),
    /// Upsert a cluster record immediately.
    StoreCluster(ClusterRecord),
    /// Force-flush the span buffer.
    Flush,
    /// Shut down the writer, flushing any remaining spans.
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

// ── Writer handle ───────────────────────────────────────────────────────────

/// Handle for sending write operations to the background writer task.
///
/// Clone-able and cheap to pass around. Dropping all handles will cause the
/// background task to exit after processing remaining messages.
#[derive(Clone)]
pub struct WriterHandle {
    tx: mpsc::UnboundedSender<WriteOp>,
}

impl WriterHandle {
    /// Queue span records for batched insertion.
    pub fn send_spans(&self, spans: Vec<SpanRecord>) {
        let _ = self.tx.send(WriteOp::Spans(spans));
    }

    /// Insert metric usage records (executed immediately by the writer).
    pub fn send_metrics(&self, metrics: Vec<MetricUsageRecord>) {
        let _ = self.tx.send(WriteOp::Metrics(metrics));
    }

    /// Insert a classification record (executed immediately by the writer).
    pub fn send_classification(&self, record: ClassificationRecord) {
        let _ = self.tx.send(WriteOp::Classification(record));
    }

    /// Upsert a cluster record (executed immediately by the writer).
    pub fn send_cluster(&self, record: ClusterRecord) {
        let _ = self.tx.send(WriteOp::StoreCluster(record));
    }

    /// Force-flush any buffered spans.
    pub fn flush(&self) {
        let _ = self.tx.send(WriteOp::Flush);
    }

    /// Gracefully shut down the writer, flushing remaining data.
    ///
    /// Returns once the writer has finished flushing and exited.
    pub async fn shutdown(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = self.tx.send(WriteOp::Shutdown(tx));
        let _ = rx.await;
    }
}

// ── Constants ───────────────────────────────────────────────────────────────

/// Number of spans to buffer before auto-flushing. Matches the upstream
/// telemetry bridge to keep end-to-end latency at ~1s rather than 6s.
const SPAN_BUFFER_CAPACITY: usize = 100;

/// Interval between timer-based flushes. Matches the upstream telemetry bridge.
const FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

// ── Writer startup ──────────────────────────────────────────────────────────

/// Start the background writer task and return a handle for sending writes.
///
/// The writer owns the `Connection` (wrapped in `Arc<Mutex<>>`) and processes
/// all write operations sequentially. DuckDB blocking calls are dispatched
/// via `tokio::task::spawn_blocking` so they don't block the async runtime.
pub fn start_writer(conn: Arc<Mutex<Connection>>) -> WriterHandle {
    let (tx, rx) = mpsc::unbounded_channel::<WriteOp>();
    tokio::spawn(writer_loop(conn, rx));
    WriterHandle { tx }
}

// ── Background loop ─────────────────────────────────────────────────────────

async fn writer_loop(conn: Arc<Mutex<Connection>>, mut rx: mpsc::UnboundedReceiver<WriteOp>) {
    let mut span_buffer: Vec<SpanRecord> = Vec::with_capacity(SPAN_BUFFER_CAPACITY);
    let mut interval = tokio::time::interval(FLUSH_INTERVAL);
    // The first tick completes immediately; consume it so we don't flush an
    // empty buffer right after startup.
    interval.tick().await;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(WriteOp::Spans(spans)) => {
                        span_buffer.extend(spans);
                        if span_buffer.len() >= SPAN_BUFFER_CAPACITY {
                            flush_spans(&conn, &mut span_buffer).await;
                        }
                    }
                    Some(WriteOp::Metrics(metrics)) => {
                        insert_metrics(&conn, metrics).await;
                    }
                    Some(WriteOp::Classification(record)) => {
                        insert_classification(&conn, record).await;
                    }
                    Some(WriteOp::StoreCluster(record)) => {
                        upsert_cluster(&conn, record).await;
                    }
                    Some(WriteOp::Flush) => {
                        flush_spans(&conn, &mut span_buffer).await;
                    }
                    Some(WriteOp::Shutdown(done)) => {
                        flush_spans(&conn, &mut span_buffer).await;
                        let _ = done.send(());
                        break;
                    }
                    None => {
                        // All senders dropped — flush and exit.
                        flush_spans(&conn, &mut span_buffer).await;
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                if !span_buffer.is_empty() {
                    flush_spans(&conn, &mut span_buffer).await;
                }
            }
        }
    }
}

// ── Flush / insert helpers ──────────────────────────────────────────────────

/// Drain the span buffer and insert all spans in a single transaction.
async fn flush_spans(conn: &Arc<Mutex<Connection>>, buffer: &mut Vec<SpanRecord>) {
    if buffer.is_empty() {
        return;
    }
    let spans = std::mem::take(buffer);
    let count = spans.len();
    let conn = Arc::clone(conn);

    let result = tokio::task::spawn_blocking(move || {
        let mut conn = conn.lock().expect("DuckDB connection lock poisoned");
        flush_spans_blocking(&mut conn, &spans)
    })
    .await;

    match result {
        Ok(Ok(())) => {
            tracing::debug!("Flushed {} spans to DuckDB", count);
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to flush {} spans to DuckDB: {}", count, e);
        }
        Err(e) => {
            tracing::error!("Span flush task panicked: {}", e);
        }
    }
}

pub(super) fn flush_spans_blocking(
    conn: &mut Connection,
    spans: &[SpanRecord],
) -> Result<(), duckdb::Error> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO spans \
             (trace_id, span_id, parent_span_id, span_name, service_name, \
              span_attributes, duration_ns, status_code, status_message, \
              event_data, timestamp) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;

        for span in spans {
            stmt.execute(duckdb::params![
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
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Insert metric usage records in a single transaction.
async fn insert_metrics(conn: &Arc<Mutex<Connection>>, metrics: Vec<MetricUsageRecord>) {
    if metrics.is_empty() {
        return;
    }
    let count = metrics.len();
    let conn = Arc::clone(conn);

    let result = tokio::task::spawn_blocking(move || {
        let mut conn = conn.lock().expect("DuckDB connection lock poisoned");
        insert_metrics_blocking(&mut conn, &metrics)
    })
    .await;

    match result {
        Ok(Ok(())) => {
            tracing::debug!("Inserted {} metric usage records", count);
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to insert metric usage records: {}", e);
        }
        Err(e) => {
            tracing::error!("Metric insert task panicked: {}", e);
        }
    }
}

pub(super) fn insert_metrics_blocking(
    conn: &mut Connection,
    metrics: &[MetricUsageRecord],
) -> Result<(), duckdb::Error> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO metric_usage \
             (metric_name, source_type, source_ref, context, context_types, trace_id) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )?;

        for m in metrics {
            stmt.execute(duckdb::params![
                m.metric_name,
                m.source_type,
                m.source_ref,
                m.context,
                m.context_types,
                m.trace_id,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Insert a single classification record.
async fn insert_classification(conn: &Arc<Mutex<Connection>>, record: ClassificationRecord) {
    let conn = Arc::clone(conn);

    let result = tokio::task::spawn_blocking(move || {
        let conn = conn.lock().expect("DuckDB connection lock poisoned");
        insert_classification_blocking(&conn, &record)
    })
    .await;

    match result {
        Ok(Ok(())) => {
            tracing::debug!("Inserted classification");
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to insert classification: {}", e);
        }
        Err(e) => {
            tracing::error!("Classification insert task panicked: {}", e);
        }
    }
}

fn insert_classification_blocking(
    conn: &Connection,
    r: &ClassificationRecord,
) -> Result<(), duckdb::Error> {
    // Validate embedding values before constructing SQL fragment.
    if r.embedding.iter().any(|v| !v.is_finite()) {
        return Err(duckdb::Error::InvalidParameterName(
            "Non-finite embedding value".into(),
        ));
    }

    let embedding_sql = format!(
        "[{}]",
        r.embedding
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );

    conn.execute(
        &format!(
            "INSERT INTO intent_classifications \
             (trace_id, question, cluster_id, intent_name, confidence, \
              embedding, source_type, source) \
             VALUES (?, ?, ?, ?, ?, {embedding_sql}::FLOAT[], ?, ?)"
        ),
        duckdb::params![
            r.trace_id,
            r.question,
            r.cluster_id,
            r.intent_name,
            r.confidence,
            r.source_type,
            r.source,
        ],
    )?;
    Ok(())
}

/// Upsert (INSERT OR REPLACE) a cluster record.
async fn upsert_cluster(conn: &Arc<Mutex<Connection>>, record: ClusterRecord) {
    let conn = Arc::clone(conn);

    let result = tokio::task::spawn_blocking(move || {
        let conn = conn.lock().expect("DuckDB connection lock poisoned");
        upsert_cluster_blocking(&conn, &record)
    })
    .await;

    match result {
        Ok(Ok(())) => {
            tracing::debug!("Upserted cluster");
        }
        Ok(Err(e)) => {
            tracing::error!("Failed to upsert cluster: {}", e);
        }
        Err(e) => {
            tracing::error!("Cluster upsert task panicked: {}", e);
        }
    }
}

fn upsert_cluster_blocking(conn: &Connection, r: &ClusterRecord) -> Result<(), duckdb::Error> {
    if r.centroid.iter().any(|v| !v.is_finite()) {
        return Err(duckdb::Error::InvalidParameterName(
            "Non-finite centroid value".into(),
        ));
    }

    let centroid_sql = format!(
        "[{}]",
        r.centroid
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );

    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO intent_clusters \
             (cluster_id, intent_name, intent_description, centroid, \
              sample_questions, question_count, updated_at) \
             VALUES (?, ?, ?, {centroid_sql}::FLOAT[], ?, ?, current_timestamp)"
        ),
        duckdb::params![
            r.cluster_id,
            r.intent_name,
            r.intent_description,
            r.sample_questions,
            r.question_count,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_test_db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().expect("Failed to open in-memory DuckDB");
        // Initialize schema
        for ddl in crate::backends::duckdb::schema::ALL_DDL {
            conn.execute_batch(ddl).expect("Failed to execute DDL");
        }
        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn test_flush_spans_blocking() {
        let conn = open_test_db();
        let mut guard = conn.lock().unwrap();

        let spans = vec![
            SpanRecord {
                trace_id: "trace-1".into(),
                span_id: "span-1".into(),
                parent_span_id: "".into(),
                span_name: "test.op".into(),
                service_name: "oxy".into(),
                span_attributes: r#"{"key":"value"}"#.into(),
                duration_ns: 1_000_000,
                status_code: "OK".into(),
                status_message: "".into(),
                event_data: r#"[{"name":"event1","attributes":{"k":"v"}}]"#.into(),
                timestamp: "2026-01-01T00:00:00Z".into(),
            },
            SpanRecord {
                trace_id: "trace-1".into(),
                span_id: "span-2".into(),
                parent_span_id: "span-1".into(),
                span_name: "test.child".into(),
                service_name: "oxy".into(),
                span_attributes: "{}".into(),
                duration_ns: 500_000,
                status_code: "OK".into(),
                status_message: "".into(),
                event_data: "[]".into(),
                timestamp: "2026-01-01T00:00:01Z".into(),
            },
        ];

        flush_spans_blocking(&mut guard, &spans).expect("flush should succeed");

        let count: i64 = guard
            .query_row("SELECT count(*) FROM spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_insert_metrics_blocking() {
        let conn = open_test_db();
        let mut guard = conn.lock().unwrap();

        let metrics = vec![MetricUsageRecord {
            metric_name: "revenue".into(),
            source_type: "agent".into(),
            source_ref: "sales_agent".into(),
            context: "quarterly report".into(),
            context_types: r#"["financial"]"#.into(),
            trace_id: "trace-100".into(),
        }];

        insert_metrics_blocking(&mut guard, &metrics).expect("insert should succeed");

        let count: i64 = guard
            .query_row("SELECT count(*) FROM metric_usage", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_span_upsert_replaces_on_conflict() {
        let conn = open_test_db();
        let mut guard = conn.lock().unwrap();

        let make_span = |status: &str| SpanRecord {
            trace_id: "t1".into(),
            span_id: "s1".into(),
            parent_span_id: "".into(),
            span_name: "op".into(),
            service_name: "oxy".into(),
            span_attributes: "{}".into(),
            duration_ns: 100,
            status_code: status.into(),
            status_message: "".into(),
            event_data: "[]".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
        };

        flush_spans_blocking(&mut guard, &[make_span("UNSET")]).unwrap();
        flush_spans_blocking(&mut guard, &[make_span("OK")]).unwrap();

        let (count, status): (i64, String) = guard
            .query_row("SELECT count(*), max(status_code) FROM spans", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(status, "OK");
    }
}
