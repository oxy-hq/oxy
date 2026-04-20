//! Intent classification queries against Postgres observability tables.

use oxy_shared::errors::OxyError;
use sea_orm::{ConnectionTrait, FromQueryResult, Statement};

use super::{PostgresObservabilityStorage, format_pg_float_array, parse_pg_float_array, pg};
use crate::intent_types::IntentCluster;
use crate::types::IntentAnalyticsRow;

#[derive(Debug, FromQueryResult)]
struct UnprocessedQuestionRow {
    trace_id: String,
    question: String,
    source: String,
}

#[derive(Debug, FromQueryResult)]
struct EmbeddingRow {
    trace_id: String,
    question: String,
    embedding: String,
    intent_name: String,
    source: String,
}

#[derive(Debug, FromQueryResult)]
struct ClusterRow {
    cluster_id: i32,
    intent_name: String,
    intent_description: String,
    centroid: String,
    sample_questions: String,
}

#[derive(Debug, FromQueryResult)]
struct IntentAnalyticsQueryRow {
    intent_name: String,
    cnt: i64,
}

#[derive(Debug, FromQueryResult)]
struct OutlierRow {
    trace_id: String,
    question: String,
}

#[derive(Debug, FromQueryResult)]
struct UnknownClassificationRow {
    trace_id: String,
    question: String,
    embedding: String,
    source: String,
}

#[derive(Debug, FromQueryResult)]
struct MaxIdRow {
    max_id: i32,
}

#[derive(Debug, FromQueryResult)]
struct CountRow {
    count: i64,
}

pub(super) async fn fetch_unprocessed_questions(
    storage: &PostgresObservabilityStorage,
    limit: usize,
) -> Result<Vec<(String, String, String)>, OxyError> {
    let sql = "SELECT DISTINCT
        s.trace_id,
        s.span_attributes->>'agent.prompt' AS question,
        s.span_attributes->>'oxy.agent.ref' AS source
    FROM observability_spans s
    WHERE s.span_name = 'agent.run_agent'
      AND s.span_attributes->>'agent.prompt' != ''
      AND (s.trace_id, s.span_attributes->>'agent.prompt')
          NOT IN (SELECT trace_id, question FROM observability_intent_classifications)
    LIMIT $1";

    let rows = UnprocessedQuestionRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        sql,
        vec![(limit as i64).into()],
    ))
    .all(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Unprocessed questions query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| (r.trace_id, r.question, r.source))
        .collect())
}

pub(super) async fn load_embeddings(
    storage: &PostgresObservabilityStorage,
) -> Result<Vec<(String, String, Vec<f32>, String, String)>, OxyError> {
    let sql = "SELECT trace_id, question, embedding::TEXT AS embedding, intent_name, source
        FROM observability_intent_classifications";

    let rows = EmbeddingRow::find_by_statement(Statement::from_sql_and_values(pg(), sql, vec![]))
        .all(storage.db())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Load embeddings query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let embedding = parse_pg_float_array(&r.embedding);
            (r.trace_id, r.question, embedding, r.intent_name, r.source)
        })
        .collect())
}

pub(super) async fn store_clusters(
    storage: &PostgresObservabilityStorage,
    clusters: &[IntentCluster],
) -> Result<(), OxyError> {
    let db = storage.db();

    db.execute(Statement::from_string(
        pg(),
        "DELETE FROM observability_intent_clusters".to_string(),
    ))
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Failed to clear clusters: {e}")))?;

    for cluster in clusters {
        let centroid = format_pg_float_array(&cluster.centroid)?;
        let sample_questions_json =
            serde_json::to_string(&cluster.sample_questions).unwrap_or_else(|_| "[]".into());

        let sql = format!(
            "INSERT INTO observability_intent_clusters
             (cluster_id, intent_name, intent_description, centroid,
              sample_questions, question_count)
             VALUES ($1, $2, $3, {centroid}, $4::JSONB, $5)"
        );

        db.execute(Statement::from_sql_and_values(
            pg(),
            &sql,
            vec![
                (cluster.cluster_id as i32).into(),
                cluster.intent_name.clone().into(),
                cluster.intent_description.clone().into(),
                sample_questions_json.into(),
                (cluster.sample_questions.len() as i64).into(),
            ],
        ))
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Insert cluster failed: {e}")))?;
    }

    Ok(())
}

pub(super) async fn load_clusters(
    storage: &PostgresObservabilityStorage,
) -> Result<Vec<IntentCluster>, OxyError> {
    let sql = "SELECT cluster_id, intent_name, intent_description,
        centroid::TEXT AS centroid, sample_questions::TEXT AS sample_questions
        FROM observability_intent_clusters
        ORDER BY cluster_id";

    let rows = ClusterRow::find_by_statement(Statement::from_sql_and_values(pg(), sql, vec![]))
        .all(storage.db())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Load clusters query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let centroid = parse_pg_float_array(&r.centroid);
            let sample_questions: Vec<String> =
                serde_json::from_str(&r.sample_questions).unwrap_or_default();

            IntentCluster {
                cluster_id: r.cluster_id as u32,
                intent_name: r.intent_name,
                intent_description: r.intent_description,
                centroid,
                sample_questions,
            }
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn store_classification(
    storage: &PostgresObservabilityStorage,
    trace_id: &str,
    question: &str,
    cluster_id: u32,
    intent_name: &str,
    confidence: f32,
    embedding: &[f32],
    source_type: &str,
    source: &str,
) -> Result<(), OxyError> {
    let embedding_literal = format_pg_float_array(embedding)?;

    let sql = format!(
        "INSERT INTO observability_intent_classifications
         (trace_id, question, cluster_id, intent_name, confidence,
          embedding, source_type, source)
         VALUES ($1, $2, $3, $4, $5, {embedding_literal}, $6, $7)
         ON CONFLICT (trace_id, question) DO UPDATE SET
            cluster_id = EXCLUDED.cluster_id,
            intent_name = EXCLUDED.intent_name,
            confidence = EXCLUDED.confidence,
            embedding = EXCLUDED.embedding,
            source_type = EXCLUDED.source_type,
            source = EXCLUDED.source,
            classified_at = now()"
    );

    storage
        .db()
        .execute(Statement::from_sql_and_values(
            pg(),
            &sql,
            vec![
                trace_id.into(),
                question.into(),
                (cluster_id as i32).into(),
                intent_name.into(),
                sea_orm::Value::Float(Some(confidence)),
                source_type.into(),
                source.into(),
            ],
        ))
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Insert classification failed: {e}")))?;

    Ok(())
}

pub(super) async fn get_intent_analytics(
    storage: &PostgresObservabilityStorage,
    days: u32,
) -> Result<Vec<IntentAnalyticsRow>, OxyError> {
    let sql = format!(
        "SELECT intent_name, count(*)::BIGINT AS cnt
        FROM observability_intent_classifications
        WHERE classified_at >= now() - INTERVAL '{days} days'
        GROUP BY intent_name
        ORDER BY cnt DESC"
    );

    let rows = IntentAnalyticsQueryRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        &sql,
        vec![],
    ))
    .all(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Intent analytics query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| IntentAnalyticsRow {
            intent_name: r.intent_name,
            count: r.cnt as u64,
        })
        .collect())
}

pub(super) async fn get_outliers(
    storage: &PostgresObservabilityStorage,
    limit: usize,
) -> Result<Vec<(String, String)>, OxyError> {
    let sql = "SELECT trace_id, question
        FROM observability_intent_classifications
        WHERE intent_name = 'unknown'
        ORDER BY classified_at DESC
        LIMIT $1";

    let rows = OutlierRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        sql,
        vec![(limit as i64).into()],
    ))
    .all(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Outliers query failed: {e}")))?;

    Ok(rows.into_iter().map(|r| (r.trace_id, r.question)).collect())
}

pub(super) async fn load_unknown_classifications(
    storage: &PostgresObservabilityStorage,
) -> Result<Vec<(String, String, Vec<f32>, String)>, OxyError> {
    let sql = "SELECT trace_id, question, embedding::TEXT AS embedding, source
        FROM observability_intent_classifications
        WHERE intent_name = 'unknown'";

    let rows = UnknownClassificationRow::find_by_statement(Statement::from_sql_and_values(
        pg(),
        sql,
        vec![],
    ))
    .all(storage.db())
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Unknown classifications query failed: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let embedding = parse_pg_float_array(&r.embedding);
            (r.trace_id, r.question, embedding, r.source)
        })
        .collect())
}

pub(super) async fn get_unknown_count(
    storage: &PostgresObservabilityStorage,
) -> Result<usize, OxyError> {
    let sql = "SELECT count(*)::BIGINT AS count FROM observability_intent_classifications WHERE intent_name = 'unknown'";

    let result = CountRow::find_by_statement(Statement::from_sql_and_values(pg(), sql, vec![]))
        .one(storage.db())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Unknown count query failed: {e}")))?;

    Ok(result.map(|r| r.count as usize).unwrap_or(0))
}

pub(super) async fn update_cluster_record(
    storage: &PostgresObservabilityStorage,
    cluster: &IntentCluster,
) -> Result<(), OxyError> {
    let centroid = format_pg_float_array(&cluster.centroid)?;
    let sample_questions_json =
        serde_json::to_string(&cluster.sample_questions).unwrap_or_else(|_| "[]".into());

    let sql = format!(
        "INSERT INTO observability_intent_clusters
         (cluster_id, intent_name, intent_description, centroid,
          sample_questions, question_count, updated_at)
         VALUES ($1, $2, $3, {centroid}, $4::JSONB, $5, now())
         ON CONFLICT (cluster_id) DO UPDATE SET
            intent_name = EXCLUDED.intent_name,
            intent_description = EXCLUDED.intent_description,
            centroid = EXCLUDED.centroid,
            sample_questions = EXCLUDED.sample_questions,
            question_count = EXCLUDED.question_count,
            updated_at = now()"
    );

    storage
        .db()
        .execute(Statement::from_sql_and_values(
            pg(),
            &sql,
            vec![
                (cluster.cluster_id as i32).into(),
                cluster.intent_name.clone().into(),
                cluster.intent_description.clone().into(),
                sample_questions_json.into(),
                (cluster.sample_questions.len() as i64).into(),
            ],
        ))
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Upsert cluster failed: {e}")))?;

    Ok(())
}

pub(super) async fn get_next_cluster_id(
    storage: &PostgresObservabilityStorage,
) -> Result<u32, OxyError> {
    let sql =
        "SELECT COALESCE(MAX(cluster_id), 0)::INTEGER AS max_id FROM observability_intent_clusters";

    let result = MaxIdRow::find_by_statement(Statement::from_sql_and_values(pg(), sql, vec![]))
        .one(storage.db())
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Next cluster ID query failed: {e}")))?;

    Ok((result.map(|r| r.max_id).unwrap_or(0) + 1) as u32)
}
