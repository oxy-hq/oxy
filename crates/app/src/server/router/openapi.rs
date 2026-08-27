//! The OpenAPI router used by Swagger UI. Kept separate from the runtime
//! router because its route set is curated (not every handler is exposed).

use axum::body::Body;
use axum::http::Request;
use oxy::config::constants::DEFAULT_API_KEY_HEADER;
use sentry::integrations::tower::NewSentryLayer;
use tower::ServiceBuilder;
use utoipa::openapi::ExternalDocs;
use utoipa::openapi::security::{
    ApiKey as OApiKey, ApiKeyValue, Http, HttpAuthScheme, SecurityRequirement, SecurityScheme,
};
use utoipa::openapi::server::Server;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::{agent, api_keys, app, database, healthcheck, run, thread, workspaces};

use super::{IdeState, build_cors_layer};

pub async fn openapi_router() -> OpenApiRouter<IdeState> {
    OpenApiRouter::new()
        .routes(routes!(healthcheck::health_check))
        .routes(routes!(healthcheck::readiness_check))
        .routes(routes!(healthcheck::liveness_check))
        .routes(routes!(healthcheck::version_info))
        .routes(routes!(agent::get_agents))
        .routes(routes!(api_keys::create_api_key))
        .routes(routes!(api_keys::list_api_keys))
        .routes(routes!(api_keys::get_api_key))
        .routes(routes!(api_keys::delete_api_key))
        .routes(routes!(app::list_apps))
        .routes(routes!(app::get_app_result))
        .routes(routes!(app::get_chart_image))
        .routes(routes!(workspaces::get_workspace))
        .routes(routes!(workspaces::get_workspace_branches))
        .routes(routes!(run::get_automation_runs))
        .routes(routes!(run::create_automation_run))
        .routes(routes!(run::cancel_automation_run))
        .routes(routes!(run::delete_automation_run))
        .routes(routes!(run::bulk_delete_automation_runs))
        .routes(routes!(run::automation_events))
        .routes(routes!(run::automation_events_sync))
        .routes(routes!(run::get_blocks))
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

/// Markdown rendered in the Swagger UI header, and carried as the spec's
/// `info.description`. Documents both the HTTP API and the CLI tools
/// (`oxy login`, `oxy api`) that consume it; the body lives in `apidoc.md` so
/// the long markdown stays out of the code.
const APIDOC_DESCRIPTION: &str = include_str!("../../cli/commands/apidoc.md");

/// The finished OpenAPI document: [`openapi_router`]'s operations plus the
/// title, description, security schemes and server prefix that make it a
/// usable spec rather than a bare path list.
///
/// Two consumers, deliberately one builder: `oxy serve` hands it to Swagger UI
/// at `/apidoc`, and `oxy api --openapi` prints it offline. A caller who can
/// only reach the binary gets exactly the document the running server serves.
///
/// Scope: `openapi_router`'s route set is **curated**, not the whole surface —
/// it is where the request/response *schemas* live. `oxy api --routes` is the
/// complete endpoint list.
pub async fn build_openapi_doc() -> utoipa::openapi::OpenApi {
    let mut doc = openapi_router().await.into_openapi();

    doc.info.title = "Oxy API".to_string();
    doc.info.description = Some(APIDOC_DESCRIPTION.to_string());
    doc.info.contact = None;
    doc.info.license = None;

    let mut external_docs = ExternalDocs::new("https://oxygen-hq.com/docs");
    external_docs.description = Some("Oxy documentation".to_string());
    doc.external_docs = Some(external_docs);

    let mut components = doc.components.take().unwrap_or_default();
    // API key scheme — for service-to-service calls (X-API-Key header).
    components.security_schemes.insert(
        "ApiKey".to_string(),
        SecurityScheme::ApiKey(OApiKey::Header(ApiKeyValue::new(
            DEFAULT_API_KEY_HEADER.to_string(),
        ))),
    );
    // Bearer scheme — the JWT issued by `oxy login` (and returned by the
    // magic-link flow). Pass via `Authorization: Bearer <token>`.
    components.security_schemes.insert(
        "BearerAuth".to_string(),
        SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
    );
    doc.components = Some(components);

    // Endpoints accept either scheme; clients only need to supply one.
    doc.security = Some(vec![
        SecurityRequirement::new("ApiKey", Vec::<String>::new()),
        SecurityRequirement::new("BearerAuth", Vec::<String>::new()),
    ]);
    doc.servers = Some(vec![Server::new("/api")]);
    doc
}
