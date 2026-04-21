//! Intent classification queries against ClickHouse.

use clickhouse::Row;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};

use super::ClickHouseObservabilityStorage;
use crate::intent_types::IntentCluster;
use crate::types::IntentAnalyticsRow;

#[derive(Debug, Deserialize, Row)]
struct UnprocessedQuestionRow {
    trace_id: String,
    question: String,
    source: String,
}

#[derive(Debug, Deserialize, Row)]
struct EmbeddingRow {
    trace_id: String,
    question: String,
    embedding: Vec<f32>,
    intent_name: String,
    source: String,
}

#[derive(Debug, Deserialize, Row)]
struct ClusterRow {
    cluster_id: i32,
    intent_name: String,
    intent_description: String,
    centroid: Vec<f32>,
    sample_questions: String,
}

#[derive(Debug, Deserialize, Row)]
struct IntentAnalyticsQueryRow {
    intent_name: String,
    cnt: u64,
}

#[derive(Debug, Deserialize, Row)]
struct OutlierRow {
    trace_id: String,
    question: String,
}

#[derive(Debug, Deserialize, Row)]
struct UnknownClassificationRow {
    trace_id: String,
    question: String,
    embedding: Vec<f32>,
    source: String,
}

#[derive(Debug, Deserialize, Row)]
struct MaxIdRow {
    max_id: i32,
}

#[derive(Debug, Deserialize, Row)]
struct CountOnly {
    count: u64,
}

#[derive(Debug, Serialize, Row)]
struct ClusterInsertRow {
    cluster_id: i32,
    intent_name: String,
    intent_description: String,
    centroid: Vec<f32>,
    sample_questions: String,
    question_count: i64,
}

#[derive(Debug, Serialize, Row)]
struct ClassificationInsertRow {
    trace_id: String,
    question: String,
    cluster_id: i32,
    intent_name: String,
    confidence: f32,
    embedding: Vec<f32>,
    source_type: String,
    source: String,
}

pub(super) async fn fetch_unprocessed_questions(
    storage: &ClickHouseObservabilityStorage,
    limit: usize,
) -> Result<Vec<(String, String, String)>, OxyError> {
    let sql = format!(
        "SELECT DISTINCT
            s.trace_id AS trace_id,
            JSONExtractString(s.span_attributes, 'agent.prompt') AS question,
            JSONExtractString(s.span_attributes, 'oxy.agent.ref') AS source
        FROM observability_spans s
        WHERE s.span_name IN ('agent.run_agent', 'analytics.run')
          AND JSONExtractString(s.span_attributes, 'agent.prompt') != ''
          AND (s.trace_id, JSONExtractString(s.span_attributes, 'agent.prompt'))
              NOT IN (SELECT trace_id, question FROM observability_intent_classifications)
        LIMIT {limit}"
    );

    let rows: Vec<UnprocessedQuestionRow> = storage
        .client()
        .query(&sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Unprocessed questions query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| (r.trace_id, r.question, r.source))
        .collect())
}

pub(super) async fn load_embeddings(
    storage: &ClickHouseObservabilityStorage,
) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError> {
    let sql = "SELECT trace_id, question, embedding, intent_name, source
        FROM observability_intent_classifications FINAL";

    let rows: Vec<EmbeddingRow> = storage
        .client()
        .query(sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Load embeddings query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| (r.trace_id, r.question, r.embedding, r.intent_name, r.source))
        .collect())
}

pub(super) async fn store_clusters(
    storage: &ClickHouseObservabilityStorage,
    clusters: &[IntentCluster],
) -> Result<(), OxyError> {
    // ReplacingMergeTree relies on updated_at for versioning; we can't simply
    // DELETE rows without `ALTER TABLE ... DELETE` being asynchronous. Instead
    // truncate and re-insert; ClickHouse supports synchronous TRUNCATE.
    storage
        .client()
        .query("TRUNCATE TABLE observability_intent_clusters")
        .execute()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Failed to truncate clusters: {e}")))?;

    if clusters.is_empty() {
        return Ok(());
    }

    let mut insert = storage
        .client()
        .insert::<ClusterInsertRow>("observability_intent_clusters")
        .await
        .map_err(|e| OxyError::RuntimeError(format!("ClickHouse insert init failed: {e}")))?;

    for c in clusters {
        if c.centroid.iter().any(|v| !v.is_finite()) {
            return Err(OxyError::RuntimeError("Non-finite centroid value".into()));
        }
        let row = ClusterInsertRow {
            cluster_id: c.cluster_id as i32,
            intent_name: c.intent_name.clone(),
            intent_description: c.intent_description.clone(),
            centroid: c.centroid.clone(),
            sample_questions: serde_json::to_string(&c.sample_questions)
                .unwrap_or_else(|_| "[]".into()),
            question_count: c.sample_questions.len() as i64,
        };
        insert
            .write(&row)
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Cluster write failed: {e}")))?;
    }

    insert
        .end()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Cluster insert end failed: {e}")))?;

    Ok(())
}

pub(super) async fn load_clusters(
    storage: &ClickHouseObservabilityStorage,
) -> Result<Vec<IntentCluster>, OxyError> {
    let sql = "SELECT cluster_id, intent_name, intent_description, centroid, sample_questions
        FROM observability_intent_clusters FINAL
        ORDER BY cluster_id";

    let rows: Vec<ClusterRow> = storage
        .client()
        .query(sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Load clusters query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let sample_questions: Vec<String> =
                serde_json::from_str(&r.sample_questions).unwrap_or_default();
            IntentCluster {
                cluster_id: r.cluster_id as u32,
                intent_name: r.intent_name,
                intent_description: r.intent_description,
                centroid: r.centroid,
                sample_questions,
            }
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn store_classification(
    storage: &ClickHouseObservabilityStorage,
    trace_id: &str,
    question: &str,
    cluster_id: u32,
    intent_name: &str,
    confidence: f32,
    embedding: &[f32],
    source_type: &str,
    source: &str,
) -> Result<(), OxyError> {
    if embedding.iter().any(|v| !v.is_finite()) {
        return Err(OxyError::RuntimeError("Non-finite embedding value".into()));
    }

    let mut insert = storage
        .client()
        .insert::<ClassificationInsertRow>("observability_intent_classifications")
        .await
        .map_err(|e| OxyError::RuntimeError(format!("ClickHouse insert init failed: {e}")))?;

    let row = ClassificationInsertRow {
        trace_id: trace_id.to_string(),
        question: question.to_string(),
        cluster_id: cluster_id as i32,
        intent_name: intent_name.to_string(),
        confidence,
        embedding: embedding.to_vec(),
        source_type: source_type.to_string(),
        source: source.to_string(),
    };

    insert
        .write(&row)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Classification write failed: {e}")))?;
    insert
        .end()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Classification insert end failed: {e}")))?;

    Ok(())
}

pub(super) async fn get_intent_analytics(
    storage: &ClickHouseObservabilityStorage,
    days: u32,
) -> Result<Vec<IntentAnalyticsRow>, OxyError> {
    let sql = format!(
        "SELECT intent_name, count() AS cnt
        FROM observability_intent_classifications FINAL
        WHERE classified_at >= now() - INTERVAL {days} DAY
        GROUP BY intent_name
        ORDER BY cnt DESC"
    );

    let rows: Vec<IntentAnalyticsQueryRow> = storage
        .client()
        .query(&sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Intent analytics query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| IntentAnalyticsRow {
            intent_name: r.intent_name,
            count: r.cnt,
        })
        .collect())
}

pub(super) async fn get_outliers(
    storage: &ClickHouseObservabilityStorage,
    limit: usize,
) -> Result<Vec<(String, String)>, OxyError> {
    let sql = format!(
        "SELECT trace_id, question
        FROM observability_intent_classifications FINAL
        WHERE intent_name = 'unknown'
        ORDER BY classified_at DESC
        LIMIT {limit}"
    );

    let rows: Vec<OutlierRow> = storage
        .client()
        .query(&sql)
        .fetch_all()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Outliers query failed: {e}")))?;

    Ok(rows.into_iter().map(|r| (r.trace_id, r.question)).collect())
}

pub(super) async fn load_unknown_classifications(
    storage: &ClickHouseObservabilityStorage,
) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError> {
    let sql = "SELECT trace_id, question, embedding, source
        FROM observability_intent_classifications FINAL
        WHERE intent_name = 'unknown'";

    let rows: Vec<UnknownClassificationRow> =
        storage.client().query(sql).fetch_all().await.map_err(|e| {
            OxyError::RuntimeError(format!("Unknown classifications query failed: {e}"))
        })?;

    Ok(rows
        .into_iter()
        .map(|r| (r.trace_id, r.question, r.embedding, r.source))
        .collect())
}

pub(super) async fn get_unknown_count(
    storage: &ClickHouseObservabilityStorage,
) -> Result<usize, OxyError> {
    let sql = "SELECT count() AS count FROM observability_intent_classifications FINAL WHERE intent_name = 'unknown'";

    let result: CountOnly = storage
        .client()
        .query(sql)
        .fetch_one()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Unknown count query failed: {e}")))?;

    Ok(result.count as usize)
}

pub(super) async fn update_cluster_record(
    storage: &ClickHouseObservabilityStorage,
    cluster: &IntentCluster,
) -> Result<(), OxyError> {
    if cluster.centroid.iter().any(|v| !v.is_finite()) {
        return Err(OxyError::RuntimeError("Non-finite centroid value".into()));
    }

    let mut insert = storage
        .client()
        .insert::<ClusterInsertRow>("observability_intent_clusters")
        .await
        .map_err(|e| OxyError::RuntimeError(format!("ClickHouse insert init failed: {e}")))?;

    let row = ClusterInsertRow {
        cluster_id: cluster.cluster_id as i32,
        intent_name: cluster.intent_name.clone(),
        intent_description: cluster.intent_description.clone(),
        centroid: cluster.centroid.clone(),
        sample_questions: serde_json::to_string(&cluster.sample_questions)
            .unwrap_or_else(|_| "[]".into()),
        question_count: cluster.sample_questions.len() as i64,
    };

    insert
        .write(&row)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Cluster write failed: {e}")))?;
    insert
        .end()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Cluster insert end failed: {e}")))?;

    Ok(())
}

pub(super) async fn get_next_cluster_id(
    storage: &ClickHouseObservabilityStorage,
) -> Result<u32, OxyError> {
    let sql = "SELECT toInt32(coalesce(max(cluster_id), 0)) AS max_id FROM observability_intent_clusters FINAL";

    let result: MaxIdRow = storage
        .client()
        .query(sql)
        .fetch_one()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Next cluster ID query failed: {e}")))?;

    Ok((result.max_id + 1) as u32)
}
