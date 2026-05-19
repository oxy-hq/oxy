//! Intent classification and clustering queries for the Airhouse backend.

use oxy_shared::errors::OxyError;
use tokio_postgres::SimpleQueryMessage;

use super::{AirhouseObservabilityStorage, esc, get_i64, get_str, get_u64, parse_float_array, format_float_array};
use crate::intent_types::IntentCluster;
use crate::types::IntentAnalyticsRow;

fn rows(messages: &[SimpleQueryMessage]) -> impl Iterator<Item = &tokio_postgres::SimpleQueryRow> {
    messages.iter().filter_map(|m| match m {
        SimpleQueryMessage::Row(r) => Some(r),
        _ => None,
    })
}

pub async fn fetch_unprocessed_questions(
    storage: &AirhouseObservabilityStorage,
    limit: usize,
) -> Result<Vec<(String, String, String)>, OxyError> {
    let sql = format!(
        "SELECT DISTINCT
            s.trace_id,
            json_extract_string(s.span_attributes, '$.\"agent.prompt\"') AS question,
            json_extract_string(s.span_attributes, '$.\"oxy.agent.ref\"') AS source
        FROM oxy_obs_spans s
        WHERE s.span_name IN ('agent.run_agent', 'analytics.run')
          AND json_extract_string(s.span_attributes, '$.\"agent.prompt\"') != ''
          AND (s.trace_id, json_extract_string(s.span_attributes, '$.\"agent.prompt\"'))
              NOT IN (SELECT trace_id, question FROM oxy_obs_intent_classifications)
        LIMIT {limit}"
    );
    let msgs = storage.query(&sql).await?;
    let result = rows(&msgs)
        .map(|r| (get_str(r, "trace_id"), get_str(r, "question"), get_str(r, "source")))
        .collect();
    Ok(result)
}

pub async fn load_embeddings(
    storage: &AirhouseObservabilityStorage,
) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError> {
    let sql = "SELECT trace_id, question,
                   CAST(embedding AS VARCHAR) AS embedding,
                   intent_name, source
               FROM oxy_obs_intent_classifications";
    let msgs = storage.query(sql).await?;
    let result = rows(&msgs)
        .map(|r| {
            (
                get_str(r, "trace_id"),
                get_str(r, "question"),
                parse_float_array(&get_str(r, "embedding")),
                get_str(r, "intent_name"),
                get_str(r, "source"),
            )
        })
        .collect();
    Ok(result)
}

pub async fn store_clusters(
    storage: &AirhouseObservabilityStorage,
    clusters: &[IntentCluster],
) -> Result<(), OxyError> {
    storage.execute("DELETE FROM oxy_obs_intent_clusters").await?;

    for cluster in clusters {
        let centroid = format_float_array(&cluster.centroid);
        let sample_questions = serde_json::to_string(&cluster.sample_questions)
            .unwrap_or_else(|_| "[]".into());
        let sql = format!(
            "INSERT INTO oxy_obs_intent_clusters
             (cluster_id, intent_name, intent_description, centroid, sample_questions, question_count)
             VALUES ({}, '{}', '{}', {}, '{}', {})",
            cluster.cluster_id,
            esc(&cluster.intent_name),
            esc(&cluster.intent_description),
            centroid,
            esc(&sample_questions),
            cluster.sample_questions.len() as i64,
        );
        storage.execute(&sql).await?;
    }
    Ok(())
}

pub async fn load_clusters(
    storage: &AirhouseObservabilityStorage,
) -> Result<Vec<IntentCluster>, OxyError> {
    let sql = "SELECT cluster_id, intent_name, intent_description,
                   CAST(centroid AS VARCHAR) AS centroid,
                   sample_questions
               FROM oxy_obs_intent_clusters
               ORDER BY cluster_id";
    let msgs = storage.query(sql).await?;
    let result = rows(&msgs)
        .map(|r| {
            let sample_questions_str = get_str(r, "sample_questions");
            let sample_questions: Vec<String> =
                serde_json::from_str(&sample_questions_str).unwrap_or_default();
            IntentCluster {
                cluster_id: get_i64(r, "cluster_id") as u32,
                intent_name: get_str(r, "intent_name"),
                intent_description: get_str(r, "intent_description"),
                centroid: parse_float_array(&get_str(r, "centroid")),
                sample_questions,
            }
        })
        .collect();
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn store_classification(
    storage: &AirhouseObservabilityStorage,
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
    let confidence = if confidence.is_finite() {
        confidence.clamp(0.0, 1.0)
    } else {
        tracing::warn!(
            trace_id,
            "non-finite confidence ({confidence}) coerced to 0.0 for intent classification"
        );
        0.0
    };
    let emb = format_float_array(embedding);
    let sql = format!(
        "INSERT OR REPLACE INTO oxy_obs_intent_classifications
         (trace_id, question, cluster_id, intent_name, confidence, embedding, source_type, source)
         VALUES ('{}', '{}', {}, '{}', {}, {}, '{}', '{}')",
        esc(trace_id),
        esc(question),
        cluster_id as i32,
        esc(intent_name),
        confidence,
        emb,
        esc(source_type),
        esc(source),
    );
    storage.execute(&sql).await
}

pub async fn get_intent_analytics(
    storage: &AirhouseObservabilityStorage,
    days: u32,
) -> Result<Vec<IntentAnalyticsRow>, OxyError> {
    let sql = format!(
        "SELECT intent_name, count(*) AS cnt
        FROM oxy_obs_intent_classifications
        WHERE classified_at >= current_timestamp::TIMESTAMP - INTERVAL '{days} DAY'
        GROUP BY intent_name
        ORDER BY cnt DESC"
    );
    let msgs = storage.query(&sql).await?;
    let result = rows(&msgs)
        .map(|r| IntentAnalyticsRow {
            intent_name: get_str(r, "intent_name"),
            count: get_u64(r, "cnt"),
        })
        .collect();
    Ok(result)
}

pub async fn get_outliers(
    storage: &AirhouseObservabilityStorage,
    limit: usize,
) -> Result<Vec<(String, String)>, OxyError> {
    let sql = format!(
        "SELECT trace_id, question
        FROM oxy_obs_intent_classifications
        WHERE intent_name = 'unknown'
        ORDER BY classified_at DESC
        LIMIT {limit}"
    );
    let msgs = storage.query(&sql).await?;
    let result = rows(&msgs)
        .map(|r| (get_str(r, "trace_id"), get_str(r, "question")))
        .collect();
    Ok(result)
}

pub async fn load_unknown_classifications(
    storage: &AirhouseObservabilityStorage,
) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError> {
    let sql = "SELECT trace_id, question,
                   CAST(embedding AS VARCHAR) AS embedding,
                   source
               FROM oxy_obs_intent_classifications
               WHERE intent_name = 'unknown'";
    let msgs = storage.query(sql).await?;
    let result = rows(&msgs)
        .map(|r| {
            (
                get_str(r, "trace_id"),
                get_str(r, "question"),
                parse_float_array(&get_str(r, "embedding")),
                get_str(r, "source"),
            )
        })
        .collect();
    Ok(result)
}

pub async fn get_unknown_count(
    storage: &AirhouseObservabilityStorage,
) -> Result<usize, OxyError> {
    let sql = "SELECT count(*) AS n FROM oxy_obs_intent_classifications WHERE intent_name = 'unknown'";
    let msgs = storage.query(sql).await?;
    let count = rows(&msgs)
        .next()
        .and_then(|r| r.get("n").and_then(|s| s.parse::<usize>().ok()))
        .unwrap_or(0);
    Ok(count)
}

pub async fn update_cluster_record(
    storage: &AirhouseObservabilityStorage,
    cluster: &IntentCluster,
) -> Result<(), OxyError> {
    let centroid = format_float_array(&cluster.centroid);
    let sample_questions = serde_json::to_string(&cluster.sample_questions)
        .unwrap_or_else(|_| "[]".into());
    let sql = format!(
        "INSERT OR REPLACE INTO oxy_obs_intent_clusters
         (cluster_id, intent_name, intent_description, centroid,
          sample_questions, question_count, updated_at)
         VALUES ({}, '{}', '{}', {}, '{}', {}, current_timestamp)",
        cluster.cluster_id,
        esc(&cluster.intent_name),
        esc(&cluster.intent_description),
        centroid,
        esc(&sample_questions),
        cluster.sample_questions.len() as i64,
    );
    storage.execute(&sql).await
}

pub async fn get_next_cluster_id(
    storage: &AirhouseObservabilityStorage,
) -> Result<u32, OxyError> {
    let sql = "SELECT COALESCE(MAX(cluster_id), 0) AS max_id FROM oxy_obs_intent_clusters";
    let msgs = storage.query(sql).await?;
    let max_id = rows(&msgs)
        .next()
        .and_then(|r| r.get("max_id").and_then(|s| s.parse::<i32>().ok()))
        .unwrap_or(0);
    Ok((max_id + 1) as u32)
}
