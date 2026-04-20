use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260416_000001_create_observability_tables"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // ── Spans table ───────────────────────────────────────────────────
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS observability_spans (
                trace_id VARCHAR NOT NULL,
                span_id VARCHAR NOT NULL,
                parent_span_id VARCHAR DEFAULT '',
                span_name VARCHAR NOT NULL,
                service_name VARCHAR DEFAULT 'oxy',
                span_attributes JSONB DEFAULT '{}',
                duration_ns BIGINT DEFAULT 0,
                status_code VARCHAR DEFAULT 'UNSET',
                status_message VARCHAR DEFAULT '',
                event_data JSONB DEFAULT '[]',
                timestamp TIMESTAMPTZ DEFAULT now(),
                PRIMARY KEY (trace_id, span_id)
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_obs_spans_timestamp ON observability_spans(timestamp DESC)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_obs_spans_name ON observability_spans(span_name)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_obs_spans_parent ON observability_spans(parent_span_id)",
        )
        .await?;

        // ── Intent clusters table ─────────────────────────────────────────
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS observability_intent_clusters (
                cluster_id INTEGER PRIMARY KEY,
                intent_name VARCHAR NOT NULL,
                intent_description VARCHAR DEFAULT '',
                centroid REAL[],
                sample_questions JSONB DEFAULT '[]',
                question_count BIGINT DEFAULT 0,
                created_at TIMESTAMPTZ DEFAULT now(),
                updated_at TIMESTAMPTZ DEFAULT now()
            )
            "#,
        )
        .await?;

        // ── Intent classifications table ──────────────────────────────────
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS observability_intent_classifications (
                trace_id VARCHAR NOT NULL,
                question VARCHAR NOT NULL,
                cluster_id INTEGER DEFAULT 0,
                intent_name VARCHAR DEFAULT 'unknown',
                confidence REAL DEFAULT 0.0,
                embedding REAL[],
                source_type VARCHAR DEFAULT 'agent',
                source VARCHAR DEFAULT '',
                classified_at TIMESTAMPTZ DEFAULT now(),
                PRIMARY KEY (trace_id, question)
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_obs_ic_trace ON observability_intent_classifications(trace_id)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_obs_ic_classified_at ON observability_intent_classifications(classified_at DESC)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_obs_ic_intent ON observability_intent_classifications(intent_name)",
        )
        .await?;

        // ── Metric usage table ────────────────────────────────────────────
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS observability_metric_usage (
                id UUID DEFAULT gen_random_uuid(),
                metric_name VARCHAR NOT NULL,
                source_type VARCHAR DEFAULT '',
                source_ref VARCHAR DEFAULT '',
                context TEXT DEFAULT '',
                context_types JSONB DEFAULT '[]',
                trace_id VARCHAR DEFAULT '',
                created_at TIMESTAMPTZ DEFAULT now()
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_obs_mu_metric ON observability_metric_usage(metric_name, created_at)",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_obs_mu_trace ON observability_metric_usage(trace_id)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS observability_metric_usage")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS observability_intent_classifications")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS observability_intent_clusters")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS observability_spans")
            .await?;
        Ok(())
    }
}
