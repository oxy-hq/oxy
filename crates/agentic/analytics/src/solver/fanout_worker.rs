//! [`AnalyticsFanoutWorker`] — concurrent solve+execute for a single sub-spec.

use std::collections::HashMap;
use std::sync::Arc;

use agentic_connector::DatabaseConnector;
use agentic_core::{
    BackTarget,
    events::{CoreEvent, DomainEvents, EventStream, Outcome},
    orchestrator::{RunContext, SessionMemory},
    result::CellValue,
    solver::FanoutWorker,
};
use async_trait::async_trait;
use tracing::Instrument;

use crate::config::StateConfig;
use crate::events::{AnalyticsEvent, QuerySource};
use crate::llm::{InitialMessages, LlmClient, ThinkingConfig, ToolLoopConfig};
use crate::metric_sink::SharedMetricSink;
use crate::schemas::solve_response_schema;
use crate::semantic::SemanticCatalog;
use crate::solver::executing::execution_type_for;
use crate::tools::execute_solving_tool;
use crate::types::{SolutionPayload, SolutionSource};
use crate::{AnalyticsDomain, AnalyticsError, AnalyticsResult, AnalyticsSolution, QuerySpec};

use super::AnalyticsSolver;
use super::prompts::QUESTION_TYPE_DEFS;

/// [`QuerySpec`] independently of other workers.
///
/// Holds only shared (`Arc`-wrapped) or `Clone` state cloned from the parent
/// [`AnalyticsSolver`].
pub struct AnalyticsFanoutWorker {
    client: LlmClient,
    #[allow(dead_code)]
    catalog: Arc<SemanticCatalog>,
    connectors: HashMap<String, Arc<dyn DatabaseConnector>>,
    default_connector: String,
    event_tx: Option<EventStream<AnalyticsEvent>>,
    global_instructions: Option<String>,
    sql_examples: Vec<String>,
    state_configs: HashMap<String, StateConfig>,
    state_clients: HashMap<String, LlmClient>,
    global_thinking: Option<ThinkingConfig>,
    extended_thinking_active: bool,
    max_tokens: Option<u32>,
    /// Source attribution for observability — mirrors the parent solver so
    /// fan-out sub-queries land in the Metrics and Execution Analytics tabs.
    agent_id: String,
    question: String,
    /// Handle to the analytics run's root span, captured at worker
    /// construction time (`from_solver`), when the orchestrator is still
    /// inside the `run_span` context. Used to explicitly parent the
    /// `analytics.tool_call` child span so it inherits `analytics.run`'s
    /// `trace_id` even when the outer `.instrument(sub_span)` wrapper is
    /// not in the current thread's span stack at `info_span!` time.
    run_span: tracing::Span,
    /// Sink for Tier 1 metric recording, mirrored from the parent solver.
    metric_sink: Option<SharedMetricSink>,
}

impl AnalyticsFanoutWorker {
    /// Construct a worker from the parent solver's shared state.
    pub fn from_solver(solver: &AnalyticsSolver) -> Self {
        Self {
            client: solver.client.clone(),
            catalog: Arc::clone(&solver.catalog),
            connectors: solver.connectors.clone(),
            default_connector: solver.default_connector.clone(),
            event_tx: solver.event_tx.clone(),
            global_instructions: solver.global_instructions.clone(),
            sql_examples: solver.sql_examples.clone(),
            state_configs: solver.state_configs.clone(),
            state_clients: solver.state_clients.clone(),
            global_thinking: solver.global_thinking.clone(),
            extended_thinking_active: solver.extended_thinking_active,
            max_tokens: solver.max_tokens,
            agent_id: solver.agent_id.clone(),
            question: solver.question.clone(),
            // Captured while the orchestrator is still running inside the
            // `run_span` context — by the time this worker is spawned into a
            // tokio task, the thread-local span stack may have dropped it.
            run_span: tracing::Span::current(),
            metric_sink: solver.metric_sink.clone(),
        }
    }

    /// Return the LLM client for `state`, falling back to the global client.
    fn client_for_state(&self, state: &str) -> &LlmClient {
        self.state_clients.get(state).unwrap_or(&self.client)
    }

    /// Build a composite system prompt (mirrors `AnalyticsSolver::build_system_prompt`).
    fn build_system_prompt(&self, state: &str, base: &str, dialect: Option<&str>) -> String {
        let mut parts = vec![base.to_string()];

        match state {
            "clarifying" | "specifying" => {
                parts.push(QUESTION_TYPE_DEFS.to_string());
            }
            _ => {}
        }

        if let Some(global) = &self.global_instructions
            && !global.trim().is_empty()
        {
            parts.push(format!(
                "<global_instructions>\n{}\n</global_instructions>",
                global.trim()
            ));
        }

        if let Some(state_cfg) = self.state_configs.get(state)
            && let Some(state_instr) = &state_cfg.instructions
            && !state_instr.trim().is_empty()
        {
            parts.push(format!(
                "<state_instructions>\n{}\n</state_instructions>",
                state_instr.trim()
            ));
        }

        match state {
            "specifying" | "solving" => {
                if let Some(d) = dialect {
                    parts.push(format!(
                        "<sql_dialect>{d} — use {d}-compatible syntax, \
                         including its identifier quoting and built-in functions.</sql_dialect>"
                    ));
                }
                if state == "solving" && !self.sql_examples.is_empty() {
                    let examples: Vec<String> = self
                        .sql_examples
                        .iter()
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| format!("<sql_example>\n{}\n</sql_example>", s.trim()))
                        .collect();
                    if !examples.is_empty() {
                        parts.push(format!(
                            "<sql_examples>\n{}\n</sql_examples>",
                            examples.join("\n")
                        ));
                    }
                }
            }
            _ => {}
        }

        parts.join("\n\n")
    }

    /// Return the [`ThinkingConfig`] for a state.
    fn thinking_for_state(&self, state: &str, default: ThinkingConfig) -> ThinkingConfig {
        // Extended thinking override takes absolute precedence.
        if self.extended_thinking_active
            && let Some(global) = &self.global_thinking
        {
            return global.clone();
        }
        if let Some(t) = self
            .state_configs
            .get(state)
            .and_then(|c| c.thinking.as_ref())
            .map(|t| t.to_thinking_config())
        {
            return t;
        }
        if let Some(global) = &self.global_thinking {
            return global.clone();
        }
        default
    }

    /// Return the max tool rounds for a state, falling back to `default`.
    fn max_tool_rounds_for_state(&self, state: &str, default: u32) -> u32 {
        self.state_configs
            .get(state)
            .and_then(|c| c.max_retries)
            .unwrap_or(default)
    }
}

#[async_trait]
impl<Ev: DomainEvents> FanoutWorker<AnalyticsDomain, Ev> for AnalyticsFanoutWorker {
    async fn solve_and_execute(
        &self,
        spec: QuerySpec,
        index: usize,
        _total: usize,
        _events: &Option<EventStream<Ev>>,
        _ctx: &RunContext<AnalyticsDomain>,
        _mem: &SessionMemory<AnalyticsDomain>,
    ) -> Result<AnalyticsResult, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        let sub = Some(index);
        // Use the worker's own event stream (typed as AnalyticsEvent) rather
        // than the generic `_events` parameter.
        let tx = &self.event_tx;

        // ── Check for pre-computed skip (SemanticLayer / SqlFile / Procedure / VendorEngine) ──
        let solution = match &spec.solution_source {
            SolutionSource::SemanticLayer => {
                if let Some(payload) = spec.precomputed.clone() {
                    Some(AnalyticsSolution {
                        payload,
                        solution_source: SolutionSource::SemanticLayer,
                        connector_name: spec.connector_name.clone(),
                        semantic_query: spec.query_request_item.clone(),
                    })
                } else {
                    None
                }
            }
            // SqlFile: precomputed SQL is always set at Specifying time.
            SolutionSource::SqlFile { .. } => {
                spec.precomputed.clone().map(|payload| AnalyticsSolution {
                    payload,
                    solution_source: spec.solution_source.clone(),
                    connector_name: spec.connector_name.clone(),
                    semantic_query: None,
                })
            }
            SolutionSource::Procedure { file_path } => Some(AnalyticsSolution {
                payload: SolutionPayload::Sql(String::new()),
                solution_source: SolutionSource::Procedure {
                    file_path: file_path.clone(),
                },
                connector_name: spec.connector_name.clone(),
                semantic_query: None,
            }),
            SolutionSource::VendorEngine(_) => {
                if let Some(payload) = spec.precomputed.clone() {
                    Some(AnalyticsSolution {
                        payload,
                        solution_source: spec.solution_source.clone(),
                        connector_name: spec.connector_name.clone(),
                        semantic_query: None,
                    })
                } else {
                    None
                }
            }
            SolutionSource::LlmWithSemanticContext => None,
        };

        // ── Solve (unless skipped) ──────────────────────────────────────────────
        let solution = if let Some(s) = solution {
            s
        } else {
            // StateEnter: solving
            super::emit_core(
                tx,
                CoreEvent::StateEnter {
                    state: "solving".into(),
                    revision: 0,
                    trace_id: String::new(),
                    sub_spec_index: sub,
                },
            )
            .await;

            let result = self.solve_spec(&spec, sub).await;

            let outcome = if result.is_ok() {
                Outcome::Advanced
            } else {
                Outcome::Failed
            };
            super::emit_core(
                tx,
                CoreEvent::StateExit {
                    state: "solving".into(),
                    outcome,
                    trace_id: String::new(),
                    sub_spec_index: sub,
                },
            )
            .await;

            result?
        };

        // ── Execute ─────────────────────────────────────────────────────────────
        super::emit_core(
            tx,
            CoreEvent::StateEnter {
                state: "executing".into(),
                revision: 0,
                trace_id: String::new(),
                sub_spec_index: sub,
            },
        )
        .await;

        let result = self.execute_solution(&solution, sub).await;

        let outcome = if result.is_ok() {
            Outcome::Advanced
        } else {
            Outcome::Failed
        };
        super::emit_core(
            tx,
            CoreEvent::StateExit {
                state: "executing".into(),
                outcome,
                trace_id: String::new(),
                sub_spec_index: sub,
            },
        )
        .await;

        result
    }
}

impl AnalyticsFanoutWorker {
    /// Core solve logic — shared-ref version of `AnalyticsSolver::solve_impl`.
    async fn solve_spec(
        &self,
        spec: &QuerySpec,
        sub_spec_index: Option<usize>,
    ) -> Result<AnalyticsSolution, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        use super::prompts::{SOLVE_BASE_PROMPT, solve_type_addendum};
        use super::solving::build_solve_user_prompt;
        use super::strip_json_fences;

        let user_prompt = build_solve_user_prompt(spec, None);
        let initial = InitialMessages::User(user_prompt);

        let tools = AnalyticsSolver::tools_for_state_solving();
        let type_addendum = solve_type_addendum(&spec.intent.question_type);
        let solve_prompt = format!("{SOLVE_BASE_PROMPT}{type_addendum}");
        let solve_dialect = self
            .connectors
            .get(&spec.connector_name)
            .map(|c| c.dialect().as_str());
        let system_prompt = self.build_system_prompt("solving", &solve_prompt, solve_dialect);
        let thinking = self.thinking_for_state("solving", ThinkingConfig::Adaptive);
        let max_rounds = self.max_tool_rounds_for_state("solving", 3);
        let connector = self
            .connectors
            .get(&spec.connector_name)
            .cloned()
            .expect("connector for spec must be registered");

        let output = match self
            .client_for_state("solving")
            .run_with_tools(
                &system_prompt,
                initial,
                &tools,
                |name: String, params| {
                    let connector = Arc::clone(&connector);
                    Box::pin(async move {
                        execute_solving_tool(&name, params, &*connector)
                            .await
                            .map(|v| Box::new(v) as Box<dyn agentic_core::tools::ToolOutput>)
                    })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: max_rounds,
                    state: "solving".into(),
                    thinking,
                    response_schema: Some(solve_response_schema()),
                    max_tokens_override: self.max_tokens,
                    sub_spec_index,
                    system_date_hint: Some(AnalyticsSolver::current_date_hint()),
                },
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("LLM call failed during solve: {e}");
                return Err((
                    AnalyticsError::NeedsUserInput { prompt: msg },
                    BackTarget::Solve(spec.clone(), Default::default()),
                ));
            }
        };

        let sql = if let Some(structured) = output.structured_response {
            structured["sql"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string()
        } else {
            strip_json_fences(&output.text).trim().to_string()
        };

        super::emit_domain(
            &self.event_tx,
            AnalyticsEvent::QueryGenerated {
                sql: sql.clone(),
                sub_spec_index,
            },
        )
        .await;

        let solution_source = spec.solution_source.clone();
        let connector_name = spec.connector_name.clone();
        let semantic_query = matches!(solution_source, SolutionSource::SemanticLayer)
            .then(|| spec.query_request_item.clone())
            .flatten();
        Ok(AnalyticsSolution {
            payload: SolutionPayload::Sql(sql),
            solution_source,
            connector_name,
            semantic_query,
        })
    }

    /// Execute a solution against the appropriate connector.
    ///
    /// Shared-ref version of `AnalyticsSolver::execute_solution`, limited to
    /// the `SolutionPayload::Sql` path (fan-out specs are always SQL).
    async fn execute_solution(
        &self,
        solution: &AnalyticsSolution,
        sub_spec_index: Option<usize>,
    ) -> Result<AnalyticsResult, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        const DEFAULT_SAMPLE_LIMIT: u64 = 1_000;
        let start = std::time::Instant::now();

        let query_source = match &solution.solution_source {
            SolutionSource::SemanticLayer => QuerySource::Semantic,
            SolutionSource::SqlFile { .. } => QuerySource::VerifiedSql,
            SolutionSource::VendorEngine(_) => QuerySource::Vendor,
            // NOTE: `Procedure` solutions *can* reach this fan-out worker (unlike the
            // serial `execute_solution` in executing.rs, which intercepts them first).
            // Badging them as `Llm` is imprecise — user-authored YAML procedures are
            // not LLM-generated SQL. Revisit by introducing a dedicated
            // `QuerySource::UserDefined` variant if/when procedure provenance needs to
            // surface distinctly in the UI.
            SolutionSource::LlmWithSemanticContext | SolutionSource::Procedure { .. } => {
                QuerySource::Llm
            }
        };

        let sql = match &solution.payload {
            SolutionPayload::Sql(sql) => sql.clone(),
            SolutionPayload::Vendor(_) => {
                // Vendor path is not supported in fan-out; fall through to error.
                return Err((
                    AnalyticsError::NeedsUserInput {
                        prompt: "Vendor path not supported in fan-out worker".into(),
                    },
                    BackTarget::Execute(solution.clone(), Default::default()),
                ));
            }
        };

        let connector = self
            .connectors
            .get(&solution.connector_name)
            .or_else(|| self.connectors.get(&self.default_connector))
            .or_else(|| self.connectors.values().next())
            .expect("AnalyticsFanoutWorker must have at least one connector")
            .clone();

        // Child `tool_call` span — identical shape to the serial path so
        // fan-out sub-queries appear in the Execution Analytics tab.
        // Parent is pinned to `self.run_span` so the span's `trace_id`
        // always inherits `analytics.run`'s, independent of whatever the
        // current tokio worker thread's span stack happens to look like.
        let (execution_type, is_verified) = execution_type_for(&solution.solution_source);
        let tool_span = tracing::info_span!(
            parent: &self.run_span,
            "analytics.tool_call",
            oxy.name = "analytics.tool_call",
            oxy.span_type = "tool_call",
            oxy.execution_type = execution_type,
            oxy.is_verified = is_verified,
            connector = %solution.connector_name,
            sub_spec_index = sub_spec_index.unwrap_or(usize::MAX) as i64,
        );
        let exec_result = connector
            .execute_query(&sql, DEFAULT_SAMPLE_LIMIT)
            .instrument(tool_span.clone())
            .await;
        match exec_result {
            Ok(exec) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let columns = exec.result.columns.clone();
                let rows: Vec<Vec<serde_json::Value>> = exec
                    .result
                    .rows
                    .iter()
                    .map(|row| {
                        row.0
                            .iter()
                            .map(|cell| match cell {
                                CellValue::Text(s) => serde_json::Value::String(s.clone()),
                                CellValue::Number(n) => serde_json::json!(n),
                                CellValue::Null => serde_json::Value::Null,
                            })
                            .collect()
                    })
                    .collect();
                // Keep the metric recording and the tool_call.output event
                // inside the same `in_scope` so the sink's `trace_id` lookup
                // (if it walks `Span::current()`) sees `tool_span`. In the
                // fan-out path the thread-local span stack doesn't always
                // carry the outer `.instrument(sub_span)` back to us
                // synchronously, so `Span::current()` can otherwise be
                // `Span::none()`.
                tool_span.in_scope(|| {
                    tracing::info!(
                        name: "tool_call.output",
                        status = "success",
                        row_count = exec.result.rows.len(),
                        duration_ms = duration_ms,
                    );
                    if let (Some(sink), Some(q)) =
                        (self.metric_sink.as_ref(), &solution.semantic_query)
                    {
                        sink.record_analytics_query(
                            &self.agent_id,
                            &self.question,
                            &q.measures,
                            &q.dimensions,
                            &sql,
                        );
                    }
                });
                super::emit_domain(
                    &self.event_tx,
                    AnalyticsEvent::QueryExecuted {
                        query: sql.clone(),
                        row_count: exec.result.rows.len(),
                        duration_ms,
                        success: true,
                        error: None,
                        columns,
                        rows,
                        source: query_source,
                        sub_spec_index,
                        semantic_query: solution.semantic_query.clone(),
                    },
                )
                .await;
                Ok(AnalyticsResult::single(exec.result, Some(exec.summary)))
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                tool_span.in_scope(|| {
                    tracing::info!(
                        name: "tool_call.output",
                        status = "error",
                        "error.message" = %e,
                        duration_ms = duration_ms,
                    );
                });
                super::emit_domain(
                    &self.event_tx,
                    AnalyticsEvent::QueryExecuted {
                        query: sql.clone(),
                        row_count: 0,
                        duration_ms,
                        success: false,
                        error: Some(e.to_string()),
                        columns: vec![],
                        rows: vec![],
                        source: query_source,
                        sub_spec_index,
                        semantic_query: solution.semantic_query.clone(),
                    },
                )
                .await;
                Err((
                    AnalyticsError::SyntaxError {
                        query: sql,
                        message: e.to_string(),
                    },
                    BackTarget::Execute(solution.clone(), Default::default()),
                ))
            }
        }
    }
}
