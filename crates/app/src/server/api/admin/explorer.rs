//! `/admin/explorer/*` — cross-tenant, read-only search over the DB resources
//! operators reach for when debugging: **threads** and **runs**. Each row is
//! enriched with the workspace / org / user it belongs to and deep-link ids,
//! so an operator can go from "this conversation looks broken" straight to the
//! owning tenant.
//!
//! Mounted under the permissive `oxy_owner_or_app_admin` guard with the rest
//! of `/admin/*`. Read-only — no mutations live here.
//!
//! Perf note: the search uses leading-wildcard `ILIKE '%term%'`, which can't
//! use a btree index and falls back to a sequential scan across every tenant's
//! `threads` / `agentic_runs`. The empty-term default is cheap (`ORDER BY
//! created_at DESC LIMIT`); only an actual term triggers the scan, and this is
//! an infrequent operator path. If it ever becomes hot, add a `pg_trgm` GIN
//! index on the searched columns — deferred for now to avoid the extension +
//! migration surface.

use axum::Json;
use axum::Router;
use axum::extract::Query;
use axum::routing::get;
use chrono::{DateTime, FixedOffset};
use sea_orm::{DatabaseBackend, DbErr, FromQueryResult, Statement};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::internal_jobs::{connect, db_err};
use crate::server::router::AppState;
use axum::response::Response;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/explorer/threads", get(search_threads))
        .route("/explorer/runs", get(search_runs))
}

#[derive(Deserialize)]
struct SearchQuery {
    search: Option<String>,
    status: Option<String>,
    limit: Option<u64>,
}

fn clamp_limit(limit: Option<u64>) -> i64 {
    limit.unwrap_or(50).clamp(1, 200) as i64
}

// ---------------------------------------------------------------------------
// Threads
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug, FromQueryResult)]
pub struct ThreadRow {
    pub id: Uuid,
    pub title: String,
    pub input_snippet: String,
    pub source_type: String,
    pub is_processing: bool,
    pub created_at: DateTime<FixedOffset>,
    pub user_email: Option<String>,
    pub workspace_id: Option<Uuid>,
    pub workspace_name: Option<String>,
    pub org_id: Option<Uuid>,
    pub org_name: Option<String>,
    pub org_slug: Option<String>,
}

async fn search_threads(Query(q): Query<SearchQuery>) -> Result<Json<Vec<ThreadRow>>, Response> {
    let db = connect().await?;
    let search = q.search.unwrap_or_default();
    let like = format!("%{search}%");
    let limit = clamp_limit(q.limit);

    let rows = ThreadRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        // $1 = raw term (exact-id match + empty-term passthrough), $2 = ILIKE
        // pattern, $3 = limit. Threads link to a workspace via `project_id`.
        "SELECT t.id AS id, t.title AS title, left(t.input, 200) AS input_snippet, \
                t.source_type AS source_type, t.is_processing AS is_processing, \
                t.created_at AS created_at, u.email AS user_email, \
                t.project_id AS workspace_id, w.name AS workspace_name, \
                w.org_id AS org_id, o.name AS org_name, o.slug AS org_slug \
         FROM threads t \
         LEFT JOIN users u ON t.user_id = u.id \
         LEFT JOIN workspaces w ON t.project_id = w.id \
         LEFT JOIN organizations o ON w.org_id = o.id \
         WHERE ($1 = '' OR t.title ILIKE $2 OR t.input ILIKE $2 OR t.output ILIKE $2 \
                OR t.id::text = $1) \
         ORDER BY t.created_at DESC \
         LIMIT $3",
        [search.into(), like.into(), limit.into()],
    ))
    .all(&db)
    .await
    .map_err(db_err)?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug, FromQueryResult)]
pub struct RunRow {
    pub id: String,
    pub question_snippet: String,
    pub task_status: Option<String>,
    pub source_type: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<FixedOffset>,
    pub thread_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub workspace_name: Option<String>,
    pub org_id: Option<Uuid>,
    pub org_name: Option<String>,
    pub org_slug: Option<String>,
    pub user_email: Option<String>,
}

async fn search_runs(Query(q): Query<SearchQuery>) -> Result<Json<Vec<RunRow>>, Response> {
    let db = connect().await?;
    let search = q.search.unwrap_or_default();
    let like = format!("%{search}%");
    let status = q.status.unwrap_or_default();
    let limit = clamp_limit(q.limit);

    let rows = RunRow::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        // $1 = raw term, $2 = ILIKE pattern, $3 = optional status filter,
        // $4 = limit. Originating user comes via the run's thread.
        "SELECT ar.id AS id, left(ar.question, 200) AS question_snippet, \
                ar.task_status AS task_status, ar.source_type AS source_type, \
                ar.error_message AS error_message, ar.created_at AS created_at, \
                ar.thread_id AS thread_id, ar.workspace_id AS workspace_id, \
                w.name AS workspace_name, w.org_id AS org_id, \
                o.name AS org_name, o.slug AS org_slug, u.email AS user_email \
         FROM agentic_runs ar \
         LEFT JOIN workspaces w ON ar.workspace_id = w.id \
         LEFT JOIN organizations o ON w.org_id = o.id \
         LEFT JOIN threads th ON ar.thread_id = th.id \
         LEFT JOIN users u ON th.user_id = u.id \
         WHERE ($1 = '' OR ar.question ILIKE $2 OR ar.error_message ILIKE $2 OR ar.id = $1) \
           AND ($3 = '' OR ar.task_status = $3) \
         ORDER BY ar.created_at DESC \
         LIMIT $4",
        [search.into(), like.into(), status.into(), limit.into()],
    ))
    .all(&db)
    .await
    .map_err(db_err)?;
    Ok(Json(rows))
}
