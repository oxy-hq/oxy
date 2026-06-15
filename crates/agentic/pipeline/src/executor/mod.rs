//! [`TaskExecutor`] implementation for the agentic pipeline layer.
//!
//! This is the composition point where domain knowledge (analytics, builder,
//! workflow) meets the generic coordinator-worker infrastructure. The runtime
//! only sees [`TaskExecutor`]; this crate knows how to start the right pipeline
//! for each [`TaskSpec`] variant.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentic_analytics::SchemaCatalog;
use agentic_builder::{BuilderAppRunner, BuilderTestRunner};
use agentic_core::delegation::{TaskAssignment, TaskSpec};
use agentic_runtime::worker::{ExecutingTask, TaskExecutor};
use async_trait::async_trait;
use sea_orm::DatabaseConnection;

use crate::platform::{BuilderBridges, PlatformContext};
use crate::{PipelineBuilder, ThinkingMode};

// ── PipelineTaskExecutor ─────────────────────────────────────────────────────

/// Knows how to start analytics/builder pipelines and workflow executions.
///
/// Injected into the [`Worker`](agentic_runtime::worker::Worker) by the
/// HTTP/CLI layer.
pub struct PipelineTaskExecutor {
    pub platform: Arc<dyn PlatformContext>,
    /// Required for builder delegation; `None` is fine for analytics-only runs.
    pub builder_bridges: Option<BuilderBridges>,
    pub schema_cache: Option<Arc<Mutex<HashMap<String, SchemaCatalog>>>>,
    pub builder_test_runner: Option<Arc<dyn BuilderTestRunner>>,
    pub builder_app_runner: Option<Arc<dyn BuilderAppRunner>>,
    pub db: DatabaseConnection,
    /// Runtime state for registering answer channels (needed by workflow
    /// orchestrator tasks so the coordinator can resume them via answer channel
    /// instead of TaskSpec::Resume).
    pub state: Option<Arc<agentic_runtime::state::RuntimeState>>,
}

#[async_trait]
impl TaskExecutor for PipelineTaskExecutor {
    async fn execute(&self, assignment: TaskAssignment) -> Result<ExecutingTask, String> {
        // When this task has a parent, it's a delegation child — the
        // coordinator already created the run row, so pass the run_id
        // through to skip the duplicate insert.
        let is_child = assignment.parent_task_id.is_some();
        match &assignment.spec {
            TaskSpec::Agent {
                agent_id,
                question,
                extra,
            } => {
                // Top-level scheduled agent runs pre-seed the run row in
                // `start_agent_run` (mirrors the workflow / airway
                // pattern). Detect that case by checking the DB and pass
                // `existing_run_id` so the analytics builder doesn't try
                // to insert a duplicate row.
                let pre_seeded = !is_child
                    && agentic_runtime::crud::get_run(&self.db, &assignment.run_id)
                        .await
                        .map_err(|e| format!("failed to load run: {e}"))?
                        .is_some();
                let existing_run_id = if is_child || pre_seeded {
                    Some(assignment.run_id.clone())
                } else {
                    None
                };
                self.execute_agent(agent_id, question, existing_run_id, extra.as_ref())
                    .await
            }

            TaskSpec::Workflow {
                workflow_ref,
                variables,
                retry_from_run_id,
                cache_enabled,
                body,
                initial_render_context,
            } => {
                self.execute_workflow(
                    &assignment.run_id,
                    workflow_ref,
                    variables.clone(),
                    retry_from_run_id.clone(),
                    *cache_enabled,
                    body.clone(),
                    initial_render_context.clone(),
                )
                .await
            }

            TaskSpec::Resume {
                run_id,
                resume_data,
                answer,
            } => {
                self.execute_resume(run_id, resume_data.clone(), answer.clone())
                    .await
            }

            TaskSpec::WorkflowStep {
                step_config,
                render_context,
                workflow_context,
            } => {
                self.execute_workflow_step(
                    step_config.clone(),
                    render_context.clone(),
                    workflow_context.clone(),
                )
                .await
            }

            TaskSpec::WorkflowDecision {
                run_id,
                pending_child_answer,
            } => {
                self.execute_workflow_decision(run_id, pending_child_answer.clone())
                    .await
            }

            TaskSpec::Custom { kind, .. } => Err(format!(
                "PipelineTaskExecutor does not handle Custom tasks (kind: {kind})"
            )),

            TaskSpec::Airway {
                pipeline_ref,
                variables,
                resources,
            } => {
                self.execute_airway(pipeline_ref, variables.as_ref(), resources)
                    .await
            }

            TaskSpec::Compile {
                workspace_id,
                git_sha,
                branch,
                promote,
                kind,
                owner_user_id,
            } => {
                self.execute_compile(
                    *workspace_id,
                    git_sha.clone(),
                    branch.clone(),
                    *promote,
                    kind.as_deref(),
                    *owner_user_id,
                )
                .await
            }
        }
    }

    async fn resume_from_state(
        &self,
        run: &agentic_runtime::entity::run::Model,
        suspend_data: Option<agentic_core::human_input::SuspendedRunData>,
    ) -> Result<ExecutingTask, String> {
        let source_type = run.source_type.as_deref().unwrap_or("analytics");

        // Temporal-style workflow runs: if `agentic_workflow_state` exists for
        // this run, resume by enqueuing a WorkflowDecision (stateless path).
        if source_type == "workflow" {
            match agentic_workflow::extension::load_workflow_state(&self.db, &run.id).await {
                Ok(Some(_)) => {
                    return self.execute_workflow_decision(&run.id, None).await;
                }
                Ok(None) => {
                    // No durable state (run started before the Temporal refactor).
                    // Fall through to legacy resume path below.
                }
                Err(e) => {
                    tracing::warn!(
                        target: "pipeline",
                        run_id = %run.id,
                        error = %e,
                        "failed to check workflow state; falling back to legacy resume"
                    );
                }
            }
        }

        // Also check task_metadata for workflow orchestrator state.
        if let Some(ref meta) = run.task_metadata
            && meta.get("original_spec").is_some()
            && let Some(spec) = meta.get("original_spec")
            && spec.get("type").and_then(|t| t.as_str()) == Some("workflow")
        {
            // This was a workflow child — try to re-run the workflow.
            if let Some(workflow_ref) = spec.get("workflow_ref").and_then(|v| v.as_str()) {
                return self
                    .execute_workflow(&run.id, workflow_ref, None, None, false, None, None)
                    .await;
            }
        }

        match source_type {
            "workflow" | "workflow_step" => {
                // Workflow tasks without orchestrator checkpoint.
                if let Some(data) = suspend_data {
                    self.execute_resume(&run.id, data, String::new()).await
                } else {
                    Err(format!(
                        "cannot resume workflow run {}: no saved state",
                        run.id
                    ))
                }
            }
            _ => {
                // Analytics/builder: resume from checkpoint if available.
                if let Some(data) = suspend_data {
                    self.execute_resume(&run.id, data, String::new()).await
                } else {
                    // No checkpoint — run hadn't reached a suspension point.
                    // Cannot resume; user needs to resubmit the question.
                    Err(format!(
                        "run {} (type={source_type}) has no checkpoint — resubmit the question",
                        run.id
                    ))
                }
            }
        }
    }
}

/// The well-known agent ID that routes to the builder domain instead of
/// analytics.  Used by analytics → builder delegation.
pub const BUILDER_AGENT_ID: &str = "__builder__";

/// Returns `true` when `agent_id` should be routed to the builder domain
/// rather than the analytics domain.
fn is_builder_agent(agent_id: &str) -> bool {
    agent_id == BUILDER_AGENT_ID
}

impl PipelineTaskExecutor {
    async fn execute_agent(
        &self,
        agent_id: &str,
        question: &str,
        existing_run_id: Option<String>,
        extra: Option<&serde_json::Value>,
    ) -> Result<ExecutingTask, String> {
        let mut pb =
            PipelineBuilder::new(self.platform.clone()).workspace_id(self.platform.workspace_id());
        if let Some(bridges) = self.builder_bridges.clone() {
            pb = pb.with_builder_bridges(bridges);
        }
        let mut builder = if is_builder_agent(agent_id) {
            pb.builder(None)
        } else {
            pb.analytics(agent_id)
        }
        .question(question)
        .thinking_mode(ThinkingMode::Auto);

        // `extra` is an envelope packed by `agentic-workflow` carrying
        // domain-opaque per-agent knobs. Today it carries the
        // analytics SQL-gen mode flag (`output_mode == "sql"`); the
        // builder path ignores it.
        if !is_builder_agent(agent_id)
            && let Some(extra_value) = extra
            && let Some(mode) = extra_value.get("output_mode").and_then(|v| v.as_str())
            && mode == "sql"
        {
            builder = builder.analytics_sql_mode();
        }

        // For delegation children, use the coordinator-assigned run_id
        // and skip the duplicate DB insert.
        if let Some(run_id) = existing_run_id.clone() {
            builder = builder.existing_run(run_id);
        }

        // Gate HITL when an agent runs as a delegation child
        // (existing_run_id is set → the coordinator created this
        // task). The parent workflow's SSE stream doesn't yet
        // surface child-run events, so a nested suspension leaves
        // the workflow UI looking hung. The provider differs by
        // agent type because the expected answer shape differs:
        //
        //   - Builder: `Accept` clears file-change confirmations.
        //   - Analytics: a directive string ("proceed with best
        //     interpretation") is more useful than a literal
        //     `Accept` as the answer to an `ask_user` call.
        //
        // Lift this gate once the workflow run page streams nested
        // analytics events (see the streaming-children audit).
        if existing_run_id.is_some() {
            let provider: agentic_core::human_input::HumanInputHandle =
                if is_builder_agent(agent_id) {
                    std::sync::Arc::new(agentic_core::human_input::AutoAcceptInputProvider)
                } else {
                    std::sync::Arc::new(agentic_core::human_input::NoClarificationProvider)
                };
            builder = builder.human_input(provider);
        }

        if let Some(cache) = &self.schema_cache {
            builder = builder.schema_cache(cache.clone());
        }
        if let Some(runner) = &self.builder_test_runner {
            builder = builder.test_runner(runner.clone());
        }
        if let Some(runner) = &self.builder_app_runner {
            builder = builder.app_runner(runner.clone());
        }

        let started = builder
            .start(&self.db)
            .await
            .map_err(|e| format!("failed to start agent pipeline: {e}"))?;

        let (task, _bridge) = started.into_executing_task();
        Ok(task)
    }

    async fn execute_resume(
        &self,
        run_id: &str,
        resume_data: agentic_core::human_input::SuspendedRunData,
        answer: String,
    ) -> Result<ExecutingTask, String> {
        // Load run from DB to get source_type, agent_id, model, thread_id.
        let run = agentic_runtime::crud::get_run(&self.db, run_id)
            .await
            .map_err(|e| format!("failed to load run: {e}"))?
            .ok_or_else(|| format!("run {run_id} not found"))?;

        let source_type = run.source_type.as_deref().unwrap_or("analytics");
        // Resolve agent_id with a fallback. Top-level runs land it on
        // `metadata.agent_id` (via `start_analytics`'s insert path).
        // Delegation children are inserted by `insert_child_run` with
        // `metadata = None`, but their `task_metadata.original_spec`
        // carries the full `TaskSpec::Agent` — including `agent_id`.
        // Without this fallback, resuming a workflow → analytics
        // chain would feed `""` into `start_analytics`, which then
        // calls `base_dir.join("")` (returns the workspace root, a
        // directory) and fails with `IO error: Is a directory`.
        let agent_id = run
            .metadata
            .as_ref()
            .and_then(|m| m.get("agent_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                run.task_metadata
                    .as_ref()
                    .and_then(|m| m.get("original_spec"))
                    .and_then(|s| s.get("agent_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        let model = run
            .metadata
            .as_ref()
            .and_then(|m| m.get("model"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Resume path: `existing_run_id` will be set below, so the
        // builder skips the DB insert — workspace_id is not consulted at
        // INSERT time. We still set it for trace coherence and so any
        // future cold-resume insert lands on the right row.
        let mut builder = PipelineBuilder::new(self.platform.clone())
            .workspace_id(self.platform.workspace_id())
            .question(&run.question);
        if let Some(bridges) = self.builder_bridges.clone() {
            builder = builder.with_builder_bridges(bridges);
        }

        if let Some(cache) = &self.schema_cache {
            builder = builder.schema_cache(cache.clone());
        }
        if let Some(runner) = &self.builder_test_runner {
            builder = builder.test_runner(runner.clone());
        }
        if let Some(runner) = &self.builder_app_runner {
            builder = builder.app_runner(runner.clone());
        }
        if let Some(tid) = run.thread_id {
            builder = builder.thread(tid);
        }

        let started = builder
            .resume(
                &self.db,
                run_id,
                source_type,
                &agent_id,
                model,
                resume_data,
                answer,
            )
            .await
            .map_err(|e| format!("failed to resume pipeline: {e}"))?;

        let (task, _bridge) = started.into_executing_task();
        Ok(task)
    }

    /// Dispatch a `TaskSpec::Airway`. Loads `.airway.yml` from the
    /// workspace, parses it into an [`AirwayPipelineSpec`], and hands
    /// off to `AirwayWorker` which spawns the engine run and returns
    /// the runtime-shape channel pair.
    ///
    /// `variables` is captured but not yet applied — YAML templating
    /// lands in a follow-up alongside the CLI/HTTP entry points.
    async fn execute_airway(
        &self,
        pipeline_ref: &str,
        variables: Option<&serde_json::Value>,
        resources: &[String],
    ) -> Result<ExecutingTask, String> {
        // Defence-in-depth: `start_airway_run` already contained the
        // ref at submit time, but re-validate at queue-claim too (the
        // queued spec is caller-influenced). `workspace_path` resolves
        // through `PlatformContext`'s `WorkspaceContext` supertrait.
        let path =
            crate::pipeline_ref::resolve_pipeline_ref(self.platform.workspace_path(), pipeline_ref)
                .map_err(|e| format!("airway: {e}"))?;
        let yaml = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("airway: read pipeline_ref `{pipeline_ref}`: {e}"))?;
        // Render with the same `variables` that `start_airway_run`
        // validated against, so the worker's document matches what the
        // submitter saw.
        let mut spec = agentic_airway::AirwayPipelineSpec::from_yaml_with_vars(&yaml, variables)
            .map_err(|e| format!("airway: parse `{pipeline_ref}`: {e}"))?;

        // Capture QuickBooks' refresh-token var name *before* secret
        // resolution strips it — the write-back sink needs to know which
        // secret to update when Intuit rotates the token mid-run.
        let qb_refresh_var: Option<String> = if spec.source.kind == "quickbooks" {
            spec.source
                .config
                .get("refresh_token_var")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        } else {
            None
        };

        // Resource override (e.g. "retry failed tables"): restrict the run
        // to the named subset. The worker filters the source by
        // `spec.resources`, so this re-runs only those streams.
        if !resources.is_empty() {
            spec.resources = resources.to_vec();
        }

        self.resolve_airway_source_secrets(&mut spec).await?;
        let airhouse_db = self.resolve_airway_destination(&mut spec).await?;

        let db = Arc::new(self.db.clone());
        let mut worker = match qb_refresh_var {
            Some(var_name) => {
                let sink: Arc<dyn agentic_airway::RefreshTokenSink> =
                    Arc::new(PlatformRefreshTokenSink {
                        platform: self.platform.clone(),
                        var_name,
                    });
                agentic_airway::AirwayWorker::with_refresh_sink(db, sink)
            }
            None => agentic_airway::AirwayWorker::new(db),
        };
        // Airhouse destinations hold/cycle one pgwire connection for the whole
        // load; attach a provider so each (re)connect re-mints a fresh
        // (non-expired) ephemeral credential instead of reusing the static DSN.
        if let Some(database) = airhouse_db {
            let provider: Arc<dyn agentic_airway::CredentialProvider> =
                Arc::new(PlatformAirhouseCredentialProvider {
                    platform: self.platform.clone(),
                    database,
                });
            worker = worker.with_credential_provider(provider);
        }
        Ok(worker.execute(spec))
    }

    /// Dispatch a `TaskSpec::Compile` through the host-supplied
    /// [`CompileDispatcher`] port. The actual worker (which touches the
    /// `entity` crate for the compile boundary schema) lives in the host
    /// — pipeline keeps no `oxy-compile` / `entity` deps per the
    /// layering rules.
    async fn execute_compile(
        &self,
        workspace_id: uuid::Uuid,
        git_sha: Option<String>,
        branch: Option<String>,
        promote: bool,
        kind: Option<&str>,
        owner_user_id: Option<uuid::Uuid>,
    ) -> Result<ExecutingTask, String> {
        let dispatcher = self.platform.compile_dispatcher().ok_or_else(|| {
            "compile: PlatformContext::compile_dispatcher() returned None — the host \
             needs to wire OxyCompileDispatcher (or equivalent) for compile tasks to run."
                .to_string()
        })?;
        dispatcher
            .dispatch(
                workspace_id,
                git_sha,
                branch,
                promote,
                kind.map(str::to_string),
                owner_user_id,
            )
            .await
    }

    /// Substitute a source's `*_var` credential references with values from the
    /// platform secret manager, then strip the `_var` keys so the connector
    /// factory sees only resolved literals. Each source kind opts in
    /// explicitly to the (field, var-key) pairs it manages as secrets.
    async fn resolve_airway_source_secrets(
        &self,
        spec: &mut agentic_airway::AirwayPipelineSpec,
    ) -> Result<(), String> {
        let kind = spec.source.kind.clone();
        // rest_api carries its credential nested under `config.auth`
        // (`token_var`/`key_var`), not as a flat `config` field like the kinds
        // below, so it resolves through its own helper.
        if kind == "rest_api" {
            return self.resolve_rest_api_auth_secrets(spec).await;
        }
        // (field, var-key) pairs each source kind supports as managed secrets.
        // `client_id` / `realm_id` are identifiers, not secrets. Kinds not
        // listed here carry no managed credentials.
        //
        // KNOWN / FOLLOW-UP (out of scope here): this table is stringly typed
        // and has no compile-time link to airway's per-kind source defs (same
        // for the `kind: String` discovery field in agentic-http). The planned
        // cleanup is to expose `Source::managed_secrets()` from each Params
        // struct so this stays in sync automatically.
        let pairs: &[(&str, &str)] = match kind.as_str() {
            "toast" => &[
                ("client_secret", "client_secret_var"),
                ("client_id", "client_id_var"),
            ],
            "quickbooks" => &[
                ("client_secret", "client_secret_var"),
                ("refresh_token", "refresh_token_var"),
            ],
            "clickhouse" => &[("password", "password_var")],
            // Open-Meteo commercial API key → routes the connector to the paid
            // `customer-*` endpoint (the keyless endpoint is non-commercial only).
            "weather" => &[("api_key", "api_key_var")],
            // BestTime private API key → POSTed as `api_key_private` query
            // param to `/forecasts` (every call). Same pattern as `weather`.
            "besttime" => &[("api_key", "api_key_var")],
            _ => return Ok(()),
        };
        let Some(obj) = spec.source.config.as_object_mut() else {
            return Ok(());
        };
        for (field, var_key) in pairs {
            let Some(var_val) = obj.get(*var_key) else {
                continue;
            };
            let var_name = var_val.as_str().ok_or_else(|| {
                format!("airway {kind}: `{var_key}` must be a string secret name")
            })?;
            let secret = self
                .platform
                .resolve_secret(var_name)
                .await
                .ok_or_else(|| {
                    format!(
                        "airway {kind}: secret `{var_name}` (referenced by `{var_key}`) \
                     could not be resolved from the secret manager"
                    )
                })?;
            // A resolved-but-empty secret is treated as "unset": skip the
            // field insert so an absent credential stays absent (e.g.
            // ClickHouse must send no `X-ClickHouse-Key`, not an empty one —
            // see `clickhouse_conn` in agentic-airway). The `var_key` is
            // still removed so the rendered spec never leaks the indirection.
            if !secret.is_empty() {
                obj.insert((*field).to_string(), serde_json::Value::String(secret));
            }
            obj.remove(*var_key);
        }
        Ok(())
    }

    /// rest_api credentials live nested under `config.auth` as `token_var`
    /// (bearer) / `key_var` (`api_key` header + `api_key_query`), unlike the
    /// flat-config kinds in [`Self::resolve_airway_source_secrets`]. Resolve
    /// each from the platform secret manager into its literal `auth.{token,key}`
    /// field, then strip the `*_var` indirection so the connector factory sees
    /// only resolved literals.
    async fn resolve_rest_api_auth_secrets(
        &self,
        spec: &mut agentic_airway::AirwayPipelineSpec,
    ) -> Result<(), String> {
        for (field, var_key, var_name) in rest_api_secret_var_refs(&spec.source.config)? {
            let secret = self.platform.resolve_secret(&var_name).await.ok_or_else(|| {
                format!(
                    "airway rest_api: secret `{var_name}` (referenced by `auth.{var_key}`) \
                     could not be resolved from the secret manager"
                )
            })?;
            set_rest_api_auth_secret(&mut spec.source.config, field, var_key, &secret);
        }
        Ok(())
    }

    /// Turn a `destination: { database, dataset_name }` reference into a
    /// concrete inline connector by resolving the named `config.yml`
    /// database through the platform (secret substitution + per-subject
    /// `airhouse_managed` minting happen host-side). Inline destinations
    /// (the `memory` fixture, already-resolved specs) pass through.
    ///
    /// Returns `Some(database)` when it resolved to an **airhouse**
    /// destination, so the caller can attach a credential provider that
    /// re-mints the ephemeral credential on every (re)connect; `None`
    /// otherwise.
    async fn resolve_airway_destination(
        &self,
        spec: &mut agentic_airway::AirwayPipelineSpec,
    ) -> Result<Option<String>, String> {
        let agentic_airway::DestinationSpec::Reference(ref_) = &spec.destination else {
            return Ok(None);
        };
        let database = ref_.database.clone();
        let dataset_name = ref_.dataset_name.clone();
        let schema_separator = ref_.schema_separator.clone();
        let resolved = self
            .platform
            .resolve_pipeline_destination(&database)
            .await
            .ok_or_else(|| {
                format!(
                    "airway: destination `database: {database}` is not a known \
                     config.yml database with an airway-writable type \
                     (postgres or airhouse)"
                )
            })?;
        let mut config = serde_json::json!({
            "connection_string": resolved.connection_string,
            "dataset_name": dataset_name,
        });
        // `schema_separator` is an airhouse-only knob. Gate on the resolved
        // kind: other destinations (`postgres`, `memory`) deny unknown fields,
        // so emitting it would surface as an opaque YAML-parse error at run
        // start. Fail fast with a clear message instead.
        if let Some(sep) = schema_separator {
            if resolved.kind != "airhouse" {
                return Err(format!(
                    "airway: `schema_separator` only applies to airhouse destinations, \
                     but `database: {database}` resolves to `{}`. Remove `schema_separator` \
                     from the pipeline's destination.",
                    resolved.kind
                ));
            }
            config["schema_separator"] = serde_json::Value::String(sep);
        }
        let is_airhouse = resolved.kind == "airhouse";
        spec.destination =
            agentic_airway::DestinationSpec::Inline(agentic_airway::DestinationConfig {
                kind: resolved.kind,
                config,
            });
        Ok(is_airhouse.then_some(database))
    }
}

/// Collect `(target_field, var_key, secret_name)` triples for `*_var`
/// credential references nested under a rest_api source's `config.auth`:
/// `token_var` → `token` (bearer) and `key_var` → `key` (`api_key` header +
/// `api_key_query`). Pure — performs no secret lookup; the async resolution
/// happens in [`PipelineTaskExecutor::resolve_rest_api_auth_secrets`].
///
/// A present-but-non-string `*_var` is a hard error (matching the flat-config
/// path in [`PipelineTaskExecutor::resolve_airway_source_secrets`]); an absent
/// one is simply skipped.
fn rest_api_secret_var_refs(
    config: &serde_json::Value,
) -> Result<Vec<(&'static str, &'static str, String)>, String> {
    let mut refs = Vec::new();
    let Some(auth) = config.get("auth").and_then(|a| a.as_object()) else {
        return Ok(refs);
    };
    for (field, var_key) in [("token", "token_var"), ("key", "key_var")] {
        if let Some(value) = auth.get(var_key) {
            let name = value.as_str().ok_or_else(|| {
                format!("airway rest_api: `auth.{var_key}` must be a string secret name")
            })?;
            refs.push((field, var_key, name.to_string()));
        }
    }
    Ok(refs)
}

/// Apply one resolved rest_api auth secret in place: set `config.auth[field]`
/// to the resolved literal (skipped when empty — treated as "unset", mirroring
/// the flat-config kinds in [`PipelineTaskExecutor::resolve_airway_source_secrets`])
/// and always strip `config.auth[var_key]` so the rendered spec never leaks the
/// `*_var` indirection to the connector factory.
fn set_rest_api_auth_secret(
    config: &mut serde_json::Value,
    field: &str,
    var_key: &str,
    secret: &str,
) {
    let Some(auth) = config.get_mut("auth").and_then(|a| a.as_object_mut()) else {
        return;
    };
    if !secret.is_empty() {
        auth.insert(
            field.to_string(),
            serde_json::Value::String(secret.to_string()),
        );
    }
    auth.remove(var_key);
}

/// Persists a rotated OAuth refresh token back to the platform secret
/// store. Wired into the airway worker for `quickbooks` pipelines: when
/// Intuit rotates the refresh token mid-run, the connector calls
/// [`persist`](agentic_airway::RefreshTokenSink::persist) and we upsert
/// the new value under the same `*_var` secret name the run resolved from.
struct PlatformRefreshTokenSink {
    platform: Arc<dyn PlatformContext>,
    var_name: String,
}

#[async_trait]
impl agentic_airway::RefreshTokenSink for PlatformRefreshTokenSink {
    async fn persist(&self, refresh_token: &str) -> Result<(), String> {
        self.platform
            .persist_secret(&self.var_name, refresh_token)
            .await
    }
}

/// Re-mints a fresh `airhouse_managed` credential on every (re)connect for an
/// airway pipeline destination. Wired into the airway worker for airhouse
/// destinations: when the destination opens or cycles its long-lived pgwire
/// connection, it calls this to get a freshly-minted DSN.
///
/// DESIGN ASSUMPTION (verified against airhouse as of 0.x, but a CP property
/// not enforced here): a credential's `expires_at` is checked **only at the
/// SCRAM handshake** — `get_user_credentials` filters expired rows and is the
/// auth path's lookup — and never per-query, so an established session persists
/// past the credential's expiry (the ephemeral-user sweeper only reclaims
/// storage after a grace window, it doesn't drop live sessions). That's why
/// re-resolving (which re-mints via the broker) on each connect is sufficient
/// and the standard short TTL needs no bump. If airhouse ever starts validating
/// `expires_at` per-query, long single-segment loads would fail and this
/// provider would need to force a full-TTL mint (`evict_and_remint`) per cycle.
struct PlatformAirhouseCredentialProvider {
    platform: Arc<dyn PlatformContext>,
    database: String,
}

#[async_trait]
impl agentic_airway::CredentialProvider for PlatformAirhouseCredentialProvider {
    async fn connection_string(&self) -> Result<String, String> {
        self.platform
            .resolve_pipeline_destination(&self.database)
            .await
            .map(|resolved| resolved.connection_string)
            .ok_or_else(|| {
                format!(
                    "airway: failed to re-resolve airhouse destination `{}` \
                     for credential refresh",
                    self.database
                )
            })
    }
}

mod workflow;

pub use workflow::run_decision_task;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_agent_id_routes_to_builder() {
        assert!(is_builder_agent("__builder__"));
    }

    #[test]
    fn regular_agent_id_routes_to_analytics() {
        assert!(!is_builder_agent("revenue"));
        assert!(!is_builder_agent("duckdb"));
        assert!(!is_builder_agent(""));
    }

    #[test]
    fn rest_api_bearer_token_var_is_collected() {
        let config = serde_json::json!({
            "base_url": "https://api.yelp.com/v3",
            "auth": { "type": "bearer", "token_var": "YELP_API_KEY" }
        });
        assert_eq!(
            rest_api_secret_var_refs(&config).unwrap(),
            vec![("token", "token_var", "YELP_API_KEY".to_string())]
        );
    }

    #[test]
    fn rest_api_api_key_query_key_var_is_collected() {
        let config = serde_json::json!({
            "auth": { "type": "api_key_query", "key_var": "CENSUS_API_KEY", "param": "key" }
        });
        assert_eq!(
            rest_api_secret_var_refs(&config).unwrap(),
            vec![("key", "key_var", "CENSUS_API_KEY".to_string())]
        );
    }

    #[test]
    fn rest_api_literal_or_absent_auth_collects_no_refs() {
        // already-literal token (no `*_var` indirection) → nothing to resolve
        let literal = serde_json::json!({ "auth": { "type": "bearer", "token": "sk-literal" } });
        assert!(rest_api_secret_var_refs(&literal).unwrap().is_empty());
        // no auth block at all (e.g. keyless public API like nces_schools)
        let none = serde_json::json!({ "base_url": "https://example.com" });
        assert!(rest_api_secret_var_refs(&none).unwrap().is_empty());
    }

    #[test]
    fn rest_api_non_string_var_is_a_hard_error() {
        // A present-but-non-string `*_var` is a config typo; error loudly like
        // the flat-config path rather than silently skipping the credential.
        let config = serde_json::json!({
            "auth": { "type": "bearer", "token_var": 123 }
        });
        let err = rest_api_secret_var_refs(&config).unwrap_err();
        assert!(err.contains("must be a string secret name"), "got: {err}");
    }

    #[test]
    fn set_rest_api_auth_secret_writes_field_and_strips_var() {
        let mut config = serde_json::json!({
            "auth": { "type": "bearer", "token_var": "YELP_API_KEY" }
        });
        set_rest_api_auth_secret(&mut config, "token", "token_var", "sk-abc123");
        assert_eq!(config["auth"]["token"], "sk-abc123");
        assert!(
            config["auth"].get("token_var").is_none(),
            "the `*_var` indirection must be stripped so the connector never sees it"
        );
    }

    #[test]
    fn set_rest_api_auth_secret_empty_secret_skips_field_and_strips_var() {
        // An empty resolved secret is "unset": don't write an empty token, but
        // still strip the var so the rendered spec carries no indirection.
        let mut config = serde_json::json!({
            "auth": { "type": "bearer", "token_var": "YELP_API_KEY" }
        });
        set_rest_api_auth_secret(&mut config, "token", "token_var", "");
        assert!(config["auth"].get("token").is_none());
        assert!(config["auth"].get("token_var").is_none());
    }
}
