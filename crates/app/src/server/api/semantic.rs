//! Semantic-layer endpoints for the IDE:
//!
//! - `GET /semantic/topic/{pathb64}` — parse one `.topic.yml` and hydrate its views.
//! - `GET /semantic/view/{pathb64}` — parse one `.view.yml`.
//! - `POST /semantic/compile` — compile a `{ topic, dimensions, measures, … }`
//!   query into dialect-specific SQL via airlayer.
//! - `POST /semantic` — compile **and execute** the same query, returning
//!   rows (JSON) or a parquet file handle. Used by the IDE's "Run" button.
//!
//! Compile + execute both go through `agentic_workflow::semantic_bridge`
//! and `agentic_connector` — same code paths the agentic pipeline's
//! `semantic_query` step uses, so IDE results stay in lockstep with
//! runtime results without re-introducing `oxy-workflow`.

use axum::{
    extract::{self, Path},
    http::StatusCode,
};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use oxy_semantic::parse_semantic_layer_from_dir;
use oxy_semantic::parser::{ParserConfig, SemanticLayerParser};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use agentic_workflow::config::SemanticQueryConfig;
use agentic_workflow::semantic_bridge::resolve_and_compile;
use oxy::adapters::session_filters::SessionFilters;
use oxy::config::model::ConnectionOverrides;
use oxy_auth::extractor::AuthenticatedUserExtractor;

use crate::server::api::data::{
    ResultFormat, SQLParams, SemanticQueryResponse, SqlErrorResponse, SqlExecuteError,
    agentic_error_response, run_via_agentic_connector,
};
use crate::server::api::middlewares::workspace_context::{
    EffectiveWorkspaceRole, WorkspaceManagerExtractor,
};

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub message: String,
}

#[derive(Deserialize)]
pub struct ViewPath {
    pub workspace_id: Uuid,
    pub file_path_b64: String,
}

#[derive(Deserialize)]
pub struct TopicPath {
    pub workspace_id: Uuid,
    pub file_path_b64: String,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct ViewResponse {
    pub view_name: String,
    pub name: String,
    pub description: Option<String>,
    pub datasource: Option<String>,
    pub table: Option<String>,
    #[schema(value_type = Vec<Object>)]
    pub dimensions: Vec<serde_json::Value>,
    #[schema(value_type = Vec<Object>)]
    pub measures: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct TopicResponse {
    pub name: String,
    pub description: Option<String>,
    pub views: Vec<String>,
    pub base_view: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct TopicDetailsResponse {
    pub topic: TopicResponse,
    pub views: Vec<ViewResponse>,
}

/// Decode a base64-encoded workspace-relative file path, accepting both
/// the standard padded form (`...==`) and the unpadded form the IDE
/// historically used. Mirrors the same `path_b64` tolerance the
/// agentic-workflow file route applies — keeps the routes consistent
/// for clients that pick one form or the other.
fn decode_b64_path(
    file_path_b64: &str,
) -> Result<String, (StatusCode, extract::Json<ErrorResponse>)> {
    let decoded = BASE64_STANDARD
        .decode(file_path_b64)
        .or_else(|_| {
            // Tolerate unpadded base64 by retrying with the standard padding rule.
            let pad = (4 - (file_path_b64.len() % 4)) % 4;
            let padded = format!("{file_path_b64}{}", "=".repeat(pad));
            BASE64_STANDARD.decode(padded)
        })
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                extract::Json(ErrorResponse {
                    message: format!("Invalid base64 file path: {e}"),
                }),
            )
        })?;
    String::from_utf8(decoded).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: format!("Invalid UTF-8 in file path: {e}"),
            }),
        )
    })
}

pub async fn get_view_details(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(ViewPath {
        workspace_id: _,
        file_path_b64,
    }): Path<ViewPath>,
) -> Result<extract::Json<ViewResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    let file_path_str = decode_b64_path(&file_path_b64)?;

    let full_path_str = workspace_manager
        .config_manager
        .resolve_file(&file_path_str)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: format!("Failed to resolve file path: {e}"),
                }),
            )
        })?;
    let full_path = std::path::PathBuf::from(full_path_str);
    if !full_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            extract::Json(ErrorResponse {
                message: format!("View file {file_path_str} not found"),
            }),
        ));
    }

    let parser_config = ParserConfig::new(workspace_manager.config_manager.semantics_scan_path());
    let parser = SemanticLayerParser::new(parser_config);
    let view = parser.parse_view_file(&full_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to parse view file: {e}"),
            }),
        )
    })?;

    Ok(extract::Json(ViewResponse {
        view_name: view.name.clone(),
        name: view.name,
        description: view.description,
        datasource: view.datasource,
        table: view.table,
        dimensions: json_array(&view.dimensions),
        measures: view.measures.map(|m| json_array(&m)).unwrap_or_default(),
    }))
}

pub async fn get_topic_details(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(TopicPath {
        workspace_id: _,
        file_path_b64,
    }): Path<TopicPath>,
) -> Result<extract::Json<TopicDetailsResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    let file_path_str = decode_b64_path(&file_path_b64)?;
    let semantics_path = workspace_manager.config_manager.semantics_scan_path();

    let full_path_str = workspace_manager
        .config_manager
        .resolve_file(&file_path_str)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: format!("Failed to resolve file path: {e}"),
                }),
            )
        })?;
    let full_path = std::path::PathBuf::from(full_path_str);
    if !full_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            extract::Json(ErrorResponse {
                message: format!("Topic file {file_path_str} not found"),
            }),
        ));
    }

    let parser_config = ParserConfig::new(&semantics_path);
    let parser = SemanticLayerParser::new(parser_config);
    let topic = parser.parse_topic_file(&full_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to parse topic file: {e}"),
            }),
        )
    })?;

    // Hydrate referenced views from the full semantic layer parse. A
    // missing reference is a real authoring error — return 400 with
    // the offending name rather than silently dropping the view, so the
    // IDE can surface it to the user.
    let parse_result = parse_semantic_layer_from_dir(&semantics_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to parse semantic layer: {e}"),
            }),
        )
    })?;

    let mut views_with_data = Vec::with_capacity(topic.views.len());
    for view_name in &topic.views {
        let Some(view) = parse_result
            .semantic_layer
            .views
            .iter()
            .find(|v| v.name == *view_name)
        else {
            return Err((
                StatusCode::BAD_REQUEST,
                extract::Json(ErrorResponse {
                    message: format!("Could not find view {view_name} in semantic layer"),
                }),
            ));
        };
        views_with_data.push(ViewResponse {
            view_name: view_name.clone(),
            name: view.name.clone(),
            description: view.description.clone(),
            datasource: view.datasource.clone(),
            table: view.table.clone(),
            dimensions: json_array(&view.dimensions),
            measures: view.measures.as_ref().map(json_array).unwrap_or_default(),
        });
    }

    Ok(extract::Json(TopicDetailsResponse {
        topic: TopicResponse {
            name: topic.name,
            description: topic.description,
            views: topic.views,
            base_view: topic.base_view,
        },
        views: views_with_data,
    }))
}

/// Serialize anything to a `Vec<serde_json::Value>`, treating a
/// non-array serialization as empty. Lets the response shape stay
/// `Vec<Object>` regardless of whether the semantic model exposes the
/// field as an array or an option/scalar.
fn json_array<T: Serialize>(value: &T) -> Vec<serde_json::Value> {
    match serde_json::to_value(value).unwrap_or(serde_json::Value::Null) {
        serde_json::Value::Array(a) => a,
        _ => Vec::new(),
    }
}

// ── POST /semantic/compile ─────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct SemanticQueryCompileResponse {
    pub sql: String,
}

/// Compile a semantic query into dialect-specific SQL without executing
/// it. Used by the IDE's SQL preview panel — `SemanticExplorerContext`
/// fires this as the user clicks dimensions / measures / filters so the
/// preview stays in sync.
///
/// The request body is the same flat shape the FE has always sent
/// (`{ topic, dimensions, measures, time_dimensions, filters, orders,
/// limit, … }`); extra fields the FE includes for the execute path
/// (`variables`, `session_filters`, `connections`, `result_format`) are
/// tolerated by serde and ignored — compile doesn't need them.
///
/// Compilation goes through `agentic_workflow::semantic_bridge`, which
/// drives airlayer end-to-end. Same code path the agentic-pipeline's
/// `semantic_query` step uses, so the IDE preview and runtime stay in
/// lockstep.
pub async fn compile_semantic_query(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>,
    extract::Json(query): extract::Json<SemanticQueryConfig>,
) -> Result<extract::Json<SemanticQueryCompileResponse>, (StatusCode, extract::Json<ErrorResponse>)>
{
    let scan_path = workspace_manager.config_manager.semantics_scan_path();
    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();

    let (sql, _database_name) =
        tokio::task::spawn_blocking(move || resolve_and_compile(&scan_path, &databases, &query))
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    extract::Json(ErrorResponse {
                        message: format!("compile task panicked: {e}"),
                    }),
                )
            })?
            .map_err(|e| {
                // airlayer surfaces both "schema/validation" errors (bad
                // dimension name, missing topic, …) and "internal" ones with the
                // same shape. The FE renders the message inline, so 400 is the
                // friendlier default — the user typed something the layer
                // rejected, not the server falling over.
                (
                    StatusCode::BAD_REQUEST,
                    extract::Json(ErrorResponse {
                        message: e.to_string(),
                    }),
                )
            })?;

    Ok(extract::Json(SemanticQueryCompileResponse { sql }))
}

#[derive(Deserialize)]
pub struct WorkspacePath {
    pub workspace_id: Uuid,
}

// ── POST /semantic ─────────────────────────────────────────────────────────

/// FE request body for the IDE's "Run" button. Mirrors the legacy
/// `SemanticQueryRequest` shape so `executeSemanticQuery` on the FE
/// keeps working untouched.
///
/// The `query` field carries the topic / dimensions / measures / filters
/// just like `/semantic/compile`. `session_filters` / `connections` /
/// `result_format` are the execute-side knobs that `/sql/query` already
/// honors; they get plumbed through to the connector verbatim.
// `SemanticQueryConfig` lives in `agentic-workflow` and doesn't impl
// `ToSchema`, so this request type stays out of the curated OpenAPI
// surface (utoipa). The handler still deserializes it via serde.
#[derive(Deserialize)]
pub struct SemanticQueryExecuteRequest {
    #[serde(flatten)]
    pub query: SemanticQueryConfig,
    #[serde(default)]
    pub session_filters: Option<SessionFilters>,
    #[serde(default)]
    pub connections: Option<ConnectionOverrides>,
    #[serde(default)]
    pub result_format: Option<ResultFormat>,
}

/// Compile a semantic query to SQL, then execute it via the same
/// connector path `/sql/query` uses. Single round-trip from the FE.
///
/// Server-side database selection: the topic's first view with a
/// `datasource:` set wins, so the FE can't accidentally route SQL to
/// the wrong warehouse — `resolve_and_compile` is the source of truth
/// for both the SQL and the database name.
pub async fn execute_semantic_query(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>,
    extract::Json(payload): extract::Json<SemanticQueryExecuteRequest>,
) -> Result<extract::Json<SemanticQueryResponse>, (StatusCode, extract::Json<SqlErrorResponse>)> {
    let scan_path = workspace_manager.config_manager.semantics_scan_path();
    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();

    let query = payload.query;
    let (sql, database) =
        tokio::task::spawn_blocking(move || resolve_and_compile(&scan_path, &databases, &query))
            .await
            .map_err(|e| {
                // Task panic — distinct from a compile error, but the
                // FE only renders `message`, so a 500 is fine.
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    extract::Json(SqlErrorResponse {
                        message: format!("compile task panicked: {e}"),
                        code: None,
                        detail: None,
                        hint: None,
                        position: None,
                        sql: None,
                    }),
                )
            })?
            .map_err(|e| {
                // Bad topic / unknown dimension / etc. → 400. Same
                // reasoning as `/semantic/compile`'s error mapping.
                (
                    StatusCode::BAD_REQUEST,
                    extract::Json(SqlErrorResponse {
                        message: e.to_string(),
                        code: None,
                        detail: None,
                        hint: None,
                        position: None,
                        sql: None,
                    }),
                )
            })?;

    let sql_payload = SQLParams {
        sql,
        database,
        filters: payload.session_filters,
        connections: payload.connections,
        result_format: payload.result_format,
    };

    run_via_agentic_connector(&workspace_manager, user.id, role, &sql_payload)
        .await
        .map(extract::Json)
        .map_err(|e: SqlExecuteError| agentic_error_response(&sql_payload, e))
}
