//! High-level API for starting and driving agentic pipelines.
//!
//! [`PipelineBuilder`] encapsulates config loading, connector resolution,
//! solver building, and pipeline startup. Both the HTTP layer and the CLI
//! use this crate — no domain logic is duplicated.

pub mod executor;
pub mod platform;
pub mod recovery;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentic_analytics::SchemaCatalog;
use agentic_analytics::config::AgentConfig;
use agentic_builder::BuilderTestRunner;
use agentic_llm::LlmClient;
use agentic_runtime::event_registry::EventRegistry;
use agentic_runtime::handle::{PipelineHandle, PipelineOutcome};
use agentic_runtime::state::RuntimeState;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::platform::{BuilderBridges, PlatformContext, ProjectContext};

// ── Re-exports for consumers ────────────────────────────────────────────────

/// Re-export so HTTP/CLI don't import domain crates directly.
pub use agentic_analytics::AnalyticsRunMeta;
pub use agentic_analytics::SchemaCatalog as AnalyticsSchemaCatalog;
pub use agentic_analytics::extension::AnalyticsMigrator;
pub use agentic_analytics::{AnalyticsMetricSink, SharedMetricSink};
pub use agentic_builder::BuilderTestRunner as BuilderTestRunnerTrait;
pub use agentic_workflow::WorkflowMigrator;

// ── ThinkingMode ────────────────────────────────────────────────────────────

/// Thinking mode preset for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    Auto,
    ExtendedThinking,
}

impl ThinkingMode {
    /// Serialize for DB storage. Always returns `Some` so analytics queries can
    /// filter on `thinking_mode` without special-casing NULL. NULL in the
    /// column means "mode was never set" (e.g. legacy rows), not "Auto".
    pub fn to_db(self) -> Option<String> {
        match self {
            Self::Auto => Some("auto".to_string()),
            Self::ExtendedThinking => Some("extended_thinking".to_string()),
        }
    }

    pub fn is_extended(self) -> bool {
        matches!(self, Self::ExtendedThinking)
    }
}

impl Default for ThinkingMode {
    fn default() -> Self {
        Self::Auto
    }
}

// ── PipelineBuilder ─────────────────────────────────────────────────────────

/// Builder for starting agentic pipelines.
///
/// Encapsulates all domain-specific setup (config loading, connector
/// resolution, solver building) behind a clean API. Both the HTTP layer
/// and the CLI use this builder.
pub struct PipelineBuilder {
    platform: Arc<dyn PlatformContext>,
    builder_bridges: Option<BuilderBridges>,
    domain: Option<Domain>,
    question: String,
    thread_id: Option<Uuid>,
    thinking_mode: ThinkingMode,
    schema_cache: Option<Arc<Mutex<HashMap<String, SchemaCatalog>>>>,
    builder_test_runner: Option<Arc<dyn BuilderTestRunner>>,
    /// When set, use this run_id and skip the DB `insert_run` call.
    /// Used for delegation children where the coordinator already created
    /// the run via `insert_run_with_parent`.
    existing_run_id: Option<String>,
    /// Override the default human input provider for the builder domain.
    /// When set, passed through to `BuilderPipelineParams.human_input`.
    human_input: Option<agentic_core::human_input::HumanInputHandle>,
}

enum Domain {
    Analytics { agent_id: String },
    Builder { model: Option<String> },
}

/// Error from pipeline building.
#[derive(Debug)]
pub enum PipelineError {
    Config(String),
    Build(String),
    Db(sea_orm::DbErr),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(msg) => write!(f, "config error: {msg}"),
            Self::Build(msg) => write!(f, "build error: {msg}"),
            Self::Db(e) => write!(f, "db error: {e}"),
        }
    }
}

impl From<sea_orm::DbErr> for PipelineError {
    fn from(e: sea_orm::DbErr) -> Self {
        Self::Db(e)
    }
}

impl PipelineBuilder {
    pub fn new(platform: Arc<dyn PlatformContext>) -> Self {
        Self {
            platform,
            builder_bridges: None,
            domain: None,
            question: String::new(),
            thread_id: None,
            thinking_mode: ThinkingMode::Auto,
            schema_cache: None,
            builder_test_runner: None,
            existing_run_id: None,
            human_input: None,
        }
    }

    /// Supply the four builder-domain port impls. Required before starting
    /// the builder pipeline; ignored for analytics runs.
    pub fn with_builder_bridges(mut self, bridges: BuilderBridges) -> Self {
        self.builder_bridges = Some(bridges);
        self
    }

    /// Override the human input provider for the builder domain.
    pub fn human_input(mut self, provider: agentic_core::human_input::HumanInputHandle) -> Self {
        self.human_input = Some(provider);
        self
    }

    /// Use an existing run_id instead of generating a new one.
    ///
    /// When set, `start()` skips the DB `insert_run` call — the caller
    /// (typically the coordinator) is responsible for having already
    /// created the run row.  Used for delegation children.
    pub fn existing_run(mut self, run_id: String) -> Self {
        self.existing_run_id = Some(run_id);
        self
    }

    /// Configure for the analytics domain.
    pub fn analytics(mut self, agent_id: &str) -> Self {
        self.domain = Some(Domain::Analytics {
            agent_id: agent_id.to_string(),
        });
        self
    }

    /// Configure for the builder domain.
    pub fn builder(mut self, model: Option<String>) -> Self {
        self.domain = Some(Domain::Builder { model });
        self
    }

    /// Set the user's question.
    pub fn question(mut self, q: &str) -> Self {
        self.question = q.to_string();
        self
    }

    /// Link to a conversation thread.
    pub fn thread(mut self, id: Uuid) -> Self {
        self.thread_id = Some(id);
        self
    }

    /// Set thinking mode.
    pub fn thinking_mode(mut self, mode: ThinkingMode) -> Self {
        self.thinking_mode = mode;
        self
    }

    /// Set schema cache (shared across requests in HTTP mode).
    pub fn schema_cache(mut self, cache: Arc<Mutex<HashMap<String, SchemaCatalog>>>) -> Self {
        self.schema_cache = Some(cache);
        self
    }

    /// Set builder test runner.
    pub fn test_runner(mut self, runner: Arc<dyn BuilderTestRunner>) -> Self {
        self.builder_test_runner = Some(runner);
        self
    }

    /// Build and start the pipeline.
    ///
    /// Inserts the run record in the database, starts the domain pipeline,
    /// and returns a [`StartedPipeline`] with an erased handle.
    pub async fn start(
        mut self,
        db: &DatabaseConnection,
    ) -> Result<StartedPipeline, PipelineError> {
        let domain = self.domain.take().ok_or_else(|| {
            PipelineError::Config("domain not set (call .analytics() or .builder())".into())
        })?;

        // When an existing_run_id is provided (delegation child), use it and
        // skip the DB insert — the coordinator already created the run row.
        let (run_id, skip_db_insert) = match self.existing_run_id.take() {
            Some(id) => (id, true),
            None => (Uuid::new_v4().to_string(), false),
        };
        let base_dir = self.platform.workspace_path().to_path_buf();

        match domain {
            Domain::Analytics { agent_id } => {
                self.start_analytics(db, &run_id, &agent_id, &base_dir, skip_db_insert)
                    .await
            }
            Domain::Builder { model } => {
                self.start_builder(db, &run_id, model, &base_dir, skip_db_insert)
                    .await
            }
        }
    }

    /// Resume a suspended run with the user's answer.
    ///
    /// Rebuilds the solver + orchestrator from config, then calls
    /// `orchestrator.resume(resume_data, answer)` instead of `run(intent)`.
    /// Does NOT insert a new run record — the existing run is reused.
    ///
    /// `source_type` must be `"analytics"` or `"builder"`.
    pub async fn resume(
        self,
        db: &DatabaseConnection,
        run_id: &str,
        source_type: &str,
        agent_id: &str,
        model: Option<String>,
        resume_data: agentic_core::human_input::SuspendedRunData,
        answer: String,
    ) -> Result<StartedPipeline, PipelineError> {
        let base_dir = self.platform.workspace_path().to_path_buf();

        // Update DB status back to running.
        agentic_runtime::crud::update_run_running(db, run_id).await?;

        match source_type {
            "analytics" => {
                self.resume_analytics(db, run_id, agent_id, &base_dir, resume_data, answer)
                    .await
            }
            "builder" => {
                self.resume_builder(db, run_id, model, &base_dir, resume_data, answer)
                    .await
            }
            _ => Err(PipelineError::Config(format!(
                "cold resume not supported for source_type: {source_type}"
            ))),
        }
    }

    async fn start_analytics(
        self,
        db: &DatabaseConnection,
        run_id: &str,
        agent_id: &str,
        base_dir: &std::path::Path,
        skip_db_insert: bool,
    ) -> Result<StartedPipeline, PipelineError> {
        // Load config — try the literal path first, then with `.agentic.yml` extension.
        let config_path = base_dir.join(agent_id);
        let config_path = if config_path.exists() {
            config_path
        } else {
            let with_ext = base_dir.join(format!("{}.agentic.yml", agent_id));
            if with_ext.exists() {
                with_ext
            } else {
                config_path // will produce a clear "not found" error
            }
        };
        let config = AgentConfig::from_file(&config_path)
            .map_err(|e| PipelineError::Config(format!("{e}")))?;

        // Insert run + extension (skipped for delegation children — the
        // coordinator already created the run via insert_run_with_parent).
        let source_type = "analytics";
        if !skip_db_insert {
            let metadata = serde_json::json!({
                "agent_id": agent_id,
                "thinking_mode": self.thinking_mode.to_db(),
            });
            agentic_runtime::crud::insert_run(
                db,
                run_id,
                &self.question,
                self.thread_id,
                source_type,
                Some(metadata),
            )
            .await?;
            agentic_analytics::insert_run_meta(db, run_id, agent_id, self.thinking_mode.to_db())
                .await?;
        }

        // Resolve project model + connectors via the platform port.
        let project_model = self
            .platform
            .resolve_model(config.llm.model_ref.as_deref(), config.llm.model.is_some())
            .await;

        // Resolve databases + connectors.
        let mut effective_databases: Vec<String> = config.databases.clone();
        if let Ok(resolved) = config.resolve_context(base_dir) {
            for db_name in resolved.referenced_databases {
                if !effective_databases.contains(&db_name) {
                    effective_databases.push(db_name);
                }
            }
        }
        let connector_configs =
            platform::resolve_connectors(&effective_databases, &*self.platform).await;
        let connectors = agentic_connector::build_named_connectors(connector_configs).await;

        // Procedure runner.
        let procedure_runner: Option<Arc<dyn agentic_analytics::ProcedureRunner>> = {
            let procedure_files = config
                .resolve_context(base_dir)
                .map(|ctx| ctx.procedure_files)
                .unwrap_or_default();
            let workspace: Arc<dyn agentic_workflow::WorkspaceContext> = self.platform.clone();
            let runner = agentic_workflow::OxyProcedureRunner::new(workspace)
                .with_procedure_files(procedure_files);
            Some(Arc::new(runner))
        };

        // Thread history.
        let (history, prior_spec_hint) = if let Some(tid) = self.thread_id {
            let turns = agentic_runtime::crud::get_thread_history(db, tid, 10)
                .await
                .unwrap_or_default();
            let history: Vec<agentic_analytics::ConversationTurn> = turns
                .into_iter()
                .map(|t| agentic_analytics::ConversationTurn {
                    question: t.question,
                    answer: t.answer,
                })
                .collect();
            (history, None)
        } else {
            (vec![], None)
        };

        // Build params.
        let params = agentic_analytics::PipelineParams {
            config,
            base_dir: base_dir.to_path_buf(),
            agent_id: agent_id.to_string(),
            connectors,
            default_connector: effective_databases.first().cloned(),
            question: self.question,
            history,
            prior_spec_hint,
            schema_cache: self.schema_cache,
            project_model,
            use_extended_thinking: self.thinking_mode.is_extended(),
            procedure_runner,
            metric_sink: self.platform.metric_sink(),
        };

        // Start pipeline.
        let handle = agentic_analytics::start_pipeline(params)
            .await
            .map_err(|e| PipelineError::Build(format!("{e}")))?;

        Ok(StartedPipeline {
            run_id: run_id.to_string(),
            source_type: source_type.to_string(),
            inner: ErasedHandle::Analytics(handle),
        })
    }

    async fn resume_analytics(
        self,
        db: &DatabaseConnection,
        run_id: &str,
        agent_id: &str,
        base_dir: &std::path::Path,
        resume_data: agentic_core::human_input::SuspendedRunData,
        answer: String,
    ) -> Result<StartedPipeline, PipelineError> {
        // Load config (same resolution as start_analytics).
        let config_path = base_dir.join(agent_id);
        let config_path = if config_path.exists() {
            config_path
        } else {
            let with_ext = base_dir.join(format!("{}.agentic.yml", agent_id));
            if with_ext.exists() {
                with_ext
            } else {
                config_path
            }
        };
        let config = AgentConfig::from_file(&config_path)
            .map_err(|e| PipelineError::Config(format!("{e}")))?;

        // Resolve project model + connectors via the platform port.
        let project_model = self
            .platform
            .resolve_model(config.llm.model_ref.as_deref(), config.llm.model.is_some())
            .await;

        // Resolve databases + connectors.
        let mut effective_databases: Vec<String> = config.databases.clone();
        if let Ok(resolved) = config.resolve_context(base_dir) {
            for db_name in resolved.referenced_databases {
                if !effective_databases.contains(&db_name) {
                    effective_databases.push(db_name);
                }
            }
        }
        let connector_configs =
            platform::resolve_connectors(&effective_databases, &*self.platform).await;
        let connectors = agentic_connector::build_named_connectors(connector_configs).await;

        // Procedure runner.
        let procedure_runner: Option<Arc<dyn agentic_analytics::ProcedureRunner>> = {
            let procedure_files = config
                .resolve_context(base_dir)
                .map(|ctx| ctx.procedure_files)
                .unwrap_or_default();
            let workspace: Arc<dyn agentic_workflow::WorkspaceContext> = self.platform.clone();
            let runner = agentic_workflow::OxyProcedureRunner::new(workspace)
                .with_procedure_files(procedure_files);
            Some(Arc::new(runner))
        };

        // Thread history.
        let (history, prior_spec_hint) = if let Some(tid) = self.thread_id {
            let turns = agentic_runtime::crud::get_thread_history(db, tid, 10)
                .await
                .unwrap_or_default();
            let history: Vec<agentic_analytics::ConversationTurn> = turns
                .into_iter()
                .map(|t| agentic_analytics::ConversationTurn {
                    question: t.question,
                    answer: t.answer,
                })
                .collect();
            (history, None)
        } else {
            (vec![], None)
        };

        let params = agentic_analytics::PipelineParams {
            config,
            base_dir: base_dir.to_path_buf(),
            agent_id: agent_id.to_string(),
            connectors,
            default_connector: effective_databases.first().cloned(),
            question: self.question,
            history,
            prior_spec_hint,
            schema_cache: self.schema_cache,
            project_model,
            use_extended_thinking: self.thinking_mode.is_extended(),
            procedure_runner,
            metric_sink: self.platform.metric_sink(),
        };

        let handle = agentic_analytics::resume_pipeline(params, resume_data, answer)
            .await
            .map_err(|e| PipelineError::Build(format!("{e}")))?;

        Ok(StartedPipeline {
            run_id: run_id.to_string(),
            source_type: "analytics".to_string(),
            inner: ErasedHandle::Analytics(handle),
        })
    }

    async fn resume_builder(
        self,
        db: &DatabaseConnection,
        run_id: &str,
        model: Option<String>,
        base_dir: &std::path::Path,
        resume_data: agentic_core::human_input::SuspendedRunData,
        answer: String,
    ) -> Result<StartedPipeline, PipelineError> {
        let bridges = self.builder_bridges.clone().ok_or_else(|| {
            PipelineError::Config(
                "builder bridges not provided — call .with_builder_bridges() first".into(),
            )
        })?;

        // Resolve model + API key (same as start_builder).
        let client = build_builder_llm_client(&*self.platform, model).await;

        // Thread history.
        let history: Vec<agentic_builder::ConversationTurn> = if let Some(tid) = self.thread_id {
            agentic_runtime::crud::get_thread_history_with_events(db, tid, 10)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(q, a, exchanges)| agentic_builder::ConversationTurn {
                    question: q,
                    answer: a,
                    tool_exchanges: exchanges
                        .into_iter()
                        .map(|e| agentic_builder::ToolExchange {
                            name: e.name,
                            input: e.input,
                            output: e.output,
                        })
                        .collect(),
                })
                .collect()
        } else {
            vec![]
        };

        let handle = agentic_builder::resume_pipeline(
            agentic_builder::BuilderPipelineParams {
                client,
                project_root: base_dir.to_path_buf(),
                question: self.question,
                history,
                db_provider: Some(bridges.db_provider),
                project_validator: Some(bridges.project_validator),
                schema_provider: Some(bridges.schema_provider),
                semantic_compiler: Some(bridges.semantic_compiler),
                test_runner: self.builder_test_runner,
                human_input: None,
            },
            resume_data,
            answer,
        );

        Ok(StartedPipeline {
            run_id: run_id.to_string(),
            source_type: "builder".to_string(),
            inner: ErasedHandle::Builder(handle),
        })
    }

    async fn start_builder(
        self,
        db: &DatabaseConnection,
        run_id: &str,
        model: Option<String>,
        base_dir: &std::path::Path,
        skip_db_insert: bool,
    ) -> Result<StartedPipeline, PipelineError> {
        // Skip DB insert for delegation children — the coordinator already
        // created the run via insert_run_with_parent.
        let source_type = "builder";
        if !skip_db_insert {
            let metadata = serde_json::json!({ "agent_id": "__builder__" });
            agentic_runtime::crud::insert_run(
                db,
                run_id,
                &self.question,
                self.thread_id,
                source_type,
                Some(metadata),
            )
            .await?;
        }

        let bridges = self.builder_bridges.clone().ok_or_else(|| {
            PipelineError::Config(
                "builder bridges not provided — call .with_builder_bridges() first".into(),
            )
        })?;

        // Resolve model + API key.
        let client = build_builder_llm_client(&*self.platform, model).await;

        // Thread history.
        let history: Vec<agentic_builder::ConversationTurn> = if let Some(tid) = self.thread_id {
            agentic_runtime::crud::get_thread_history_with_events(db, tid, 10)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(q, a, exchanges)| agentic_builder::ConversationTurn {
                    question: q,
                    answer: a,
                    tool_exchanges: exchanges
                        .into_iter()
                        .map(|e| agentic_builder::ToolExchange {
                            name: e.name,
                            input: e.input,
                            output: e.output,
                        })
                        .collect(),
                })
                .collect()
        } else {
            vec![]
        };

        let handle = agentic_builder::start_pipeline(agentic_builder::BuilderPipelineParams {
            client,
            project_root: base_dir.to_path_buf(),
            question: self.question,
            history,
            db_provider: Some(bridges.db_provider),
            project_validator: Some(bridges.project_validator),
            schema_provider: Some(bridges.schema_provider),
            semantic_compiler: Some(bridges.semantic_compiler),
            test_runner: self.builder_test_runner,
            human_input: self.human_input,
        });

        Ok(StartedPipeline {
            run_id: run_id.to_string(),
            source_type: source_type.to_string(),
            inner: ErasedHandle::Builder(handle),
        })
    }
}

/// Resolve the builder domain's LLM client via the platform port.
///
/// Preserves the legacy fallback: if no model config matches, default to
/// `claude-sonnet-4-6` with the key read from `ANTHROPIC_API_KEY`.
async fn build_builder_llm_client(ctx: &dyn ProjectContext, model: Option<String>) -> LlmClient {
    let model_name = model.unwrap_or_else(|| "claude-sonnet-4-6".to_string());
    if let Some(info) = ctx.resolve_model(Some(&model_name), false).await {
        return platform::build_llm_client(&info);
    }
    let api_key = ctx
        .resolve_secret("ANTHROPIC_API_KEY")
        .await
        .unwrap_or_default();
    LlmClient::with_model(api_key, model_name)
}

// ── StartedPipeline (type-erased) ───────────────────────────────────────────

/// A started pipeline with type-erased domain events.
///
/// Call [`drive()`](StartedPipeline::drive) to run the full lifecycle
/// (bridge task + outcome loop + cleanup).
pub struct StartedPipeline {
    pub run_id: String,
    pub source_type: String,
    inner: ErasedHandle,
}

enum ErasedHandle {
    Analytics(agentic_runtime::handle::PipelineHandle<agentic_analytics::AnalyticsEvent>),
    Builder(agentic_runtime::handle::PipelineHandle<agentic_builder::BuilderEvent>),
}

impl StartedPipeline {
    /// Drive the pipeline through its full lifecycle using the runtime.
    ///
    /// Spawns the bridge task, processes outcomes, handles suspension/resume,
    /// Convert into an [`ExecutingTask`] for use with the coordinator-worker
    /// architecture.
    ///
    /// Spawns background tasks that:
    /// - Drain domain-typed events, serialize them to `(String, Value)`, and
    ///   forward to the `ExecutingTask::events` channel.
    /// - Map [`PipelineOutcome`] to [`TaskOutcome`] and send on the outcome
    ///   channel.
    pub fn into_executing_task(
        self,
    ) -> (
        agentic_runtime::worker::ExecutingTask,
        tokio::task::JoinHandle<()>,
    ) {
        use agentic_core::delegation::TaskOutcome;

        tracing::info!(target: "worker", run_id = %self.run_id, source_type = %self.source_type, "converting StartedPipeline into ExecutingTask");
        let (event_tx, event_rx) = mpsc::channel::<(String, serde_json::Value)>(256);
        let (outcome_tx, outcome_rx) = mpsc::channel::<TaskOutcome>(4);
        let cancel = tokio_util::sync::CancellationToken::new();

        // HITL answers are routed via `RuntimeState::answer_txs` by the
        // coordinator, not through `ExecutingTask::answers`, so we pass `None`.
        let bridge_handle = match self.inner {
            ErasedHandle::Analytics(handle) => {
                spawn_bridge_tasks(handle, event_tx, outcome_tx, cancel.clone())
            }
            ErasedHandle::Builder(handle) => {
                spawn_bridge_tasks(handle, event_tx, outcome_tx, cancel.clone())
            }
        };

        (
            agentic_runtime::worker::ExecutingTask {
                events: event_rx,
                outcomes: outcome_rx,
                cancel,
                answers: None,
            },
            bridge_handle,
        )
    }
}

/// Spawn tasks that bridge a typed `PipelineHandle<Ev>` into the generic
/// `ExecutingTask` channels.
///
/// Returns a `JoinHandle` that completes once the event-draining and
/// outcome-forwarding bridge tasks have both finished. Callers that need to
/// know when no more events or outcomes will be forwarded (e.g. before
/// notifying SSE subscribers) can await it — bounded with a timeout in case a
/// producer keeps a sender open past the terminal outcome.
fn spawn_bridge_tasks<Ev: agentic_core::DomainEvents + 'static>(
    handle: PipelineHandle<Ev>,
    event_tx: mpsc::Sender<(String, serde_json::Value)>,
    outcome_tx: mpsc::Sender<agentic_core::delegation::TaskOutcome>,
    cancel: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    use agentic_core::delegation::TaskOutcome;

    let pipeline_cancel = handle.cancel.clone();
    let mut events = handle.events;
    let mut outcomes = handle.outcomes;

    // Forward cancellation.
    tokio::spawn({
        let cancel = cancel.clone();
        async move {
            cancel.cancelled().await;
            pipeline_cancel.cancel();
        }
    });

    // Drain events and serialize.
    let events_task = tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let (event_type, payload) = event.serialize();
            if event_tx.send((event_type, payload)).await.is_err() {
                break;
            }
        }
    });

    // Map PipelineOutcome → TaskOutcome. Forward ALL outcomes (pipeline
    // may produce Suspended then Done after resume).
    let outcomes_task = tokio::spawn(async move {
        while let Some(outcome) = outcomes.recv().await {
            let is_terminal = matches!(
                outcome,
                PipelineOutcome::Done { .. }
                    | PipelineOutcome::Failed(_)
                    | PipelineOutcome::Cancelled
            );
            let task_outcome = match outcome {
                PipelineOutcome::Done { answer, metadata } => {
                    TaskOutcome::Done { answer, metadata }
                }
                PipelineOutcome::Suspended {
                    reason,
                    resume_data,
                    trace_id,
                } => TaskOutcome::Suspended {
                    reason,
                    resume_data,
                    trace_id,
                },
                PipelineOutcome::Failed(msg) => TaskOutcome::Failed(msg),
                PipelineOutcome::Cancelled => TaskOutcome::Cancelled,
            };
            if outcome_tx.send(task_outcome).await.is_err() {
                break;
            }
            if is_terminal {
                break;
            }
        }
    });

    tokio::spawn(async move {
        let _ = tokio::join!(events_task, outcomes_task);
    })
}

// ── Coordinator-based drive ─────────────────────────────────────────────────

/// Drive a pipeline using the coordinator-worker architecture.
///
/// This is the new path that supports agent delegation and workflow execution
/// as child tasks. It creates a [`LocalTransport`], [`Worker`], and
/// [`Coordinator`], wires them together, and runs the pipeline to completion.
///
/// Drop-in replacement for [`StartedPipeline::drive`] when delegation support
/// is needed.
pub async fn drive_with_coordinator(
    started: StartedPipeline,
    db: DatabaseConnection,
    state: Arc<RuntimeState>,
    answer_rx: mpsc::Receiver<String>,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
    platform: Arc<dyn PlatformContext>,
    builder_bridges: Option<BuilderBridges>,
    schema_cache: Option<Arc<Mutex<HashMap<String, agentic_analytics::SchemaCatalog>>>>,
    builder_test_runner: Option<Arc<dyn agentic_builder::BuilderTestRunner>>,
) {
    use agentic_core::transport::{CoordinatorTransport, WorkerTransport};
    use agentic_runtime::coordinator::Coordinator;
    use agentic_runtime::transport::DurableTransport;
    use agentic_runtime::worker::Worker;

    let run_id = started.run_id.clone();
    let _source_type = started.source_type.clone();

    // Convert the already-started pipeline into an ExecutingTask.
    let (executing_task, bridge_handle) = started.into_executing_task();

    // Create the durable transport backed by the task queue table.
    let transport = DurableTransport::new(db.clone());

    // Create the task executor for child tasks (delegation).
    let executor = Arc::new(executor::PipelineTaskExecutor {
        platform,
        builder_bridges,
        schema_cache,
        builder_test_runner,
        db: db.clone(),
        state: Some(state.clone()),
    });

    // Worker: handles task execution (including the initial root task).
    let worker = Worker::new(transport.clone() as Arc<dyn WorkerTransport>, executor);

    // Query the max existing event seq so the coordinator starts after it.
    // This is critical for cold resume — avoids seq conflicts with prior events.
    let root_next_seq = agentic_runtime::crud::get_max_seq(&db, &run_id)
        .await
        .unwrap_or(-1)
        + 1;

    // Coordinator: manages the task tree.
    let mut coordinator = Coordinator::new(
        db,
        state.clone(),
        transport.clone() as Arc<dyn CoordinatorTransport>,
    );
    coordinator.register_answer_channel(run_id.clone(), answer_rx);

    // For the root task, we already have an ExecutingTask from the started
    // pipeline. We need to feed its events and outcome into the coordinator
    // via the transport, then run the coordinator loop.
    //
    // Strategy: spawn a "virtual worker" that forwards the existing
    // ExecutingTask to the coordinator, then start the real worker for any
    // child tasks that may be spawned.
    let root_task_id = run_id.clone();

    // Forward cancellation from RuntimeState's cancel_tx to the coordinator
    // transport so the coordinator sees the root task as cancelled.
    let cancel_forwarder = {
        let transport_cancel = transport.clone();
        let cancel_task_id = root_task_id.clone();
        tokio::spawn(async move {
            // Wait for the cancel signal.
            while cancel_rx.changed().await.is_ok() {
                if *cancel_rx.borrow() {
                    tracing::info!(
                        target: "coordinator",
                        task_id = %cancel_task_id,
                        "cancel signal received, cancelling root task"
                    );
                    let _ = transport_cancel.cancel(&cancel_task_id).await;
                    break;
                }
            }
        })
    };

    let virtual_worker = {
        let transport_clone = transport.clone();
        let task_id = root_task_id.clone();
        tokio::spawn(async move {
            use agentic_core::transport::WorkerTransport;
            tracing::info!(target: "worker", task_id = %task_id, "virtual worker started");

            // Forward cancellation from transport to the executing task,
            // mirroring what Worker::handle_task does for child tasks.
            let cancel_token = transport_clone.cancellation_token(&task_id);
            let task_cancel = executing_task.cancel.clone();
            let _cancel_fwd = tokio::spawn({
                let task_id = task_id.clone();
                async move {
                    cancel_token.cancelled().await;
                    tracing::info!(target: "worker", task_id = %task_id, "cancellation forwarded to root task");
                    task_cancel.cancel();
                }
            });

            // Spawn heartbeat loop for the root task.
            let heartbeat_cancel = WorkerTransport::spawn_heartbeat(
                transport_clone.as_ref(),
                &task_id,
                std::time::Duration::from_secs(15),
            );

            let mut events = executing_task.events;
            let mut outcomes = executing_task.outcomes;

            // Forward events and outcomes concurrently.
            // Events and outcomes arrive on separate channels; we must
            // process both without blocking one on the other. The pipeline
            // may emit a Suspended outcome while the events channel is
            // still open (pipeline task holds the sender).
            loop {
                tokio::select! {
                    event = events.recv() => {
                        match event {
                            Some((event_type, payload)) => {
                                let _ = transport_clone
                                    .send(agentic_core::transport::WorkerMessage::Event {
                                        task_id: task_id.clone(),
                                        event_type,
                                        payload,
                                    })
                                    .await;
                            }
                            None => {
                                // Events channel closed — drain remaining outcomes.
                                while let Some(outcome) = outcomes.recv().await {
                                    let is_terminal = matches!(
                                        outcome,
                                        agentic_core::delegation::TaskOutcome::Done { .. }
                                            | agentic_core::delegation::TaskOutcome::Failed(_)
                                            | agentic_core::delegation::TaskOutcome::Cancelled
                                    );
                                    let _ = transport_clone
                                        .send(agentic_core::transport::WorkerMessage::Outcome {
                                            task_id: task_id.clone(),
                                            outcome,
                                        })
                                        .await;
                                    if is_terminal {
                                        heartbeat_cancel.cancel();
                                        return;
                                    }
                                }
                                heartbeat_cancel.cancel();
                                return;
                            }
                        }
                    }
                    outcome = outcomes.recv() => {
                        match outcome {
                            Some(outcome) => {
                                let outcome_type = match &outcome {
                                    agentic_core::delegation::TaskOutcome::Done { .. } => "Done",
                                    agentic_core::delegation::TaskOutcome::Suspended { .. } => "Suspended",
                                    agentic_core::delegation::TaskOutcome::Failed(_) => "Failed",
                                    agentic_core::delegation::TaskOutcome::Cancelled => "Cancelled",
                                };
                                tracing::info!(target: "worker", task_id = %task_id, outcome_type, "virtual worker forwarding outcome");
                                let is_terminal = matches!(
                                    outcome,
                                    agentic_core::delegation::TaskOutcome::Done { .. }
                                        | agentic_core::delegation::TaskOutcome::Failed(_)
                                        | agentic_core::delegation::TaskOutcome::Cancelled
                                );
                                let _ = transport_clone
                                    .send(agentic_core::transport::WorkerMessage::Outcome {
                                        task_id: task_id.clone(),
                                        outcome,
                                    })
                                    .await;
                                if is_terminal {
                                    // Drain remaining events before exiting.
                                    while let Ok(ev) = events.try_recv() {
                                        let _ = transport_clone
                                            .send(agentic_core::transport::WorkerMessage::Event {
                                                task_id: task_id.clone(),
                                                event_type: ev.0,
                                                payload: ev.1,
                                            })
                                            .await;
                                    }
                                    heartbeat_cancel.cancel();
                                    return;
                                }
                            }
                            None => {
                                // Outcome channel closed — drain remaining events
                                // so late-arriving events (e.g. awaiting_input
                                // emitted just before the Suspended outcome) are
                                // not lost.
                                while let Some(ev) = events.recv().await {
                                    let _ = transport_clone
                                        .send(agentic_core::transport::WorkerMessage::Event {
                                            task_id: task_id.clone(),
                                            event_type: ev.0,
                                            payload: ev.1,
                                        })
                                        .await;
                                }
                                heartbeat_cancel.cancel();
                                return;
                            }
                        }
                    }
                }
            }
        })
    };

    // Register the root task in the coordinator (already running via virtual worker).
    coordinator.register_root(run_id.clone(), root_next_seq);

    // Spawn the worker for child tasks.
    let child_worker = tokio::spawn(async move {
        worker.run().await;
    });

    // Run the coordinator (blocks until all tasks complete).
    tracing::info!(target: "coordinator", run_id = %run_id, "drive_with_coordinator: starting coordinator loop");
    coordinator.run().await;
    tracing::info!(target: "coordinator", run_id = %run_id, "drive_with_coordinator: coordinator loop finished");

    // Wait for the bridge task to flush remaining events (including `done`)
    // to the DB before notifying subscribers and deregistering. Bounded by a
    // timeout in case a producer keeps a sender open past the terminal
    // outcome.
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), bridge_handle).await;
    state.notify(&run_id);

    // Coordinator has exited and bridge has flushed: abort background tasks
    // that would otherwise linger (cancel forwarder still watching cancel_rx,
    // virtual worker still waiting on a closed channel, child worker still
    // polling the transport).
    cancel_forwarder.abort();
    virtual_worker.abort();
    child_worker.abort();

    // Clean up.
    state.deregister(&run_id);
}

// ── Event registry construction ─────────────────────────────────────────────

/// Build an [`EventRegistry`] with all known domain handlers pre-registered.
///
/// Call this once at startup — both HTTP server and CLI use the same registry.
pub fn build_event_registry() -> EventRegistry {
    let mut registry = EventRegistry::new();
    registry.register("analytics", agentic_analytics::event_handler());
    registry.register("builder", agentic_builder::event_handler());
    registry
}

// ── Domain-specific CRUD facades ────────────────────────────────────────────

/// Update a completed run's answer + analytics spec_hint extension.
///
/// Wraps `runtime::crud::update_run_done` + analytics extension update.
pub async fn update_run_done(
    db: &DatabaseConnection,
    run_id: &str,
    answer: &str,
    spec_hint: Option<serde_json::Value>,
) -> Result<(), sea_orm::DbErr> {
    agentic_runtime::crud::update_run_done(db, run_id, answer, None).await?;
    if let Some(hint) = spec_hint {
        agentic_analytics::update_run_spec_hint(db, run_id, hint).await?;
    }
    Ok(())
}

/// Update thinking_mode on the analytics extension table.
pub async fn update_run_thinking_mode(
    db: &DatabaseConnection,
    run_id: &str,
    thinking_mode: Option<String>,
) -> Result<(), sea_orm::DbErr> {
    agentic_analytics::update_run_thinking_mode(db, run_id, thinking_mode).await
}

/// Insert a run record + analytics extension (for non-builder runs).
///
/// Wraps `runtime::crud::insert_run` + analytics extension insert.
pub async fn insert_run(
    db: &DatabaseConnection,
    run_id: &str,
    agent_id: &str,
    question: &str,
    thread_id: Option<Uuid>,
    thinking_mode: Option<String>,
) -> Result<(), sea_orm::DbErr> {
    let source_type = if agent_id == "__builder__" {
        "builder"
    } else {
        "analytics"
    };
    let metadata = serde_json::json!({
        "agent_id": agent_id,
        "thinking_mode": thinking_mode,
    });
    agentic_runtime::crud::insert_run(db, run_id, question, thread_id, source_type, Some(metadata))
        .await?;

    if agent_id != "__builder__" {
        agentic_analytics::insert_run_meta(db, run_id, agent_id, thinking_mode).await?;
    }
    Ok(())
}

/// Get analytics extensions for a list of run IDs (bulk fetch).
pub async fn get_analytics_extensions(
    db: &DatabaseConnection,
    run_ids: &[String],
) -> Result<Vec<AnalyticsRunMeta>, sea_orm::DbErr> {
    agentic_analytics::get_run_metas(db, run_ids).await
}

/// Get a single analytics extension by run ID.
pub async fn get_analytics_extension(
    db: &DatabaseConnection,
    run_id: &str,
) -> Result<Option<AnalyticsRunMeta>, sea_orm::DbErr> {
    agentic_analytics::get_run_meta(db, run_id).await
}

/// Thread history turn with analytics-specific `spec_hint`.
pub struct ThreadHistoryTurn {
    pub question: String,
    pub answer: String,
    pub spec_hint: Option<serde_json::Value>,
}

/// Return completed runs for a thread with spec_hint (analytics-specific).
pub async fn get_thread_history(
    db: &DatabaseConnection,
    thread_id: Uuid,
    limit: u64,
) -> Result<Vec<ThreadHistoryTurn>, sea_orm::DbErr> {
    // Use runtime CRUD instead of querying the entity directly.
    let runs = agentic_runtime::crud::get_runs_by_thread(db, thread_id).await?;
    let completed: Vec<_> = runs
        .into_iter()
        .filter(|r| {
            matches!(
                r.task_status.as_deref(),
                Some("done") | Some("failed") | Some("cancelled") | Some("timed_out")
            )
        })
        .take(limit as usize)
        .collect();

    let run_ids: Vec<String> = completed.iter().map(|r| r.id.clone()).collect();
    let metas = agentic_analytics::get_run_metas(db, &run_ids).await?;
    let hint_map: std::collections::HashMap<String, serde_json::Value> = metas
        .into_iter()
        .filter_map(|m| m.spec_hint.map(|h| (m.run_id, h)))
        .collect();

    Ok(completed
        .into_iter()
        .filter_map(|r| {
            let spec_hint = hint_map.get(&r.id).cloned();
            let answer = render_history_answer(
                r.task_status.as_deref(),
                r.answer.as_deref(),
                r.error_message.as_deref(),
            )?;
            Some(ThreadHistoryTurn {
                question: r.question,
                answer,
                spec_hint,
            })
        })
        .collect())
}

fn render_history_answer(
    task_status: Option<&str>,
    answer: Option<&str>,
    error_message: Option<&str>,
) -> Option<String> {
    if let Some(ans) = answer {
        return Some(ans.to_string());
    }
    match task_status {
        Some("failed") | Some("timed_out") => {
            Some(format!("Error: {}", error_message.unwrap_or("run failed")))
        }
        Some("cancelled") => Some(
            error_message
                .map(|m| format!("Cancelled: {m}"))
                .unwrap_or_else(|| "Cancelled by user".to_string()),
        ),
        Some("done") => error_message.map(|e| format!("Error: {e}")),
        _ => None,
    }
}

// ── Headless eval entry-point ───────────────────────────────────────────────

/// Run an agentic analytics pipeline headlessly for evaluation purposes.
///
/// Returns the answer text, or a human-readable error string if the
/// pipeline suspends / fails. The caller is expected to lift the error
/// back into its own error type (`OxyError` for the eval runner).
pub async fn run_agentic_eval(
    platform: Arc<dyn PlatformContext>,
    config_path: &std::path::Path,
    prompt: String,
) -> Result<String, String> {
    use agentic_analytics::{
        AnalyticsEvent, AnalyticsIntent, QuestionType, build_analytics_handlers,
        config::BuildContext,
    };
    use agentic_core::events::{Event, EventStream};
    use agentic_core::orchestrator::{Orchestrator, OrchestratorError};

    let base_dir = platform.workspace_path().to_path_buf();

    let config = AgentConfig::from_file(config_path).map_err(|e| {
        format!(
            "failed to load agentic config at {}: {e}",
            config_path.display()
        )
    })?;

    let mut ctx = BuildContext::default();
    ctx.project_model_info = platform
        .resolve_model(config.llm.model_ref.as_deref(), config.llm.model.is_some())
        .await;

    let mut effective_databases: Vec<String> = config.databases.clone();
    if let Ok(resolved) = config.resolve_context(&base_dir) {
        for db_name in resolved.referenced_databases {
            if !effective_databases.contains(&db_name) {
                effective_databases.push(db_name);
            }
        }
    }
    let connector_configs = platform::resolve_connectors(&effective_databases, &*platform).await;
    let connectors = agentic_connector::build_named_connectors(connector_configs).await;
    ctx.extra_default_connector = effective_databases
        .iter()
        .find(|name| connectors.contains_key(*name))
        .cloned();
    ctx.extra_connectors = connectors;

    let (solver, procedure_files) = config
        .build_solver_with_context(&base_dir, ctx)
        .await
        .map_err(|e| format!("failed to build agentic solver: {e}"))?;

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<Event<AnalyticsEvent>>(256);
    let event_stream: EventStream<AnalyticsEvent> = event_tx;
    tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    let solver = solver.with_events(event_stream.clone());
    let solver = {
        let workspace: Arc<dyn agentic_workflow::WorkspaceContext> = platform.clone();
        let runner = agentic_workflow::OxyProcedureRunner::new(workspace)
            .with_procedure_files(procedure_files);
        solver.with_procedure_runner(std::sync::Arc::new(runner))
    };

    let mut orchestrator = Orchestrator::new(solver).with_handlers(build_analytics_handlers());

    let intent = AnalyticsIntent {
        raw_question: prompt,
        summary: String::new(),
        question_type: QuestionType::SingleValue,
        metrics: vec![],
        dimensions: vec![],
        filters: vec![],
        history: vec![],
        spec_hint: None,
        selected_procedure: None,
        semantic_query: Default::default(),
        semantic_confidence: 0.0,
    };

    orchestrator
        .run(intent)
        .await
        .map(|answer| answer.text)
        .map_err(|e| match e {
            OrchestratorError::Suspended { reason, .. } => {
                let questions = match reason {
                    agentic_core::SuspendReason::HumanInput { questions } => questions,
                    _ => vec![],
                };
                let prompts: Vec<_> = questions.iter().map(|q| q.prompt.as_str()).collect();
                format!(
                    "agentic pipeline asked a clarifying question during eval: {}",
                    prompts.join("; ")
                )
            }
            OrchestratorError::MaxIterationsExceeded => "max iterations exceeded".into(),
            OrchestratorError::ResumeNotSupported => "resume not supported".into(),
            OrchestratorError::Fatal(e) => format!("fatal: {e:?}"),
        })
}
