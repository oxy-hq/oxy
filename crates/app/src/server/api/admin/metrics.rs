//! `/admin/metrics/*` — cross-tenant operator metrics for the admin
//! dashboard. The headline is **LLM cost**: token usage is persisted in
//! `agentic_run_events` (JSONB payloads on `llm_start`/`llm_end`), but dollar
//! cost is *computed at read time* from per-model rates
//! (`agentic_llm::pricing`). So this module sums tokens in SQL, then prices
//! each model bucket in Rust.
//!
//! Mounted under the permissive `oxy_owner_or_app_admin` guard with the rest
//! of `/admin/*`.

use agentic_llm::pricing::cost_for_call;
use axum::Json;
use axum::Router;
use axum::extract::{Path, Query};
use axum::response::Response;
use axum::routing::get;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{DatabaseBackend, DbErr, FromQueryResult, Statement};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use super::internal_jobs::{connect, db_err};
use crate::server::router::AppState;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/metrics/llm-usage", get(llm_usage))
        .route("/metrics/orgs/{org_id}/llm-usage", get(org_llm_usage))
}

// Response shape

#[derive(Serialize, Debug, Default)]
pub struct UsageTotals {
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub run_count: i64,
    /// Runs whose model is in the pricing table (i.e. contribute to `cost_usd`).
    pub priced_run_count: i64,
}

#[derive(Serialize, Debug)]
pub struct DayCost {
    pub day: String,
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub run_count: i64,
}

#[derive(Serialize, Debug)]
pub struct ModelCost {
    pub model: String,
    /// `None` when the model isn't in the pricing table — tokens are still
    /// reported so the UI can flag "unpriced" usage rather than hide it.
    pub cost_usd: Option<f64>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cache_read_tokens: i64,
    pub run_count: i64,
}

#[derive(Serialize, Debug)]
pub struct OrgCost {
    pub org_id: Uuid,
    pub org_name: String,
    pub org_slug: String,
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub run_count: i64,
}

#[derive(Serialize, Debug)]
pub struct LlmUsageOverview {
    pub window_days: i32,
    pub total: UsageTotals,
    pub by_day: Vec<DayCost>,
    pub by_model: Vec<ModelCost>,
    pub by_org: Vec<OrgCost>,
}

/// Per-org usage detail. Unlike `LlmUsageOverview.by_org` (a cross-tenant
/// leaderboard truncated to the top 10 by cost), this is scoped to a single
/// org server-side, so it's correct for *any* tenant — and carries the daily
/// series for a trend sparkline.
#[derive(Serialize, Debug)]
pub struct OrgUsageDetail {
    pub window_days: i32,
    pub total: UsageTotals,
    pub by_day: Vec<DayCost>,
}

#[derive(Deserialize)]
struct UsageQuery {
    days: Option<i32>,
}

// Shared CTE

/// Per-run token rollup over the window. Each run collapses to ONE model (the
/// max `llm_end.model` — runs are effectively single-model), so downstream
/// GROUP BYs can attribute the run's tokens to a priced model bucket. Token
/// casts mirror `agentic_runtime`'s per-run usage query.
const RUN_USAGE_CTE: &str = "\
    WITH run_usage AS ( \
        SELECT \
            ar.id AS run_id, \
            ar.workspace_id AS workspace_id, \
            ar.created_at AS created_at, \
            max(e.payload->>'model') FILTER (WHERE e.event_type = 'llm_end') AS model, \
            COALESCE(SUM((e.payload->>'prompt_tokens')::bigint) \
                FILTER (WHERE e.event_type = 'llm_start'), 0) AS input_tokens, \
            COALESCE(SUM((e.payload->>'output_tokens')::bigint) \
                FILTER (WHERE e.event_type = 'llm_end'), 0) AS output_tokens, \
            COALESCE(SUM((e.payload->>'cache_creation_input_tokens')::bigint) \
                FILTER (WHERE e.event_type = 'llm_end'), 0) AS cache_creation, \
            COALESCE(SUM((e.payload->>'cache_read_input_tokens')::bigint) \
                FILTER (WHERE e.event_type = 'llm_end'), 0) AS cache_read \
        FROM agentic_runs ar \
        JOIN agentic_run_events e \
            ON e.run_id = ar.id AND e.event_type IN ('llm_start', 'llm_end') \
        WHERE ar.created_at > now() - make_interval(days => $1) \
        GROUP BY ar.id, ar.workspace_id, ar.created_at \
    ) ";

#[derive(Debug, FromQueryResult)]
struct DayModelRow {
    day: String,
    model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cache_creation: i64,
    cache_read: i64,
    run_count: i64,
}

#[derive(Debug, FromQueryResult)]
struct OrgModelRow {
    org_id: Uuid,
    org_name: String,
    org_slug: String,
    model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cache_creation: i64,
    cache_read: i64,
    run_count: i64,
}

// Handler

async fn llm_usage(Query(q): Query<UsageQuery>) -> Result<Json<LlmUsageOverview>, Response> {
    let days = q.days.unwrap_or(30).clamp(1, 365);
    let db = connect().await?;

    let day_rows = fetch_day_model_rows(&db, days).await.map_err(db_err)?;
    let org_rows = fetch_org_model_rows(&db, days).await.map_err(db_err)?;

    Ok(Json(build_overview(days, day_rows, org_rows)))
}

async fn fetch_day_model_rows(
    db: &sea_orm::DatabaseConnection,
    days: i32,
) -> Result<Vec<DayModelRow>, DbErr> {
    let sql = format!(
        "{RUN_USAGE_CTE} \
         SELECT to_char(date_trunc('day', created_at), 'YYYY-MM-DD') AS day, \
                model, \
                SUM(input_tokens)::bigint AS input_tokens, \
                SUM(output_tokens)::bigint AS output_tokens, \
                SUM(cache_creation)::bigint AS cache_creation, \
                SUM(cache_read)::bigint AS cache_read, \
                COUNT(*)::bigint AS run_count \
         FROM run_usage GROUP BY 1, 2 ORDER BY 1"
    );
    DayModelRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [days.into()],
    ))
    .all(db)
    .await
}

async fn fetch_org_model_rows(
    db: &sea_orm::DatabaseConnection,
    days: i32,
) -> Result<Vec<OrgModelRow>, DbErr> {
    let sql = format!(
        "{RUN_USAGE_CTE} \
         SELECT w.org_id AS org_id, o.name AS org_name, o.slug AS org_slug, ru.model AS model, \
                SUM(ru.input_tokens)::bigint AS input_tokens, \
                SUM(ru.output_tokens)::bigint AS output_tokens, \
                SUM(ru.cache_creation)::bigint AS cache_creation, \
                SUM(ru.cache_read)::bigint AS cache_read, \
                COUNT(*)::bigint AS run_count \
         FROM run_usage ru \
         JOIN workspaces w ON ru.workspace_id = w.id \
         JOIN organizations o ON w.org_id = o.id \
         GROUP BY w.org_id, o.name, o.slug, ru.model"
    );
    OrgModelRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [days.into()],
    ))
    .all(db)
    .await
}

async fn org_llm_usage(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path(org_id): Path<Uuid>,
    Query(q): Query<UsageQuery>,
) -> Result<Json<OrgUsageDetail>, Response> {
    let days = q.days.unwrap_or(30).clamp(1, 365);
    let db = connect().await?;
    // Scope. `PlatformOperate` is held by every Global Admin regardless of bound, so
    // unfenced this reads another tenant's LLM cost and token totals. Milder than the
    // subdomain toggle — a read, and cost rather than content — but the same axis, and
    // it is the fourth `{org_id}` router rather than a special case.
    crate::server::api::admin::scope::deny_out_of_scope(&db, &actor, org_id)
        .await
        .map_err(axum::response::IntoResponse::into_response)?;
    let day_rows = fetch_org_usage_day_rows(&db, days, org_id)
        .await
        .map_err(db_err)?;
    Ok(Json(build_org_detail(days, day_rows)))
}

/// Per-(day, model) token rollup scoped to one org's workspaces. Same shape as
/// [`fetch_day_model_rows`] but the run-usage CTE is narrowed with a
/// `workspaces.org_id` join so the result is correct for any tenant, not just
/// the top-10 cost leaders the cross-tenant `by_org` keeps. $1 = days,
/// $2 = org_id.
async fn fetch_org_usage_day_rows(
    db: &sea_orm::DatabaseConnection,
    days: i32,
    org_id: Uuid,
) -> Result<Vec<DayModelRow>, DbErr> {
    let sql = "\
        WITH run_usage AS ( \
            SELECT ar.id AS run_id, ar.created_at AS created_at, \
                max(e.payload->>'model') FILTER (WHERE e.event_type = 'llm_end') AS model, \
                COALESCE(SUM((e.payload->>'prompt_tokens')::bigint) \
                    FILTER (WHERE e.event_type = 'llm_start'), 0) AS input_tokens, \
                COALESCE(SUM((e.payload->>'output_tokens')::bigint) \
                    FILTER (WHERE e.event_type = 'llm_end'), 0) AS output_tokens, \
                COALESCE(SUM((e.payload->>'cache_creation_input_tokens')::bigint) \
                    FILTER (WHERE e.event_type = 'llm_end'), 0) AS cache_creation, \
                COALESCE(SUM((e.payload->>'cache_read_input_tokens')::bigint) \
                    FILTER (WHERE e.event_type = 'llm_end'), 0) AS cache_read \
            FROM agentic_runs ar \
            JOIN agentic_run_events e \
                ON e.run_id = ar.id AND e.event_type IN ('llm_start', 'llm_end') \
            JOIN workspaces w ON ar.workspace_id = w.id \
            WHERE ar.created_at > now() - make_interval(days => $1) AND w.org_id = $2 \
            GROUP BY ar.id, ar.created_at \
        ) \
        SELECT to_char(date_trunc('day', created_at), 'YYYY-MM-DD') AS day, \
               model, \
               SUM(input_tokens)::bigint AS input_tokens, \
               SUM(output_tokens)::bigint AS output_tokens, \
               SUM(cache_creation)::bigint AS cache_creation, \
               SUM(cache_read)::bigint AS cache_read, \
               COUNT(*)::bigint AS run_count \
        FROM run_usage GROUP BY 1, 2 ORDER BY 1";
    DayModelRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        sql,
        [days.into(), org_id.into()],
    ))
    .all(db)
    .await
}

/// Price a single model bucket. `None` model or an unknown model yields
/// `None` cost (tokens still count toward usage, just not dollars).
fn price(model: &Option<String>, input: i64, output: i64, cc: i64, cr: i64) -> Option<f64> {
    let model = model.as_deref()?;
    cost_for_call(
        model,
        input.max(0) as u64,
        output.max(0) as u64,
        cc.max(0) as u64,
        cr.max(0) as u64,
    )
}

/// Fold per-(day, model) rows into the window total + the day series. Shared by
/// the cross-tenant overview and the per-org detail so both price identically.
/// Day order is preserved from the SQL `ORDER BY day`.
fn fold_days(day_rows: &[DayModelRow]) -> (UsageTotals, Vec<DayCost>) {
    let mut total = UsageTotals::default();
    let mut by_day: Vec<DayCost> = Vec::new();
    let mut day_index: BTreeMap<String, usize> = BTreeMap::new();

    for r in day_rows {
        let cost = price(
            &r.model,
            r.input_tokens,
            r.output_tokens,
            r.cache_creation,
            r.cache_read,
        );

        total.input_tokens += r.input_tokens;
        total.output_tokens += r.output_tokens;
        total.cache_creation_tokens += r.cache_creation;
        total.cache_read_tokens += r.cache_read;
        total.run_count += r.run_count;
        if let Some(c) = cost {
            total.cost_usd += c;
            total.priced_run_count += r.run_count;
        }

        let idx = *day_index.entry(r.day.clone()).or_insert_with(|| {
            by_day.push(DayCost {
                day: r.day.clone(),
                cost_usd: 0.0,
                input_tokens: 0,
                output_tokens: 0,
                run_count: 0,
            });
            by_day.len() - 1
        });
        let d = &mut by_day[idx];
        d.cost_usd += cost.unwrap_or(0.0);
        d.input_tokens += r.input_tokens;
        d.output_tokens += r.output_tokens;
        d.run_count += r.run_count;
    }

    (total, by_day)
}

fn build_org_detail(days: i32, day_rows: Vec<DayModelRow>) -> OrgUsageDetail {
    let (total, by_day) = fold_days(&day_rows);
    OrgUsageDetail {
        window_days: days,
        total,
        by_day,
    }
}

fn build_overview(
    days: i32,
    day_rows: Vec<DayModelRow>,
    org_rows: Vec<OrgModelRow>,
) -> LlmUsageOverview {
    let (total, by_day) = fold_days(&day_rows);

    // by_model — fold the same day rows by model and price each bucket.
    let mut by_model: BTreeMap<String, ModelCost> = BTreeMap::new();
    for r in &day_rows {
        let cost = price(
            &r.model,
            r.input_tokens,
            r.output_tokens,
            r.cache_creation,
            r.cache_read,
        );
        let key = r.model.clone().unwrap_or_else(|| "unknown".to_string());
        let m = by_model.entry(key.clone()).or_insert_with(|| ModelCost {
            model: key,
            cost_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            run_count: 0,
        });
        m.input_tokens += r.input_tokens;
        m.output_tokens += r.output_tokens;
        m.cache_creation_tokens += r.cache_creation;
        m.cache_read_tokens += r.cache_read;
        m.run_count += r.run_count;
        if let Some(c) = cost {
            m.cost_usd = Some(m.cost_usd.unwrap_or(0.0) + c);
        }
    }

    // by_org — fold model rows per org, price each, keep the top 10 by cost.
    let mut org_map: BTreeMap<Uuid, OrgCost> = BTreeMap::new();
    for r in &org_rows {
        let cost = price(
            &r.model,
            r.input_tokens,
            r.output_tokens,
            r.cache_creation,
            r.cache_read,
        )
        .unwrap_or(0.0);
        let entry = org_map.entry(r.org_id).or_insert_with(|| OrgCost {
            org_id: r.org_id,
            org_name: r.org_name.clone(),
            org_slug: r.org_slug.clone(),
            cost_usd: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            run_count: 0,
        });
        entry.cost_usd += cost;
        entry.input_tokens += r.input_tokens;
        entry.output_tokens += r.output_tokens;
        entry.run_count += r.run_count;
    }
    let mut by_org: Vec<OrgCost> = org_map.into_values().collect();
    by_org.sort_by(|a, b| {
        b.cost_usd
            .partial_cmp(&a.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    by_org.truncate(10);

    let mut by_model: Vec<ModelCost> = by_model.into_values().collect();
    by_model.sort_by(|a, b| {
        b.cost_usd
            .unwrap_or(0.0)
            .partial_cmp(&a.cost_usd.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    LlmUsageOverview {
        window_days: days,
        total,
        by_day,
        by_model,
        by_org,
    }
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
