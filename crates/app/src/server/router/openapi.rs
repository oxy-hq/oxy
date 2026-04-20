//! The OpenAPI router used by Swagger UI. Kept separate from the runtime
//! router because its route set is curated (not every handler is exposed).

use axum::body::Body;
use axum::http::Request;
use sentry::integrations::tower::NewSentryLayer;
use tower::ServiceBuilder;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::{agent, api_keys, app, database, healthcheck, run, thread, workflow, workspaces};

use super::{AppState, build_cors_layer};

pub async fn openapi_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Health check routes
        .routes(routes!(healthcheck::health_check))
        .routes(routes!(healthcheck::readiness_check))
        .routes(routes!(healthcheck::liveness_check))
        .routes(routes!(healthcheck::version_info))
        // Agent routes
        .routes(routes!(agent::get_agents))
        .routes(routes!(agent::ask_agent_preview))
        .routes(routes!(agent::ask_agent_sync))
        // API Keys routes
        .routes(routes!(api_keys::create_api_key))
        .routes(routes!(api_keys::list_api_keys))
        .routes(routes!(api_keys::get_api_key))
        .routes(routes!(api_keys::delete_api_key))
        // App routes
        .routes(routes!(app::list_apps))
        .routes(routes!(app::get_app_result))
        .routes(routes!(app::get_chart_image))
        // Workspace routes
        .routes(routes!(workspaces::get_workspace))
        .routes(routes!(workspaces::get_workspace_branches))
        // Run routes
        .routes(routes!(run::get_workflow_runs))
        .routes(routes!(run::create_workflow_run))
        .routes(routes!(run::cancel_workflow_run))
        .routes(routes!(run::delete_workflow_run))
        .routes(routes!(run::bulk_delete_workflow_runs))
        .routes(routes!(run::workflow_events))
        .routes(routes!(run::workflow_events_sync))
        .routes(routes!(run::get_blocks))
        // Thread routes
        .routes(routes!(thread::get_threads))
        .routes(routes!(thread::get_thread))
        .routes(routes!(thread::create_thread))
        .routes(routes!(thread::delete_thread))
        .routes(routes!(thread::delete_all_threads))
        .routes(routes!(thread::stop_thread))
        .routes(routes!(thread::bulk_delete_threads))
        .routes(routes!(thread::get_logs))
        // Workflow routes
        .routes(routes!(workflow::list))
        .routes(routes!(workflow::get))
        .routes(routes!(workflow::get_logs))
        .routes(routes!(workflow::run_workflow))
        .routes(routes!(workflow::run_workflow_sync))
        .routes(routes!(workflow::run_workflow_thread))
        .routes(routes!(workflow::run_workflow_thread_sync))
        .routes(routes!(workflow::create_from_query))
        .routes(routes!(workflow::get_workflow_run))
        // Automation routes
        .routes(routes!(workflow::save_automation))
        // Database routes
        .routes(routes!(database::create_database_config))
        .routes(routes!(database::test_database_connection))
        .layer(build_cors_layer())
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top()))
}
