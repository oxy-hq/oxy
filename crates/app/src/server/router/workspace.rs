//! The per-workspace route tree. Mounted under `/{workspace_id}` in both
//! cloud and local modes (local always uses the nil UUID).
//!
//! This module owns the tree shape plus all per-resource sub-builders
//! (automations, threads, agents, files, etc.). Secrets live in their own
//! module because they ship with an admin-gated middleware.

use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post, put};

use agentic_http::{AgenticState, airway_router, automation_router, router as agentic_router};

use crate::api::{
    agent, api_keys, app, apps, artifacts, automation, chart, competitors, compile, data,
    data_repo, database, execution_analytics, exported_chart, file, foot_traffic, integration,
    local_setup, message, metric_anomalies, metric_tree, metrics, modeling, org_subdomain,
    pipeline, preagg, result_files, run, schedules, semantic, task, test_file, test_project_run,
    test_run, thread, traces, video, workspace_custom_apps, workspace_logo, workspace_members,
    workspace_oxy_access, workspaces, world_model, world_model_graph,
};

use super::AppState;
use super::secrets::build_secret_routes;

pub(super) fn build_workspace_routes(
    app_state: AppState,
    agentic_state: Arc<AgenticState>,
    include_git_features: bool,
    include_local_setup: bool,
) -> Router<AppState> {
    let mut router = Router::new()
        .route("/details", get(workspaces::get_workspace))
        .route("/status", get(workspaces::get_workspace_status))
        // Diagnostic: the workspace's live git-worktree lifecycle on the ide
        // (branch / idle / clean / would-reap). IdeOnly — the registry is
        // ide-local, so the serve fleet forwards here.
        .route(
            "/worktrees",
            get(crate::server::worktree_registry::get_worktree_status),
        )
        // Camera fleet operator endpoints (sites / cameras / edge-boxes /
        // UniFi integration). Mounted here so `workspace_middleware`
        // (cloud) / `local_context_middleware` (local) gate them, and
        // the URL's `workspace_id` flows into the service layer for
        // resource-ownership checks. The edge-facing `/control/*` tree
        // is mounted separately at the app root (bearer auth resolves
        // workspace_id from the device token).
        .merge(oxy_cameras::routes::workspace_routes::<AppState>(
            agentic_state.db.clone(),
        ))
        // Legacy `/workflows` and `/automations` routes have been retired.
        // Workflow execution + the single-file fetch live under
        // `/agentic-workflows` (IdeOnly), mounted below. The automation LIST,
        // however, is served HERE from the compile boundary (FleetOk) so the
        // customer-nav sidebar renders on a stateless serve replica with no
        // working copy — `/agentic-workflows` is IdeOnly and lives in a crate
        // that can't reach `compiled_reader`.
        .route("/procedures", get(automation::list_automations))
        // Single-automation YAML, also from the boundary, so clicking an automation
        // renders its diagram on a serve node when the ide is down.
        .route("/procedures/{path_b64}", get(automation::get_automation))
        // Canonical "automations" aliases for the procedure list/get (the term
        // Procedure/Workflow was renamed to Automation). Same FleetOk handlers,
        // classified identically in `role_manifest.rs`. `/procedures` is kept
        // for backward compatibility.
        .route("/automations", get(automation::list_automations))
        .route("/automations/{path_b64}", get(automation::get_automation))
        // Same boundary-backed pattern for Airway pipelines (`/agentic-airway`
        // is IdeOnly + in a crate that can't reach `compiled_reader`).
        .route("/airway-pipelines", get(pipeline::list_pipelines))
        .nest("/threads", build_thread_routes())
        .nest("/agents", build_agent_routes())
        .nest("/api-keys", build_api_key_routes())
        .nest("/files", build_file_routes(include_git_features))
        // Compile boundary surface: the user-facing Compile button in
        // the IDE header. Gated by `WorkspaceEditor` and the active
        // branch must match the workspace's default branch.
        .route("/compile", post(compile::enqueue_compile))
        .route("/compile/status", get(compile::compile_status))
        .nest("/databases", build_database_routes())
        .nest("/integrations", build_integration_routes())
        .nest("/secrets", build_secret_routes(app_state))
        .route("/members", get(workspace_members::list_workspace_members))
        .route(
            "/members/{user_id}",
            put(workspace_members::set_workspace_role_override),
        )
        .route(
            "/members/{user_id}",
            delete(workspace_members::remove_workspace_role_override),
        )
        // POST = lock Oxy staff OUT; DELETE = lift the lockdown (default is
        // access-granted). Only a REAL org officer may do either.
        .route(
            "/oxy-access",
            get(workspace_oxy_access::get_oxy_access)
                .post(workspace_oxy_access::lock_oxy_access)
                .delete(workspace_oxy_access::unlock_oxy_access),
        )
        .route("/org-subdomain", get(org_subdomain::get_org_subdomain))
        .route("/custom-apps", get(workspace_custom_apps::list_custom_apps))
        .route("/logo", get(workspace_logo::get_workspace_logo))
        .nest("/apps", build_app_routes())
        .nest("/app-integrations", build_app_integration_routes())
        .nest("/tests", build_test_file_routes())
        // NOT under `/agentic-airway`, which is an `IdeOnly` `{*rest}` wildcard
        // for execution safety. This writes to S3 and reads nothing node-local,
        // so pinning it to the singleton would cost HA for no reason — it stays
        // `FleetOk`, the default for an unlisted route.
        .nest("/source-uploads", build_source_upload_routes())
        .nest("/traces", traces::traces_routes())
        .nest("/metrics", metrics::metrics_routes())
        .nest(
            "/execution-analytics",
            execution_analytics::execution_analytics_routes(),
        )
        .route("/artifacts/{id}", get(artifacts::get_artifact))
        .route("/charts/{file_path}", get(chart::get_chart))
        .route(
            "/exported-charts/{file_name}",
            get(exported_chart::get_exported_chart),
        )
        .route("/logs", get(thread::get_logs))
        .route("/events", get(run::automation_events))
        .route("/events/lookup", get(task::agentic_events))
        .route("/events/sync", get(run::automation_events_sync))
        .route("/blocks", get(run::get_blocks))
        .route(
            "/runs/{source_id}/{run_index}",
            delete(run::cancel_automation_run),
        )
        .route(
            "/builder-availability",
            get(agent::check_builder_availability),
        )
        .route("/sql/{pathb64}", post(data::execute_sql))
        .route("/sql/query", post(data::execute_sql_query))
        // Semantic-layer endpoints the IDE uses. The legacy `/semantic`
        // execute route was retired alongside `oxy-workflow`; execution
        // now flows through the agentic pipeline. Compile + the
        // read-only file handlers stay here so the IDE's topic / view
        // explorer + SQL preview panel keep working without re-introducing
        // a workflow dependency on the request path. Compile reaches
        // into airlayer via `agentic_automation::semantic_bridge`.
        .route(
            "/semantic/topic/{file_path_b64}",
            get(semantic::get_topic_details),
        )
        .route(
            "/semantic/view/{file_path_b64}",
            get(semantic::get_view_details),
        )
        .route("/semantic/preagg-status", get(preagg::get_preagg_status))
        .route("/semantic/preagg-rebuild", post(preagg::rebuild_preagg))
        .route("/semantic/compile", post(semantic::compile_semantic_query))
        .route("/semantic", post(semantic::execute_semantic_query))
        // Metric tree — structure + pure analysis ops over the semantic layer.
        .route("/semantic/metric-tree", get(metric_tree::get_metric_tree))
        .route(
            "/semantic/metric-tree/{measure_id}/sensitivity",
            get(metric_tree::get_sensitivity),
        )
        .route(
            "/semantic/metric-tree/predict",
            post(metric_tree::post_predict),
        )
        .route(
            "/semantic/metric-tree/explain",
            post(metric_tree::post_explain),
        )
        .route(
            "/semantic/metric-tree/opportunity",
            post(metric_tree::post_opportunity),
        )
        .route(
            "/semantic/metric-tree/drill",
            post(metric_tree::post_opportunity_drill),
        )
        .route(
            "/semantic/metric-tree/time-dimensions",
            get(metric_tree::get_time_dimensions),
        )
        .route(
            "/semantic/metric-tree/distribution",
            post(metric_tree::post_distribution),
        )
        .route(
            "/semantic/world-model",
            get(world_model_graph::get_world_model),
        )
        .route(
            "/semantic/world-model/instances",
            get(world_model_graph::get_world_model_instances),
        )
        .route(
            "/semantic/world-model/filter-instances",
            get(world_model_graph::get_world_model_filter_instances),
        )
        .route(
            "/semantic/world-model/filter-counts",
            post(world_model_graph::post_world_model_filter_counts),
        )
        .route(
            "/semantic/world-model/instance-detail",
            get(world_model_graph::get_world_model_instance_detail),
        )
        .route(
            "/semantic/world-model/measure-breakdown",
            get(world_model_graph::get_world_model_measure_breakdown),
        )
        // Anomaly inbox — backed by oxy-metric-monitoring. Nested so the
        // `Extension<Arc<AgenticState>>` layer scopes only to these routes
        // (same pattern as `build_schedule_routes` below).
        //
        // `/semantic/monitors` is merged rather than plainly routed because it
        // also extracts `Arc<AgenticState>` (for the per-segment scan coverage
        // it returns alongside the config) and so needs the same Extension
        // layer. Merging a one-route sub-router keeps the layer scoped to it
        // and preserves the exact path — nesting would also answer on
        // `/semantic/monitors/`, which nothing calls.
        .merge({
            use axum::Extension;
            Router::new()
                .route("/semantic/monitors", get(metric_anomalies::list_monitors))
                .layer(Extension(agentic_state.clone()))
        })
        .nest(
            "/semantic/anomalies",
            build_metric_anomaly_routes(agentic_state.clone()),
        )
        .route(
            "/world-model/events",
            get(world_model::world_model_events_sse),
        )
        .route("/world-model/cameras", get(video::list_cameras))
        .route(
            "/world-model/weather/{layer}/{z}/{x}/{y}",
            get(video::weather_tile),
        )
        .route(
            "/world-model/weather/current",
            post(video::weather_current_batch),
        )
        .route(
            "/world-model/foot-traffic/current",
            post(foot_traffic::foot_traffic_current_batch),
        )
        .route(
            "/world-model/foot-traffic/radar",
            post(foot_traffic::foot_traffic_radar_batch),
        )
        .route(
            "/world-model/competitors",
            post(competitors::get_competitors),
        )
        .route(
            "/results/files/{file_id}",
            get(result_files::get_result_file),
        )
        .route(
            "/results/files/{file_id}",
            delete(result_files::delete_result_file),
        )
        .nest("/analytics", agentic_router(agentic_state.clone()))
        // New agentic-workflow surface — coexists with the legacy `/workflows`
        // routes during migration. Will subsume them in the cleanup task.
        .nest(
            "/agentic-workflows",
            automation_router(agentic_state.clone()),
        )
        // Canonical execution surface alias (Procedures/Workflows -> Automations).
        // Same handlers as `/agentic-workflows`, mirrored in `role_manifest.rs`.
        .nest(
            "/agentic-automations",
            automation_router(agentic_state.clone()),
        )
        .nest("/agentic-airway", airway_router(agentic_state.clone()))
        // Relocated from agentic-http (§12 FU4b): the schedule handlers
        // need WorkspaceAdmin from app/role_guards which agentic-http
        // cannot depend on.
        .nest("/agentic-schedules", build_schedule_routes(agentic_state))
        .nest("/modeling", modeling::build_modeling_routes());

    if include_git_features {
        router = router
            .merge(build_git_routes())
            .nest("/repositories", build_data_repo_routes());
    }

    if include_local_setup {
        router = router
            .route("/setup/empty", post(local_setup::setup_empty))
            .route("/setup/demo", post(local_setup::setup_demo));
    }

    router
}

/// Curated subset of workspace routes for the EXTERNAL API surface
/// (`/external/api/{workspace_id}/...`): just the query + agent + world-model
/// endpoints a standalone app needs, WITHOUT the IDE / file / git / admin /
/// settings surface. Mounted with API-key-only auth + wide-open CORS by
/// [`super::protected::build_external_api_router`]. Reuses the exact same
/// handler functions as `build_workspace_routes`, so behavior is identical —
/// only the auth + CORS wrapper differs.
pub(super) fn build_external_workspace_routes(
    agentic_state: Arc<AgenticState>,
) -> Router<AppState> {
    Router::new()
        .route("/sql/query", post(data::execute_sql_query))
        .route("/semantic", post(semantic::execute_semantic_query))
        .route("/semantic/compile", post(semantic::compile_semantic_query))
        .route(
            "/world-model/events",
            get(world_model::world_model_events_sse),
        )
        .route("/world-model/cameras", get(video::list_cameras))
        // Camera live streaming for standalone apps: registry, WHEP
        // signaling, HLS proxy. Same handlers as the operator tree —
        // only the auth wrapper differs.
        .nest(
            "/world-model/camera-stream",
            oxy_cameras::routes::external_stream_routes::<AppState>(agentic_state.db.clone()),
        )
        // Evidence/compliance clip playback for standalone apps — mints a
        // presigned S3 GET (workspace-prefix checked). Without this the
        // customer app's clip-URL call falls through to the SPA.
        //
        // Path is intentionally `/cameras/clips/...` at the tree root
        // (mirrors the operator route), NOT under `/world-model/camera-stream`
        // like the streaming nest — already-shipped customer apps call this
        // exact path. Don't "tidy" it under the stream prefix; that silently
        // re-breaks playback and would need a coordinated app+server deploy.
        .merge(oxy_cameras::routes::external_clip_routes::<AppState>())
        .route(
            "/world-model/weather/{layer}/{z}/{x}/{y}",
            get(video::weather_tile),
        )
        .route(
            "/world-model/weather/current",
            post(video::weather_current_batch),
        )
        .route(
            "/world-model/foot-traffic/current",
            post(foot_traffic::foot_traffic_current_batch),
        )
        .route(
            "/world-model/foot-traffic/radar",
            post(foot_traffic::foot_traffic_radar_batch),
        )
        .route(
            "/world-model/competitors",
            post(competitors::get_competitors),
        )
        // Thin LLM passthrough for the standalone voice assistant: takes an
        // Anthropic Messages payload (system + messages + optional tools),
        // resolves the workspace Anthropic key server-side, and returns the raw
        // content blocks — so the app never ships a provider key in the browser.
        .route(
            "/world-model/llm/messages",
            post(world_model::proxy_llm_messages),
        )
        // Anomaly monitoring: list, scan, update status, explain — used by
        // standalone apps (e.g. world-model-app) that can't reach the internal
        // /api surface. Mirrors `build_metric_anomaly_routes` exactly, so the
        // TypeScript SDK's `client.anomalies.*` (list/scan/updateStatus/explain)
        // works against `/external/api` as well as `/api`.
        //
        // Nested with an explicit Extension layer because every handler extracts
        // Arc<AgenticState> for db access (same pattern as build_metric_anomaly_routes).
        //
        // `/scan` and `/{id}/explain` are long-running (a scan waits up to 55 s
        // before returning `pending: true`; an uncached explain runs a 20-30 s
        // recursive search) but bounded by the same `timeout_middleware` the rest
        // of this surface carries — no separate budget needed.
        .nest("/semantic/anomalies", {
            use axum::Extension;
            Router::new()
                .route("/", get(metric_anomalies::list_anomalies))
                .route("/scan", post(metric_anomalies::run_scan))
                // Static `/status` (bulk) and `/{id}/status` (single) differ in
                // segment count, so they don't compete for a match.
                .route("/status", post(metric_anomalies::update_status_bulk))
                .route("/{id}/status", post(metric_anomalies::update_status))
                .route("/{id}/explain", post(metric_anomalies::explain_anomaly))
                .layer(Extension(agentic_state.clone()))
        })
        // Agentic analytics: POST /analytics/runs, the SSE events stream,
        // /answer, /cancel, /threads/* — the chat surface external apps drive.
        .nest("/analytics", agentic_router(agentic_state))
}

/// Git-backed workspace routes: local and remote git operations on the
/// workspace itself. Mounted only when `include_git_features` is true —
/// local mode (`ServeMode::Local`) omits the entire set.
fn build_git_routes() -> Router<AppState> {
    Router::new()
        .route("/branches", get(workspaces::get_workspace_branches))
        .route("/branches/{branch_name}", delete(workspaces::delete_branch))
        .route("/switch-branch", post(workspaces::switch_workspace_branch))
        .route("/pull-changes", post(workspaces::pull_changes))
        .route("/fetch", post(workspaces::fetch_changes))
        .route("/push-changes", post(workspaces::push_changes))
        .route("/abort-rebase", post(workspaces::abort_rebase))
        .route("/continue-rebase", post(workspaces::continue_rebase))
        .route(
            "/resolve-conflict-file",
            post(workspaces::resolve_conflict_file),
        )
        .route(
            "/unresolve-conflict-file",
            post(workspaces::unresolve_conflict_file),
        )
        .route(
            "/resolve-conflict-with-content",
            post(workspaces::resolve_conflict_with_content),
        )
        .route("/force-push", post(workspaces::force_push_branch))
        .route("/discard-all", post(workspaces::discard_all_changes))
        .route("/recent-commits", get(workspaces::get_recent_commits))
        .route("/revision-info", get(workspaces::get_revision_info))
        .route("/reset-to-commit", post(workspaces::reset_to_commit))
}

// `build_workflow_routes` and `build_automation_routes` were retired
// alongside `oxy-workflow`. The agentic-pipeline workflow surface mounted
// at `/agentic-workflows` replaces every endpoint they exposed.

fn build_thread_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(thread::get_threads))
        .route("/", post(thread::create_thread))
        .route("/", delete(thread::delete_all_threads))
        .route("/bulk-delete", post(thread::bulk_delete_threads))
        .route("/{id}", get(thread::get_thread))
        .route("/{id}", delete(thread::delete_thread))
        // Thread-bound legacy `/workflow` and `/workflow-sync` routes were
        // retired with `oxy-workflow`. Use the agentic-pipeline workflow
        // surface (`/agentic-workflows/runs`) instead.
        .route("/{id}/messages", get(message::get_messages_by_thread))
        .route("/{id}/stop", post(thread::stop_thread))
}

fn build_agent_routes() -> Router<AppState> {
    Router::new().route("/", get(agent::get_agents))
}

fn build_api_key_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(api_keys::list_api_keys))
        .route("/", post(api_keys::create_api_key))
        .route("/{id}", get(api_keys::get_api_key))
        .route("/{id}", delete(api_keys::delete_api_key))
}

/// Schedule CRUD + run-now (§12 FU4b). Lives in the app crate so the
/// handlers can use `WorkspaceAdmin` from `role_guards` (agentic-http is
/// a lower layer and must not depend on `app`). AgenticState is attached
/// as an Extension here so the handlers can extract it.
fn build_metric_anomaly_routes(agentic_state: Arc<AgenticState>) -> Router<AppState> {
    use axum::Extension;
    Router::new()
        .route("/", get(metric_anomalies::list_anomalies))
        .route("/scan", post(metric_anomalies::run_scan))
        .route("/status", post(metric_anomalies::update_status_bulk))
        .route(
            "/{anomaly_id}/status",
            post(metric_anomalies::update_status),
        )
        .route(
            "/{anomaly_id}/explain",
            post(metric_anomalies::explain_anomaly),
        )
        .layer(Extension(agentic_state))
}

fn build_schedule_routes(agentic_state: Arc<AgenticState>) -> Router<AppState> {
    use axum::Extension;
    Router::new()
        .route("/", get(schedules::list).post(schedules::create))
        .route(
            "/{id}",
            get(schedules::get)
                .patch(schedules::update)
                .delete(schedules::delete),
        )
        .route("/{id}/run-now", post(schedules::run_now))
        .route("/{id}/backfill", post(schedules::backfill))
        .layer(Extension(agentic_state))
}

fn build_file_routes(include_git_features: bool) -> Router<AppState> {
    let mut router = Router::new()
        .route("/", get(file::get_file_tree))
        .route("/{pathb64}", get(file::get_file))
        .route("/{pathb64}", post(file::save_file))
        .route("/{pathb64}/delete-file", delete(file::delete_file))
        .route("/{pathb64}/delete-folder", delete(file::delete_folder))
        .route("/{pathb64}/rename-file", put(file::rename_file))
        .route("/{pathb64}/rename-folder", put(file::rename_folder))
        .route("/{pathb64}/new-file", post(file::create_file))
        .route("/{pathb64}/new-folder", post(file::create_folder));

    if include_git_features {
        router = router
            .route("/diff-summary", get(file::get_diff_summary))
            .route("/{pathb64}/from-git", get(file::get_file_from_git))
            .route("/{pathb64}/revert", post(file::revert_file));
    }

    router
}

fn build_database_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(database::list_databases))
        .route("/", post(database::create_database_config))
        .route("/test-connection", post(database::test_database_connection))
        .route("/inspect", post(database::inspect_database_handler))
        .route("/inspect-schemas", post(database::inspect_schemas_handler))
        .route(
            "/inspect-schema-tables",
            post(database::inspect_schema_tables_handler),
        )
        .route("/sync", post(database::sync_database))
        .route("/build", post(data::build_embeddings))
        .route("/clean", post(database::clean_data))
        .route(
            "/{database_name}/schema",
            get(database::get_database_schema),
        )
}

fn build_data_repo_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(data_repo::list_repositories))
        .route("/", post(data_repo::add_repository))
        .route("/{name}", delete(data_repo::remove_repository))
        .route("/{name}/branch", get(data_repo::get_repo_branch))
        .route("/{name}/branches", get(data_repo::list_repo_branches))
        .route("/{name}/checkout", post(data_repo::checkout_repo_branch))
        .route("/{name}/diff", get(data_repo::get_repo_diff))
        .route("/{name}/commit", post(data_repo::commit_repo))
        .route("/{name}/files", get(data_repo::get_repo_file_tree))
        .route("/github", post(data_repo::add_repo_from_github))
}

fn build_integration_routes() -> Router<AppState> {
    Router::new()
        .route("/looker", get(integration::list_looker_integrations))
        .route("/looker/query", post(integration::execute_looker_query))
        .route("/looker/query/sql", post(integration::compile_looker_query))
        .route(
            "/quickbooks/authorize",
            post(crate::integrations::quickbooks::oauth::authorize::authorize),
        )
}

// MIXED role builder. IdeOnly (must reach the ide singleton): `/source` + `/file`
// read the working copy / local state dir; `/{pathb64}` + `/run` + `/result`
// EXECUTE the inline workflow over the local DuckDB connector; `/publish`,
// `/unpublish`, `/save-from-run/{run_id}` WRITE the working copy. FleetOk
// (compile boundary + S3 + Postgres): `/` (list), `/displays`, `/data-cached`,
// `/charts/...`. Any FS-reading/-writing route added here MUST be classified by
// hand in `server/role_manifest.rs` — the `fully_fs_builder_routes_classify_ide_only`
// drift test only covers FULLY-fs builders, so it can't catch a miss here.
// Skipping it is how `/apps/source` shipped FleetOk and 404'd on the serve fleet;
// the `every_app_sub_route_is_classified` test now backstops THIS builder so a
// new sub-route fails CI unless it is IdeOnly or explicitly acknowledged FleetOk.
// See `.claude/skills/oxy-route-classification/SKILL.md`.
fn build_app_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(app::list_apps))
        .route("/{pathb64}", get(app::get_app_data))
        .route("/{pathb64}/run", post(app::run_app))
        .route("/{pathb64}/result", post(app::get_app_result))
        .route("/{pathb64}/displays", get(app::get_displays))
        // Read-only cached data (boundary def + disk/S3 cache, no execution), so
        // a serve replica shows a dashboard's last data when the ide is down.
        .route("/{pathb64}/data-cached", get(app::get_app_data_cached))
        .route("/{pathb64}/charts/{chart_path}", get(app::get_chart_image))
        .route("/{pathb64}/publish", post(app::publish_app))
        .route("/{pathb64}/unpublish", post(app::unpublish_app))
        .route("/file/{pathb64}", get(app::get_data))
        .route("/source/{pathb64}", get(app::get_source_file))
        .route("/save-from-run/{run_id}", post(app::save_app_builder_run))
}

/// Report uploads for file-based sources, into the shared landing zone.
///
/// Deliberately its own nest rather than a child of `/agentic-airway`: that
/// prefix is classified `IdeOnly` for every method so a live run stays on the
/// instance holding the working copy, and this handler touches no working copy.
fn build_source_upload_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/reports",
            post(crate::server::api::source_upload::upload_report),
        )
        // Without this, axum 0.8's 2 MiB default governs and the handler's own
        // ceiling is unreachable — a larger report fails inside `field.bytes()`
        // as a 400, never the 413 the handler writes. At `Router` level rather
        // than on the `MethodRouter` for the same reason `build_onboarding_routes`
        // gives: the latter can interact unexpectedly with outer CORS preflight
        // handling on axum 0.8.
        // `MAX_REPORT_BYTES` plus slack, because this bounds the whole
        // multipart body while the constant bounds ONE FILE: boundaries, field
        // names, `pipeline_ref`, `workflow_id` and the period all ride along.
        // Set equal, a file a few hundred bytes under the ceiling passed the
        // client-side check and the handler's own check and still died here —
        // answered by this layer's terse 400, never the handler's 413 that
        // names the size. The handler stays the authority on the file itself.
        .layer(axum::extract::DefaultBodyLimit::max(
            crate::server::api::source_upload::MAX_REPORT_BYTES + 64 * 1024,
        ))
}

fn build_test_file_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(test_file::list_test_files))
        .route(
            "/project-runs",
            get(test_project_run::list_project_runs).post(test_project_run::create_project_run),
        )
        .route(
            "/project-runs/{project_run_id}",
            delete(test_project_run::delete_project_run),
        )
        .route("/{pathb64}", get(test_file::get_test_file))
        .route(
            "/{pathb64}/cases/{case_index}",
            post(test_file::run_test_case),
        )
        .route(
            "/{pathb64}/runs",
            get(test_run::list_runs).post(test_run::create_run),
        )
        .route(
            "/{pathb64}/runs/{run_index}",
            get(test_run::get_run).delete(test_run::delete_run),
        )
        .route(
            "/{pathb64}/runs/{run_index}/human-verdicts",
            get(test_run::list_human_verdicts),
        )
        .route(
            "/{pathb64}/runs/{run_index}/cases/{case_index}/human-verdict",
            put(test_run::set_human_verdict),
        )
}

/// World-model "Apps" configuration routes — Toast / OpenWeatherMap /
/// BestTime integration entries surfaced in the Workspace Settings →
/// Apps tab. Separate from `build_app_routes` (data apps).
fn build_app_integration_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(apps::list_apps).post(apps::upsert_app))
        .route("/{kind}", delete(apps::delete_app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_pipeline::platform::ThreadOwnerLookup;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use sea_orm::DatabaseConnection;
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    use oxy_app_core::serve_mode::ServeMode;

    fn test_app_state() -> AppState {
        AppState {
            enterprise: false,
            internal: false,
            mode: ServeMode::Local,
            observability: None,
            startup_cwd: std::path::PathBuf::new(),
            preagg_cache: None,
            preagg_renewal_threshold_secs: None,
            agentic_state: None,
            semantic_layer_cache: crate::server::router::workspace_cache::new_semantic_layer_cache(
            ),
            semantic_engine_cache:
                crate::server::router::workspace_cache::new_semantic_engine_cache(),
        }
    }

    struct StubThreadOwner;

    #[async_trait]
    impl ThreadOwnerLookup for StubThreadOwner {
        async fn thread_owner(
            &self,
            _thread_id: uuid::Uuid,
        ) -> Result<Option<Option<uuid::Uuid>>, String> {
            Ok(None)
        }
    }

    fn test_agentic_state() -> Arc<AgenticState> {
        Arc::new(AgenticState::new(
            CancellationToken::new(),
            DatabaseConnection::default(),
            Arc::new(StubThreadOwner),
        ))
    }

    async fn status_for(router: axum::Router, method: &str, path: &str) -> StatusCode {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        router.oneshot(req).await.unwrap().status()
    }

    /// Every git-shaped route must 404 when `include_git_features: false`.
    /// This is the invariant that guarantees local mode cannot reach a
    /// git handler regardless of how the caller is wired.
    #[tokio::test]
    async fn git_routes_absent_when_flag_disabled() {
        let state = test_app_state();
        let router = build_workspace_routes(state.clone(), test_agentic_state(), false, false)
            .with_state(state);

        let cases: &[(&str, &str)] = &[
            ("GET", "/branches"),
            ("DELETE", "/branches/foo"),
            ("POST", "/switch-branch"),
            ("POST", "/pull-changes"),
            ("POST", "/fetch"),
            ("POST", "/push-changes"),
            ("POST", "/force-push"),
            ("POST", "/discard-all"),
            ("POST", "/abort-rebase"),
            ("POST", "/continue-rebase"),
            ("POST", "/resolve-conflict-file"),
            ("POST", "/unresolve-conflict-file"),
            ("POST", "/resolve-conflict-with-content"),
            ("GET", "/recent-commits"),
            ("GET", "/revision-info"),
            ("POST", "/reset-to-commit"),
            ("GET", "/repositories"),
            ("GET", "/files/Zm9vLnltbA==/from-git"),
            ("POST", "/files/Zm9vLnltbA==/revert"),
        ];

        for (method, path) in cases {
            let router = router.clone();
            let status = status_for(router, method, path).await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "{method} {path} must 404 when git is disabled (got {status})"
            );
        }
    }

    /// Sanity check: when the flag is on, the same routes are mounted.
    /// We only assert `!= 404` — the actual status depends on handler
    /// behavior under a stripped harness, which is out of scope.
    #[tokio::test]
    async fn git_routes_present_when_flag_enabled() {
        let state = test_app_state();
        let router = build_workspace_routes(state.clone(), test_agentic_state(), true, false)
            .with_state(state);

        let status = status_for(router, "GET", "/branches").await;
        assert_ne!(
            status,
            StatusCode::NOT_FOUND,
            "/branches must be mounted when git is enabled"
        );
    }

    /// Setup endpoints are mounted when `include_local_setup: true` (local mode).
    #[tokio::test]
    async fn setup_routes_present_when_include_local_setup_true() {
        let state = test_app_state();
        let router = build_workspace_routes(state.clone(), test_agentic_state(), false, true)
            .with_state(state);

        for path in ["/setup/empty", "/setup/demo"] {
            let status = status_for(router.clone(), "POST", path).await;
            assert_ne!(
                status,
                StatusCode::NOT_FOUND,
                "{} must be mounted when include_local_setup=true (got {})",
                path,
                status
            );
        }
    }

    /// Setup endpoints are absent when `include_local_setup: false` (cloud mode).
    #[tokio::test]
    async fn setup_routes_absent_when_include_local_setup_false() {
        let state = test_app_state();
        let router = build_workspace_routes(state.clone(), test_agentic_state(), true, false)
            .with_state(state);

        for path in ["/setup/empty", "/setup/demo"] {
            let status = status_for(router.clone(), "POST", path).await;
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "{} must 404 when include_local_setup=false (got {})",
                path,
                status
            );
        }
    }

    /// The camera fleet operator surface (sites / cameras / edge boxes /
    /// UniFi integration / preview proxies) MUST be merged into
    /// `build_workspace_routes`. That's the only thing keeping it
    /// behind `workspace_middleware`/`local_context_middleware` +
    /// `auth_middleware` — see the cross-workspace write incident
    /// closed by commit `21735f047`. If someone re-merges these routes
    /// at the app root (or under an un-authed mount), this test fails
    /// fast instead of waiting for a real cross-workspace breach.
    #[tokio::test]
    async fn camera_operator_routes_mounted_inside_workspace_tree() {
        let state = test_app_state();
        let router = build_workspace_routes(state.clone(), test_agentic_state(), false, false)
            .with_state(state);

        // One representative from each operator sub-area. Wildcard
        // routes (HLS / recording) aren't worth poking here — the
        // mount-point check on the non-wildcard variants is enough.
        let cases: &[(&str, &str)] = &[
            ("GET", "/cameras/sites"),
            ("GET", "/cameras/edge-boxes"),
            ("GET", "/cameras"),
            ("POST", "/integrations/unifi/preview"),
        ];
        for (method, path) in cases {
            let status = status_for(router.clone(), method, path).await;
            assert_ne!(
                status,
                StatusCode::NOT_FOUND,
                "{method} {path} must be mounted inside build_workspace_routes (got {status}); \
                 mounting it elsewhere bypasses workspace_middleware + auth_middleware"
            );
        }
    }

    /// The external API surface must expose the FULL anomaly inbox — list,
    /// scan, status, explain — not just the read half. Standalone custom apps
    /// can't reach `/api`, so a missing verb here means the TypeScript SDK's
    /// `client.anomalies.scan()` / `.explain()` 404 against `/external/api`
    /// while the same calls work in the IDE.
    #[tokio::test]
    async fn external_surface_exposes_full_anomaly_inbox() {
        let state = test_app_state();
        let router = build_external_workspace_routes(test_agentic_state()).with_state(state);

        let anomaly_id = "3f2504e0-4f89-11d3-9a0c-0305e82c3301";

        // `POST /{id}/status` was already mounted externally BEFORE this change
        // and is known to work in production, so whatever a bare test router
        // returns for it ("routed fine, failed later on absent workspace
        // context") is the reference for a correctly-resolving route.
        let baseline = status_for(
            router.clone(),
            "POST",
            &format!("/semantic/anomalies/{anomaly_id}/status"),
        )
        .await;
        assert!(
            !baseline.is_success() && baseline != StatusCode::NOT_FOUND,
            "baseline POST /semantic/anomalies/{{id}}/status returned {baseline}; this test \
             assumes a bare router routes the request and then fails on missing context"
        );

        let cases: &[(&str, String)] = &[
            ("GET", "/semantic/anomalies".to_string()),
            ("POST", "/semantic/anomalies/scan".to_string()),
            // Bulk status — `client.anomalies.updateStatusBulk()`. One segment,
            // so it must not be swallowed by the two-segment `/{id}/status`.
            ("POST", "/semantic/anomalies/status".to_string()),
            ("POST", format!("/semantic/anomalies/{anomaly_id}/status")),
            ("POST", format!("/semantic/anomalies/{anomaly_id}/explain")),
        ];
        for (method, path) in cases {
            let status = status_for(router.clone(), method, path).await;
            assert_ne!(
                status,
                StatusCode::NOT_FOUND,
                "{method} {path} must be mounted on the external API surface (got {status}); \
                 custom apps reach anomalies only through /external/api"
            );
            assert_ne!(
                status,
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} {path} is mounted on the external API surface under a DIFFERENT \
                 verb (got {status}); the SDK calls it with {method}"
            );
            // Equality with the baseline is what rules out a *silently broken*
            // mount: a route that resolves but whose role extractor stopped
            // resolving would diverge here (401/403) while still passing the
            // 404/405 checks above.
            assert_eq!(
                status, baseline,
                "{method} {path} returned {status} but the already-proven \
                 POST /semantic/anomalies/{{id}}/status returned {baseline}. These handlers all \
                 take the same EffectiveWorkspaceRole extractor, so a divergence means this \
                 route resolves differently — check the extractor stack, not just the mount."
            );
        }
    }

    /// Drift guard: the external anomaly mount mirrors
    /// `build_metric_anomaly_routes` verb-for-verb. Adding a route to the
    /// internal builder without mirroring it here silently leaves custom apps
    /// a version behind, so compare the two route sets from the source.
    #[test]
    fn external_anomaly_routes_mirror_internal_builder() {
        let src = include_str!("workspace.rs");

        // Route paths registered inside a named fn/nest body, normalised so
        // `{anomaly_id}` and `{id}` compare equal (axum extracts positionally).
        fn routes_in(body: &str) -> std::collections::BTreeSet<String> {
            let mut out = std::collections::BTreeSet::new();
            let mut rest = body;
            while let Some(idx) = rest.find(".route(") {
                rest = &rest[idx + ".route(".len()..];
                let Some(open) = rest.find('"') else { break };
                let after = &rest[open + 1..];
                let Some(close) = after.find('"') else { break };
                let path = &after[..close];
                // Normalise the param NAME away; only the shape matters.
                let normalised: Vec<&str> = path
                    .split('/')
                    .map(|seg| if seg.starts_with('{') { "{p}" } else { seg })
                    .collect();
                out.insert(normalised.join("/"));
                rest = &after[close..];
            }
            out
        }

        fn body_after<'a>(src: &'a str, marker: &str) -> &'a str {
            let start = src
                .find(marker)
                .unwrap_or_else(|| panic!("{marker} present"));
            let body = &src[start..];
            let end = body.find("\n}\n").unwrap_or(body.len());
            &body[..end]
        }

        let internal = routes_in(body_after(src, "fn build_metric_anomaly_routes"));

        // The external mount is an inline `.nest("/semantic/anomalies", { .. })`
        // block; slice from the nest to the end of the builder.
        let external_builder = body_after(src, "fn build_external_workspace_routes");
        let nest_start = external_builder
            .find(r#".nest("/semantic/anomalies""#)
            .expect("external anomalies nest present");
        let nest_body = &external_builder[nest_start..];
        let nest_end = nest_body.find("})").unwrap_or(nest_body.len());
        let external = routes_in(&nest_body[..nest_end]);

        assert!(
            !internal.is_empty() && internal.len() >= 4,
            "parser found only {} internal anomaly routes — the builder shape changed",
            internal.len()
        );
        assert_eq!(
            internal,
            external,
            "build_metric_anomaly_routes and the external /semantic/anomalies nest have \
             drifted. Every anomaly route must exist on BOTH surfaces — custom apps reach \
             anomalies only through /external/api. Internal-only: {:?}; external-only: {:?}",
            internal.difference(&external).collect::<Vec<_>>(),
            external.difference(&internal).collect::<Vec<_>>(),
        );
    }
}
