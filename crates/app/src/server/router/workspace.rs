//! The per-workspace route tree. Mounted under `/{workspace_id}` in both
//! cloud and local modes (local always uses the nil UUID).
//!
//! This module owns the tree shape plus all per-resource sub-builders
//! (workflows, threads, agents, files, etc.). Secrets live in their own
//! module because they ship with an admin-gated middleware.

use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post, put};

use agentic_http::{AgenticState, router as agentic_router};

use crate::api::{
    agent, api_keys, app, artifacts, chart, data, data_repo, database, execution_analytics,
    exported_chart, file, integration, local_setup, message, metrics, onboarding, result_files,
    run, semantic, task, test_file, test_project_run, test_run, thread, traces, workflow,
    workspace_members, workspaces,
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
        .nest("/workflows", build_workflow_routes())
        .nest("/automations", build_automation_routes())
        .nest("/threads", build_thread_routes())
        .nest("/agents", build_agent_routes())
        .nest("/api-keys", build_api_key_routes())
        .nest("/files", build_file_routes(include_git_features))
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
        .nest("/tests", build_test_file_routes())
        .nest("/apps", build_app_routes())
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
        .route("/events", get(run::workflow_events))
        .route("/events/lookup", get(task::agentic_events))
        .route("/events/sync", get(run::workflow_events_sync))
        .route("/blocks", get(run::get_blocks))
        .route(
            "/runs/{source_id}/{run_index}",
            delete(run::cancel_workflow_run),
        )
        .route(
            "/builder-availability",
            get(agent::check_builder_availability),
        )
        .route(
            "/onboarding-readiness",
            get(onboarding::onboarding_readiness),
        )
        .route("/sql/{pathb64}", post(data::execute_sql))
        .route("/sql/query", post(data::execute_sql_query))
        .route("/semantic", post(semantic::execute_semantic_query))
        .route("/semantic/compile", post(semantic::compile_semantic_query))
        .route(
            "/semantic/topic/{file_path_b64}",
            get(semantic::get_topic_details),
        )
        .route(
            "/semantic/view/{file_path_b64}",
            get(semantic::get_view_details),
        )
        .route(
            "/results/files/{file_id}",
            get(result_files::get_result_file),
        )
        .route(
            "/results/files/{file_id}",
            delete(result_files::delete_result_file),
        )
        .nest("/analytics", agentic_router(agentic_state));

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

/// Git-backed workspace routes: local and remote git operations on the
/// workspace itself. Mounted only when `include_git_features` is true —
/// local mode (`ServeMode::Local`) omits the entire set.
fn build_git_routes() -> Router<AppState> {
    Router::new()
        .route("/branches", get(workspaces::get_workspace_branches))
        .route("/branches/{branch_name}", delete(workspaces::delete_branch))
        .route("/switch-branch", post(workspaces::switch_workspace_branch))
        .route("/pull-changes", post(workspaces::pull_changes))
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
        .route("/recent-commits", get(workspaces::get_recent_commits))
        .route("/revision-info", get(workspaces::get_revision_info))
        .route("/reset-to-commit", post(workspaces::reset_to_commit))
}

fn build_workflow_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(workflow::list))
        .route("/from-query", post(workflow::create_from_query))
        .route("/runs/bulk-delete", post(run::bulk_delete_workflow_runs))
        .route("/{pathb64}", get(workflow::get))
        .route("/{pathb64}/run", post(workflow::run_workflow))
        .route("/{pathb64}/run-sync", post(workflow::run_workflow_sync))
        .route("/{pathb64}/logs", get(workflow::get_logs))
        .route("/{pathb64}/runs", get(run::get_workflow_runs))
        .route("/{pathb64}/runs", post(run::create_workflow_run))
        .route(
            "/{pathb64}/runs/{run_id}",
            get(workflow::get_workflow_run).delete(run::delete_workflow_run),
        )
}

fn build_automation_routes() -> Router<AppState> {
    Router::new().route("/save", post(workflow::save_automation))
}

fn build_thread_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(thread::get_threads))
        .route("/", post(thread::create_thread))
        .route("/", delete(thread::delete_all_threads))
        .route("/bulk-delete", post(thread::bulk_delete_threads))
        .route("/{id}", get(thread::get_thread))
        .route("/{id}", delete(thread::delete_thread))
        .route("/{id}/task", post(task::ask_task))
        .route("/{id}/agentic", post(task::ask_agentic))
        .route("/{id}/workflow", post(workflow::run_workflow_thread))
        .route(
            "/{id}/workflow-sync",
            post(workflow::run_workflow_thread_sync),
        )
        .route("/{id}/messages", get(message::get_messages_by_thread))
        .route("/{id}/agent", post(agent::ask_agent))
        .route("/{id}/stop", post(thread::stop_thread))
}

fn build_agent_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(agent::get_agents))
        .route("/{pathb64}", get(agent::get_agent))
        .route("/{pathb64}/ask", post(agent::ask_agent_preview))
        .route("/{pathb64}/ask-sync", post(agent::ask_agent_sync))
        .route("/{pathb64}/tests/{test_index}", post(agent::run_test))
}

fn build_api_key_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(api_keys::list_api_keys))
        .route("/", post(api_keys::create_api_key))
        .route("/{id}", get(api_keys::get_api_key))
        .route("/{id}", delete(api_keys::delete_api_key))
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
        .route("/sync", post(database::sync_database))
        .route("/build", post(data::build_embeddings))
        .route("/clean", post(database::clean_data))
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

fn build_app_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(app::list_apps))
        .route("/{pathb64}", get(app::get_app_data))
        .route("/{pathb64}/run", post(app::run_app))
        .route("/{pathb64}/result", post(app::get_app_result))
        .route("/{pathb64}/displays", get(app::get_displays))
        .route("/{pathb64}/charts/{chart_path}", get(app::get_chart_image))
        .route("/file/{pathb64}", get(app::get_data))
        .route("/source/{pathb64}", get(app::get_source_file))
        .route("/save-from-run/{run_id}", post(app::save_app_builder_run))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::server::serve_mode::ServeMode;

    fn test_app_state() -> AppState {
        AppState {
            enterprise: false,
            internal: false,
            mode: ServeMode::Local,
            observability: None,
            startup_cwd: std::path::PathBuf::new(),
        }
    }

    fn test_agentic_state() -> Arc<AgenticState> {
        Arc::new(AgenticState::new())
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
            ("POST", "/push-changes"),
            ("POST", "/force-push"),
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
}
