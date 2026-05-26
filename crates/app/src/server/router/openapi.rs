//! The OpenAPI router used by Swagger UI. Kept separate from the runtime
//! router because its route set is curated (not every handler is exposed).

use axum::body::Body;
use axum::http::Request;
use sentry::integrations::tower::NewSentryLayer;
use tower::ServiceBuilder;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::{agent, api_keys, app, database, healthcheck, run, thread, workspaces};

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
        // Workflow + automation routes have been retired; the new workflow
        // surface lives under `/agentic-workflows` (see agentic-http).
        // Database routes
        .routes(routes!(database::create_database_config))
        .routes(routes!(database::test_database_connection))
        .layer(build_cors_layer())
        .layer(ServiceBuilder::new().layer(NewSentryLayer::<Request<Body>>::new_from_top()))
}
