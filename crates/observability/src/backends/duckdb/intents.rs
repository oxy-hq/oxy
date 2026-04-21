//! Intent classification and clustering query implementations for DuckDB storage.
//!
//! Provides methods for managing intent classifications, clusters, and related
//! analytics queries against the local DuckDB database.

use std::sync::Arc;

use oxy_shared::errors::OxyError;

use crate::intent_types::IntentCluster;
use crate::types::IntentAnalyticsRow;

use super::DuckDBStorage;

// ── Queries ────────────────────────────────────────────────────────────────

impl DuckDBStorage {
    /// Fetch unprocessed questions from spans that don't have classifications yet.
    ///
    /// Returns tuples of (trace_id, question, source).
    pub async fn fetch_unprocessed_questions(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, String, String)>, OxyError> {
        let conn = Arc::clone(self.conn());
        let limit_val = limit as i64;

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT
                        s.trace_id,
                        json_extract_string(s.span_attributes, '$.\"agent.prompt\"') as question,
                        json_extract_string(s.span_attributes, '$.\"oxy.agent.ref\"') as source
                    FROM spans s
                    WHERE s.span_name IN ('agent.run_agent', 'analytics.run')
                      AND json_extract_string(s.span_attributes, '$.\"agent.prompt\"') != ''
                      AND (s.trace_id, json_extract_string(s.span_attributes, '$.\"agent.prompt\"'))
                          NOT IN (SELECT trace_id, question FROM intent_classifications)
                    LIMIT ?",
                )
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([&limit_val], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Load all embeddings from intent_classifications.
    ///
    /// Returns tuples of (trace_id, question, embedding, intent_name, source).
    pub async fn load_embeddings(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT trace_id, question, CAST(embedding AS VARCHAR) AS embedding, intent_name, source
                    FROM intent_classifications",
                )
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([], |row| {
                    let embedding_str: String = row.get(2)?;
                    let embedding = super::traces::parse_float_array(&embedding_str);

                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        embedding,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Store clusters (delete all existing, then insert new ones).
    pub async fn store_clusters(&self, clusters: &[IntentCluster]) -> Result<(), OxyError> {
        let conn = Arc::clone(self.conn());
        let clusters: Vec<IntentCluster> = clusters.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let tx = conn
                .transaction()
                .map_err(|e| OxyError::RuntimeError(format!("Transaction start failed: {e}")))?;

            tx.execute_batch("DELETE FROM intent_clusters")
                .map_err(|e| OxyError::RuntimeError(format!("Failed to clear clusters: {e}")))?;

            for cluster in &clusters {
                let centroid_sql = format!(
                    "[{}]",
                    cluster
                        .centroid
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let sample_questions_json = serde_json::to_string(&cluster.sample_questions)
                    .unwrap_or_else(|_| "[]".into());

                tx.execute(
                    &format!(
                        "INSERT INTO intent_clusters
                         (cluster_id, intent_name, intent_description, centroid,
                          sample_questions, question_count)
                         VALUES (?, ?, ?, {centroid_sql}::FLOAT[], ?, ?)"
                    ),
                    duckdb::params![
                        cluster.cluster_id,
                        cluster.intent_name,
                        cluster.intent_description,
                        sample_questions_json,
                        cluster.sample_questions.len() as i64,
                    ],
                )
                .map_err(|e| OxyError::RuntimeError(format!("Insert cluster failed: {e}")))?;
            }

            tx.commit()
                .map_err(|e| OxyError::RuntimeError(format!("Transaction commit failed: {e}")))?;

            Ok(())
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Load all clusters.
    pub async fn load_clusters(&self) -> Result<Vec<IntentCluster>, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT cluster_id, intent_name, intent_description, CAST(centroid AS VARCHAR) AS centroid, sample_questions
                    FROM intent_clusters
                    ORDER BY cluster_id",
                )
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([], |row| {
                    let centroid_str: String = row.get(3)?;
                    let centroid = super::traces::parse_float_array(&centroid_str);

                    let sample_questions_str: String = row.get(4)?;
                    let sample_questions: Vec<String> =
                        serde_json::from_str(&sample_questions_str).unwrap_or_default();

                    Ok(IntentCluster {
                        cluster_id: row.get::<_, i32>(0)? as u32,
                        intent_name: row.get(1)?,
                        intent_description: row.get(2)?,
                        centroid,
                        sample_questions,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Store a classification result.
    pub async fn store_classification(
        &self,
        trace_id: &str,
        question: &str,
        cluster_id: u32,
        intent_name: &str,
        confidence: f32,
        embedding: &[f32],
        source_type: &str,
        source: &str,
    ) -> Result<(), OxyError> {
        let conn = Arc::clone(self.conn());
        let trace_id = trace_id.to_string();
        let question = question.to_string();
        let intent_name = intent_name.to_string();
        let source_type = source_type.to_string();
        let source = source.to_string();
        let embedding = embedding.to_vec();

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            if embedding.iter().any(|v| !v.is_finite()) {
                return Err(OxyError::RuntimeError("Non-finite embedding value".into()));
            }

            let embedding_sql = format!(
                "[{}]",
                embedding
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );

            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO intent_classifications
                     (trace_id, question, cluster_id, intent_name, confidence,
                      embedding, source_type, source)
                     VALUES (?, ?, ?, ?, ?, {embedding_sql}::FLOAT[], ?, ?)"
                ),
                duckdb::params![
                    trace_id,
                    question,
                    cluster_id as i32,
                    intent_name,
                    confidence,
                    source_type,
                    source,
                ],
            )
            .map_err(|e| OxyError::RuntimeError(format!("Insert classification failed: {e}")))?;

            Ok(())
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Update a classification (upsert via INSERT OR REPLACE).
    pub async fn update_classification(
        &self,
        trace_id: &str,
        question: &str,
        cluster_id: u32,
        intent_name: &str,
        confidence: f32,
        embedding: &[f32],
        source_type: &str,
        source: &str,
    ) -> Result<(), OxyError> {
        // With PRIMARY KEY (trace_id, question), store_classification already does an upsert.
        self.store_classification(
            trace_id,
            question,
            cluster_id,
            intent_name,
            confidence,
            embedding,
            source_type,
            source,
        )
        .await
    }

    /// Get intent analytics for the last N days.
    pub async fn get_intent_analytics(
        &self,
        days: u32,
    ) -> Result<Vec<IntentAnalyticsRow>, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let sql = format!(
                "SELECT intent_name, count(*) as cnt
                FROM intent_classifications
                WHERE classified_at >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
                GROUP BY intent_name
                ORDER BY cnt DESC"
            );

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(IntentAnalyticsRow {
                        intent_name: row.get(0)?,
                        count: row.get::<_, i64>(1)? as u64,
                    })
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get outlier questions (classified as "unknown").
    pub async fn get_outliers(&self, limit: usize) -> Result<Vec<(String, String)>, OxyError> {
        let conn = Arc::clone(self.conn());
        let limit_val = limit as i64;

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT trace_id, question
                    FROM intent_classifications
                    WHERE intent_name = 'unknown'
                    ORDER BY classified_at DESC
                    LIMIT ?",
                )
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([&limit_val], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Load unknown classifications for incremental clustering.
    ///
    /// Returns tuples of (trace_id, question, embedding, source).
    pub async fn load_unknown_classifications(
        &self,
    ) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let mut stmt = conn
                .prepare(
                    "SELECT trace_id, question, CAST(embedding AS VARCHAR) AS embedding, source
                    FROM intent_classifications
                    WHERE intent_name = 'unknown'",
                )
                .map_err(|e| OxyError::RuntimeError(format!("Prepare failed: {e}")))?;

            let rows = stmt
                .query_map([], |row| {
                    let embedding_str: String = row.get(2)?;
                    let embedding = super::traces::parse_float_array(&embedding_str);

                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        embedding,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| OxyError::RuntimeError(format!("Row read failed: {e}")))
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get count of unknown classifications.
    pub async fn get_unknown_count(&self) -> Result<usize, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM intent_classifications WHERE intent_name = 'unknown'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            Ok(count as usize)
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Update a single cluster (upsert via INSERT OR REPLACE).
    pub async fn update_cluster_record(&self, cluster: &IntentCluster) -> Result<(), OxyError> {
        let conn = Arc::clone(self.conn());
        let cluster = cluster.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let centroid_sql = format!(
                "[{}]",
                cluster
                    .centroid
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let sample_questions_json =
                serde_json::to_string(&cluster.sample_questions).unwrap_or_else(|_| "[]".into());

            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO intent_clusters
                     (cluster_id, intent_name, intent_description, centroid,
                      sample_questions, question_count, updated_at)
                     VALUES (?, ?, ?, {centroid_sql}::FLOAT[], ?, ?, current_timestamp)"
                ),
                duckdb::params![
                    cluster.cluster_id,
                    cluster.intent_name,
                    cluster.intent_description,
                    sample_questions_json,
                    cluster.sample_questions.len() as i64,
                ],
            )
            .map_err(|e| OxyError::RuntimeError(format!("Upsert cluster failed: {e}")))?;

            Ok(())
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }

    /// Get the next available cluster ID.
    pub async fn get_next_cluster_id(&self) -> Result<u32, OxyError> {
        let conn = Arc::clone(self.conn());

        tokio::task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|e| OxyError::RuntimeError(format!("Lock poisoned: {e}")))?;

            let max_id: i32 = conn
                .query_row(
                    "SELECT COALESCE(MAX(cluster_id), 0) FROM intent_clusters",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| OxyError::RuntimeError(format!("Query failed: {e}")))?;

            Ok((max_id + 1) as u32)
        })
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Task failed: {e}")))?
    }
}
