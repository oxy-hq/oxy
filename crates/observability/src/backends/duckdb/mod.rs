//! DuckDB storage module for observability data
//!
//! This module provides an embedded DuckDB-based storage backend for:
//! - Trace/span storage
//! - Intent classification and clustering storage
//! - Metric usage tracking
//!
//! All writes go through a channel-based [`writer::WriterHandle`] that batches
//! span inserts and executes other writes immediately. The underlying DuckDB
//! file is stored in the project's `.oxy_state/` directory.

pub mod execution_analytics;
pub mod intents;
pub mod metrics;
pub mod schema;
pub mod traces;
pub mod trait_impl;
pub mod writer;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::{Arc, Mutex};

use duckdb::Connection;
use oxy_shared::errors::OxyError;

pub use writer::WriterHandle;

/// Embedded DuckDB storage for observability data.
///
/// Wraps a single DuckDB connection (behind `Arc<Mutex<>>`) that is shared
/// between the background writer and any read operations. WAL mode is enabled
/// so reads don't block behind the writer's transactions.
#[derive(Clone)]
pub struct DuckDBStorage {
    conn: Arc<Mutex<Connection>>,
    writer: WriterHandle,
}

impl std::fmt::Debug for DuckDBStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DuckDBStorage").finish_non_exhaustive()
    }
}

impl DuckDBStorage {
    /// Open (or create) a DuckDB database at the given file path, run schema
    /// DDL, and start the background writer task.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, OxyError> {
        let path = path.as_ref();

        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OxyError::RuntimeError(format!(
                    "Failed to create directory for DuckDB storage at {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let conn = Connection::open(path).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to open DuckDB at {}: {e}", path.display()))
        })?;

        // Enable WAL mode for better concurrent read/write behavior.
        conn.execute_batch("PRAGMA disable_progress_bar; SET wal_autocheckpoint = '512MB';")
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to configure DuckDB WAL mode: {e}"))
            })?;

        // Run schema DDL.
        for ddl in schema::ALL_DDL {
            conn.execute_batch(ddl).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to initialize DuckDB schema: {e}"))
            })?;
        }

        let conn = Arc::new(Mutex::new(conn));
        let writer = writer::start_writer(Arc::clone(&conn));

        Ok(Self { conn, writer })
    }

    /// Open an in-memory DuckDB database (useful for testing).
    pub fn open_in_memory() -> Result<Self, OxyError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to open in-memory DuckDB: {e}")))?;

        for ddl in schema::ALL_DDL {
            conn.execute_batch(ddl).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to initialize DuckDB schema: {e}"))
            })?;
        }

        let conn = Arc::new(Mutex::new(conn));
        let writer = writer::start_writer(Arc::clone(&conn));

        Ok(Self { conn, writer })
    }

    /// Get the writer handle for sending writes to the background task.
    pub fn writer(&self) -> &WriterHandle {
        &self.writer
    }

    /// Get the connection for direct read queries.
    ///
    /// Callers must acquire the mutex lock and should keep it briefly. For
    /// blocking DuckDB operations in an async context, use
    /// `tokio::task::spawn_blocking`.
    pub fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    /// Gracefully shut down the writer, flushing any buffered data.
    pub async fn shutdown(&self) {
        self.writer.shutdown().await;
    }

    /// Delete span, classification, and metric_usage rows older than
    /// `retention_days`. Intent clusters are preserved (they're aggregated
    /// labels, not event data).
    pub async fn purge_older_than(&self, retention_days: u32) -> Result<u64, OxyError> {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            // DuckDB requires a literal interval in the DELETE; retention_days
            // is a trusted config value so `format!` is safe here.
            let spans_sql = format!(
                "DELETE FROM spans WHERE timestamp < current_timestamp::TIMESTAMP - INTERVAL '{retention_days} DAY'"
            );
            let classifications_sql = format!(
                "DELETE FROM intent_classifications WHERE classified_at < current_timestamp::TIMESTAMP - INTERVAL '{retention_days} DAY'"
            );
            let metrics_sql = format!(
                "DELETE FROM metric_usage WHERE created_at < current_timestamp::TIMESTAMP - INTERVAL '{retention_days} DAY'"
            );

            let mut total: u64 = 0;
            for sql in [spans_sql, classifications_sql, metrics_sql] {
                let n = conn
                    .execute(&sql, [])
                    .map_err(|e| OxyError::RuntimeError(format!("Purge failed: {e}")))?;
                total = total.saturating_add(n as u64);
            }
            Ok(total)
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Purge task failed: {e}")))?
    }
}
