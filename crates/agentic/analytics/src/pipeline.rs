//! Single-function facade for running an analytics pipeline.
//!
//! [`start_pipeline`] builds the solver, orchestrator, and event channels
//! internally.  The caller interacts only through [`PipelineHandle`] —
//! receiving events, sending HITL answers, and cancelling.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use agentic_connector::DatabaseConnector;
use agentic_core::events::{CoreEvent, Event, EventStream};
use agentic_core::human_input::SuspendedRunData;
use agentic_core::orchestrator::{Orchestrator, OrchestratorError};
use agentic_runtime::handle::{PipelineHandle, PipelineOutcome};

use crate::catalog::SchemaCatalog;
use crate::config::{AgentConfig, BuildContext, ConfigError, ResolvedModelInfo};
use crate::events::AnalyticsEvent;
use crate::metric_sink::SharedMetricSink;
use crate::procedure::ProcedureRunner;
use crate::solver::build_analytics_handlers;
use crate::types::{AnalyticsIntent, ConversationTurn, QuestionType, SpecHint};

// ── Public types ─────────────────────────────────────────────────────────────

/// Everything needed to start an analytics pipeline.
pub struct PipelineParams {
    pub config: AgentConfig,
    pub base_dir: PathBuf,
    /// Identifier of the agentic config used for this run. Surfaced on
    /// the root span as `oxy.agent.ref` and used as the `source_ref`
    /// when recording metric usage, so Clusters / Metrics / Execution
    /// Analytics tabs can attribute activity to a specific agent.
    pub agent_id: String,
    pub connectors: HashMap<String, Arc<dyn DatabaseConnector>>,
    pub default_connector: Option<String>,
    pub question: String,
    pub history: Vec<ConversationTurn>,
    pub prior_spec_hint: Option<SpecHint>,
    pub schema_cache: Option<Arc<Mutex<HashMap<String, SchemaCatalog>>>>,
    pub project_model: Option<ResolvedModelInfo>,
    /// When `true`, thinking override and model override are derived from
    /// `config.llm.extended_thinking`.  The caller no longer needs to extract
    /// these values manually.
    pub use_extended_thinking: bool,
    /// Optional procedure search provider for the `search_procedures` tool.
    /// Procedure *execution* is handled by the coordinator, not this runner.
    pub procedure_runner: Option<Arc<dyn ProcedureRunner>>,
    /// Optional sink for recording Tier 1 metric usage (measures +
    /// dimensions) to an external observability backend. `None` means
    /// metrics won't be recorded — the pipeline still runs.
    pub metric_sink: Option<SharedMetricSink>,
}

// ── start_pipeline ───────────────────────────────────────────────────────────

/// Build solver + orchestrator and run the analytics pipeline.
///
/// Returns a [`PipelineHandle`] immediately.  The pipeline runs in a spawned
/// task.  The caller reads events and outcomes from the handle's channels.
pub async fn start_pipeline(
    params: PipelineParams,
) -> Result<PipelineHandle<AnalyticsEvent>, ConfigError> {
    // Root span for this run. `parent: None` detaches it from the HTTP
    // request span so the UI's `ParentSpanId = ''` filter picks it up, and
    // child solver-stage spans (instrumented separately) chain under it.
    //
    // `agent.prompt` and `oxy.agent.ref` mirror the classic-agent convention
    // so the observability backends (Clusters intent classifier, Execution
    // Analytics join predicate, source-type attribution) can treat
    // `analytics.run` and `agent.run_agent` uniformly.
    let run_span = tracing::info_span!(
        parent: None,
        "analytics.run",
        oxy.name = "analytics.run",
        oxy.span_type = "analytics",
        oxy.agent.ref = %params.agent_id,
        agent.prompt = %params.question,
        question = %params.question,
    );
    // Derive thinking/model overrides from config when extended thinking is requested.
    let (thinking_override, model_override) = if params.use_extended_thinking {
        params
            .config
            .llm
            .extended_thinking
            .as_ref()
            .map_or((None, None), |ext| {
                (
                    ext.thinking.as_ref().map(|t| t.to_thinking_config()),
                    ext.model.clone(),
                )
            })
    } else {
        (None, None)
    };

    let build_ctx = BuildContext {
        extra_connectors: params.connectors,
        extra_default_connector: params.default_connector,
        project_model_info: params.project_model,
        schema_cache: params.schema_cache,
        thinking_override,
        model_override,
    };

    let (solver, _procedure_files) = params
        .config
        .build_solver_with_context(&params.base_dir, build_ctx)
        .await?;

    let (event_tx, event_rx) = mpsc::channel::<Event<AnalyticsEvent>>(256);
    let event_stream: EventStream<AnalyticsEvent> = event_tx;

    let cancel_event_tx = event_stream.clone();

    let solver = solver
        .with_events(event_stream.clone())
        .with_source_attribution(params.agent_id.clone(), params.question.clone())
        .with_metric_sink(params.metric_sink.clone());

    let solver = if let Some(runner) = params.procedure_runner {
        solver.with_procedure_runner(runner)
    } else {
        solver
    };

    let (outcome_tx, outcome_rx) = mpsc::channel::<PipelineOutcome>(4);
    let cancel = CancellationToken::new();
    let cancel_child = cancel.clone();

    let mut orchestrator = Orchestrator::new(solver)
        .with_handlers(build_analytics_handlers())
        .with_events(event_stream);

    let initial_intent = AnalyticsIntent {
        raw_question: params.question,
        summary: String::new(),
        question_type: QuestionType::SingleValue,
        metrics: vec![],
        dimensions: vec![],
        filters: vec![],
        history: params.history,
        spec_hint: params.prior_spec_hint,
        selected_procedure: None,
        semantic_query: Default::default(),
        semantic_confidence: 0.0,
    };

    let join = tokio::spawn(
        async move {
            let result = tokio::select! {
                r = orchestrator.run(initial_intent) => Some(r),
                _ = cancel_child.cancelled() => None,
            };

            let outcome = match result {
                Some(Ok(answer)) => {
                    let metadata = answer
                        .spec_hint
                        .as_ref()
                        .and_then(|h| serde_json::to_value(h).ok());
                    PipelineOutcome::Done {
                        answer: answer.text,
                        metadata,
                    }
                }
                Some(Err(OrchestratorError::Suspended {
                    reason,
                    resume_data,
                    trace_id,
                    ..
                })) => PipelineOutcome::Suspended {
                    reason,
                    resume_data,
                    trace_id,
                },
                Some(Err(OrchestratorError::Fatal(e))) => {
                    PipelineOutcome::Failed(format!("fatal: {e:?}"))
                }
                Some(Err(OrchestratorError::MaxIterationsExceeded)) => {
                    PipelineOutcome::Failed("max iterations exceeded".into())
                }
                Some(Err(OrchestratorError::ResumeNotSupported)) => {
                    PipelineOutcome::Failed("resume not supported".into())
                }
                None => {
                    let _ = cancel_event_tx
                        .send(Event::Core(CoreEvent::Error {
                            message: "cancelled by user".into(),
                            trace_id: "".into(),
                        }))
                        .await;
                    PipelineOutcome::Cancelled
                }
            };

            drop(orchestrator);
            drop(cancel_event_tx);
            let _ = outcome_tx.send(outcome).await;
        }
        .instrument(run_span),
    );

    Ok(PipelineHandle {
        events: event_rx,
        outcomes: outcome_rx,
        cancel,
        join,
    })
}

// ── resume_pipeline ─────────────────────────────────────────────────────────

/// Rebuild the solver + orchestrator and resume a previously suspended run.
///
/// The pipeline is fully reconstructed (connectors, LLM clients, etc.) and
/// then `orchestrator.resume(resume_data, answer)` is called instead of
/// `orchestrator.run(intent)`. Any further suspension/resume cycles are
/// handled in the same loop as `start_pipeline`.
pub async fn resume_pipeline(
    params: PipelineParams,
    resume_data: SuspendedRunData,
    answer: String,
) -> Result<PipelineHandle<AnalyticsEvent>, ConfigError> {
    let run_span = tracing::info_span!(
        parent: None,
        "analytics.run",
        oxy.name = "analytics.run",
        oxy.span_type = "analytics",
        oxy.agent.ref = %params.agent_id,
        agent.prompt = %params.question,
        question = %params.question,
        resumed = true,
    );
    // Derive thinking/model overrides from config when extended thinking is requested.
    let (thinking_override, model_override) = if params.use_extended_thinking {
        params
            .config
            .llm
            .extended_thinking
            .as_ref()
            .map_or((None, None), |ext| {
                (
                    ext.thinking.as_ref().map(|t| t.to_thinking_config()),
                    ext.model.clone(),
                )
            })
    } else {
        (None, None)
    };

    let build_ctx = BuildContext {
        extra_connectors: params.connectors,
        extra_default_connector: params.default_connector,
        project_model_info: params.project_model,
        schema_cache: params.schema_cache,
        thinking_override,
        model_override,
    };

    let (solver, _procedure_files) = params
        .config
        .build_solver_with_context(&params.base_dir, build_ctx)
        .await?;

    let (event_tx, event_rx) = mpsc::channel::<Event<AnalyticsEvent>>(256);
    let event_stream: EventStream<AnalyticsEvent> = event_tx;

    let cancel_event_tx = event_stream.clone();

    let solver = solver
        .with_events(event_stream.clone())
        .with_source_attribution(params.agent_id.clone(), params.question.clone())
        .with_metric_sink(params.metric_sink.clone());

    let solver = if let Some(runner) = params.procedure_runner {
        solver.with_procedure_runner(runner)
    } else {
        solver
    };

    let (outcome_tx, outcome_rx) = mpsc::channel::<PipelineOutcome>(4);
    let cancel = CancellationToken::new();
    let cancel_child = cancel.clone();

    let mut orchestrator = Orchestrator::new(solver)
        .with_handlers(build_analytics_handlers())
        .with_events(event_stream);

    let join = tokio::spawn(
        async move {
            // Resume from the suspended state instead of running from scratch.
            let result = tokio::select! {
                r = orchestrator.resume(resume_data, answer) => Some(r),
                _ = cancel_child.cancelled() => None,
            };

            let outcome = match result {
                Some(Ok(answer)) => {
                    let metadata = answer
                        .spec_hint
                        .as_ref()
                        .and_then(|h| serde_json::to_value(h).ok());
                    PipelineOutcome::Done {
                        answer: answer.text,
                        metadata,
                    }
                }
                Some(Err(OrchestratorError::Suspended {
                    reason,
                    resume_data,
                    trace_id,
                    ..
                })) => PipelineOutcome::Suspended {
                    reason,
                    resume_data,
                    trace_id,
                },
                Some(Err(OrchestratorError::Fatal(e))) => {
                    PipelineOutcome::Failed(format!("fatal: {e:?}"))
                }
                Some(Err(OrchestratorError::MaxIterationsExceeded)) => {
                    PipelineOutcome::Failed("max iterations exceeded".into())
                }
                Some(Err(OrchestratorError::ResumeNotSupported)) => {
                    PipelineOutcome::Failed("resume not supported".into())
                }
                None => {
                    let _ = cancel_event_tx
                        .send(Event::Core(CoreEvent::Error {
                            message: "cancelled by user".into(),
                            trace_id: "".into(),
                        }))
                        .await;
                    PipelineOutcome::Cancelled
                }
            };

            drop(orchestrator);
            drop(cancel_event_tx);
            let _ = outcome_tx.send(outcome).await;
        }
        .instrument(run_span),
    );

    Ok(PipelineHandle {
        events: event_rx,
        outcomes: outcome_rx,
        cancel,
        join,
    })
}
