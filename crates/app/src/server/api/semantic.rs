use crate::server::api::middlewares::project::{ProjectManagerExtractor, ProjectPath};
use crate::server::api::result_files::store_result_file;
use crate::server::service::types::SemanticQueryParams;
use axum::{
    extract::{self, Path},
    http::StatusCode,
};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use oxy::adapters::session_filters::SessionFilters;
use oxy::config::model::{ConnectionOverrides, SemanticQueryTask};
use oxy::connector::load_result;
use oxy::execute::{
    Executable, ExecutionContext,
    renderer::Renderer,
    types::{Output, Source, utils::record_batches_to_2d_array},
};
use oxy_semantic::parse_semantic_layer_from_dir;
use oxy_workflow::semantic_builder::{SemanticQueryExecutable, render_semantic_query};
use oxy_workflow::semantic_validator_builder::validate_semantic_query_task;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ResultFormat {
    Parquet,
    #[default]
    Json,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct SemanticQueryRequest {
    #[serde(flatten)]
    pub query: SemanticQueryParams,

    #[serde(default)]
    pub session_filters: Option<SessionFilters>,

    #[serde(default)]
    #[schema(value_type = Object)]
    pub connections: Option<ConnectionOverrides>,

    #[serde(default)]
    pub result_format: Option<ResultFormat>,
}

#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub enum SemanticQueryResponse {
    Json(Vec<Vec<String>>),
    Parquet { file_name: String },
}

#[derive(Serialize, ToSchema)]
pub struct SemanticQueryCompileResponse {
    pub sql: String,
}

pub async fn execute_semantic_query(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath {
        project_id: _project_id,
    }): Path<ProjectPath>,
    extract::Json(payload): extract::Json<SemanticQueryRequest>,
) -> Result<extract::Json<SemanticQueryResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    // Create a dummy execution context
    let (tx, _rx) = mpsc::channel(100);
    let renderer = Renderer::new(minijinja::Value::default());

    let execution_context = ExecutionContext {
        source: Source {
            id: "api".to_string(),
            kind: "api".to_string(),
            parent_id: None,
        },
        writer: tx,
        renderer: renderer.clone(),
        project: project_manager.clone(),
        checkpoint: None,
        filters: payload.session_filters,
        connections: payload.connections,
        sandbox_info: None,
        user_id: None,
    };

    // Construct SemanticQueryTask
    let task = SemanticQueryTask {
        variables: payload.query.variables.clone(),
        query: payload.query,
        export: None,
    };

    // Render the query
    let rendered_task = render_semantic_query(&renderer, &task).map_err(|e| {
        tracing::error!("Failed to render semantic query: {}", e);
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })?;

    // Validate the query
    let validated_query =
        validate_semantic_query_task(&project_manager.config_manager, &rendered_task)
            .await
            .map_err(|e| {
                tracing::error!("Failed to validate semantic query: {}", e);
                (
                    StatusCode::BAD_REQUEST,
                    extract::Json(ErrorResponse {
                        message: e.to_string(),
                    }),
                )
            })?;

    // Execute the query
    let mut executable = SemanticQueryExecutable::new();
    let output = executable
        .execute(&execution_context, validated_query)
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute semantic query: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: e.to_string(),
                }),
            )
        })?;

    match output {
        Output::Table(table) => {
            let result_format = payload
                .result_format
                .as_ref()
                .unwrap_or(&ResultFormat::Json);

            match result_format {
                ResultFormat::Parquet => {
                    let file_name = store_result_file(&project_manager, &table.file_path)
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                extract::Json(ErrorResponse { message: e }),
                            )
                        })?;

                    Ok(extract::Json(SemanticQueryResponse::Parquet { file_name }))
                }
                ResultFormat::Json => {
                    let (batches, schema) = load_result(&table.file_path).map_err(|e| {
                        tracing::error!("Failed to load result: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            extract::Json(ErrorResponse {
                                message: e.to_string(),
                            }),
                        )
                    })?;

                    let data = record_batches_to_2d_array(&batches, &schema).map_err(|e| {
                        tracing::error!("Failed to convert batches to 2d array: {}", e);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            extract::Json(ErrorResponse {
                                message: e.to_string(),
                            }),
                        )
                    })?;
                    Ok(extract::Json(SemanticQueryResponse::Json(data)))
                }
            }
        }
        _ => {
            tracing::error!("Semantic query execution returned unexpected output type");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: "Semantic query execution returned unexpected output type".to_string(),
                }),
            ))
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub message: String,
}

pub async fn compile_semantic_query(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ProjectPath {
        project_id: _project_id,
    }): Path<ProjectPath>,
    extract::Json(payload): extract::Json<SemanticQueryRequest>,
) -> Result<extract::Json<SemanticQueryCompileResponse>, (StatusCode, extract::Json<ErrorResponse>)>
{
    // Create a dummy execution context
    let (tx, _rx) = mpsc::channel(100);
    let renderer = Renderer::new(minijinja::Value::default());

    let execution_context = ExecutionContext {
        source: Source {
            id: "api".to_string(),
            kind: "api".to_string(),
            parent_id: None,
        },
        writer: tx,
        renderer: renderer.clone(),
        project: project_manager.clone(),
        checkpoint: None,
        filters: payload.session_filters,
        connections: payload.connections,
        sandbox_info: None,
        user_id: None,
    };

    // Construct SemanticQueryTask
    let task = SemanticQueryTask {
        variables: payload.query.variables.clone(),
        query: payload.query,
        export: None,
    };

    // Render the query
    let rendered_task = render_semantic_query(&renderer, &task).map_err(|e| {
        tracing::error!("Failed to render semantic query: {}", e);
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })?;

    // Validate the query
    let validated_query =
        validate_semantic_query_task(&project_manager.config_manager, &rendered_task)
            .await
            .map_err(|e| {
                tracing::error!("Failed to validate semantic query: {}", e);
                (
                    StatusCode::BAD_REQUEST,
                    extract::Json(ErrorResponse {
                        message: e.to_string(),
                    }),
                )
            })?;

    // Compile the query
    let mut executable = SemanticQueryExecutable::new();
    let sql = executable
        .compile(&execution_context, validated_query)
        .await
        .map_err(|e| {
            tracing::error!("Failed to compile semantic query: {}", e);
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
pub struct ViewPath {
    pub project_id: Uuid,
    pub file_path_b64: String,
}

#[derive(Deserialize)]
pub struct TopicPath {
    pub project_id: Uuid,
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

pub async fn get_view_details(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(ViewPath {
        project_id: _project_id,
        file_path_b64,
    }): Path<ViewPath>,
) -> Result<extract::Json<ViewResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    use oxy_semantic::parser::ParserConfig;
    use oxy_semantic::parser::SemanticLayerParser;

    let global_registry = project_manager.config_manager.get_globals_registry();

    // Decode base64 file path
    let decoded_path = BASE64_STANDARD.decode(&file_path_b64).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: format!("Invalid base64 file path: {}", e),
            }),
        )
    })?;

    let file_path_str = String::from_utf8(decoded_path).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: format!("Invalid UTF-8 in file path: {}", e),
            }),
        )
    })?;

    // Resolve the file path using the config manager
    let full_path_str = project_manager
        .config_manager
        .resolve_file(&file_path_str)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: format!("Failed to resolve file path: {}", e),
                }),
            )
        })?;

    let full_path = std::path::PathBuf::from(full_path_str);

    // Verify file exists
    if !full_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            extract::Json(ErrorResponse {
                message: format!("View file {} not found", file_path_str),
            }),
        ));
    }

    // Parse the specific view file
    let parser_config = ParserConfig::new(project_manager.config_manager.semantics_path());
    let parser = SemanticLayerParser::new(parser_config, global_registry);

    let view = parser.parse_view_file(&full_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to parse view file: {}", e),
            }),
        )
    })?;

    Ok(extract::Json(ViewResponse {
        view_name: view.name.clone(),
        name: view.name,
        description: Some(view.description),
        datasource: view.datasource,
        table: view.table,
        dimensions: serde_json::to_value(view.dimensions)
            .unwrap_or(serde_json::Value::Array(vec![]))
            .as_array()
            .unwrap()
            .clone(),
        measures: view
            .measures
            .map(|m| {
                serde_json::to_value(m)
                    .unwrap_or(serde_json::Value::Array(vec![]))
                    .as_array()
                    .unwrap()
                    .clone()
            })
            .unwrap_or_default(),
    }))
}

pub async fn get_topic_details(
    ProjectManagerExtractor(project_manager): ProjectManagerExtractor,
    Path(TopicPath {
        project_id: _project_id,
        file_path_b64,
    }): Path<TopicPath>,
) -> Result<extract::Json<TopicDetailsResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    use oxy_semantic::parser::ParserConfig;
    use oxy_semantic::parser::SemanticLayerParser;

    let global_registry = project_manager.config_manager.get_globals_registry();
    let semantics_path = project_manager.config_manager.semantics_path();

    // Decode base64 file path
    let decoded_path = BASE64_STANDARD.decode(&file_path_b64).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: format!("Invalid base64 file path: {}", e),
            }),
        )
    })?;

    let file_path_str = String::from_utf8(decoded_path).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: format!("Invalid UTF-8 in file path: {}", e),
            }),
        )
    })?;

    // Resolve the file path using the config manager
    let full_path_str = project_manager
        .config_manager
        .resolve_file(&file_path_str)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: format!("Failed to resolve file path: {}", e),
                }),
            )
        })?;

    let full_path = std::path::PathBuf::from(full_path_str);

    // Verify file exists
    if !full_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            extract::Json(ErrorResponse {
                message: format!("Topic file {} not found", file_path_str),
            }),
        ));
    }

    // Parse the specific topic file
    let parser_config = ParserConfig::new(&semantics_path);
    let parser = SemanticLayerParser::new(parser_config, global_registry.clone());

    let topic = parser.parse_topic_file(&full_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to parse topic file: {}", e),
            }),
        )
    })?;

    // Parse the semantic layer to get all views for lookups
    let parse_result =
        parse_semantic_layer_from_dir(&semantics_path, global_registry).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: format!("Failed to parse semantic layer: {}", e),
                }),
            )
        })?;

    let mut views_with_data = Vec::new();

    for view_name in &topic.views {
        if let Some(view) = parse_result
            .semantic_layer
            .views
            .iter()
            .find(|v| v.name == *view_name)
        {
            views_with_data.push(ViewResponse {
                view_name: view_name.clone(),
                name: view.name.clone(),
                description: Some(view.description.clone()),
                datasource: view.datasource.clone(),
                table: view.table.clone(),
                dimensions: serde_json::to_value(&view.dimensions)
                    .unwrap_or(serde_json::Value::Array(vec![]))
                    .as_array()
                    .unwrap()
                    .clone(),
                measures: view
                    .measures
                    .as_ref()
                    .map(|m| {
                        serde_json::to_value(m)
                            .unwrap_or(serde_json::Value::Array(vec![]))
                            .as_array()
                            .unwrap()
                            .clone()
                    })
                    .unwrap_or_default(),
            });
        } else {
            tracing::warn!("Could not find view {} in semantic layer", view_name);
            return Err((
                StatusCode::BAD_REQUEST,
                extract::Json(ErrorResponse {
                    message: format!("Could not find view {} in semantic layer", view_name),
                }),
            ));
        }
    }

    Ok(extract::Json(TopicDetailsResponse {
        topic: TopicResponse {
            name: topic.name,
            description: Some(topic.description),
            views: topic.views,
            base_view: topic.base_view,
        },
        views: views_with_data,
    }))
}
