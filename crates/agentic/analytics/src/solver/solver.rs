use std::collections::HashMap;
use std::sync::Arc;

use agentic_connector::DatabaseConnector;
use agentic_core::{
    events::{CoreEvent, DomainEvents, EventStream, Outcome},
    human_input::{DeferredInputProvider, HumanInputHandle, ResumeInput, SuspendedRunData},
    orchestrator::{RunContext, SessionMemory},
    result::CellValue,
    solver::FanoutWorker,
    BackTarget,
};
use async_trait::async_trait;

use crate::config::StateConfig;
use crate::engine::SemanticEngine;
use crate::events::AnalyticsEvent;
use crate::llm::{InitialMessages, LlmClient, ThinkingConfig, ToolLoopConfig};
use crate::procedure::ProcedureRunner;
use crate::schemas::solve_response_schema;
use crate::semantic::SemanticCatalog;
use crate::tools::{execute_solving_tool, new_schema_cache, SchemaCache};
use crate::types::{SolutionPayload, SolutionSource};
use crate::validation::Validator;
use crate::{AnalyticsDomain, AnalyticsError, AnalyticsResult, AnalyticsSolution, QuerySpec};

use super::prompts::QUESTION_TYPE_DEFS;

// ---------------------------------------------------------------------------
// AnalyticsSolver
// ---------------------------------------------------------------------------

/// Solver for the analytics domain.
///
/// Pipeline stages are split across per-state submodules; this struct holds
/// all shared state and wires together the [`DomainSolver`] trait impl.
pub struct AnalyticsSolver {
    pub(crate) client: LlmClient,
    pub(crate) catalog: Arc<SemanticCatalog>,
    /// All configured connectors, keyed by logical name.
    pub(crate) connectors: HashMap<String, Arc<dyn DatabaseConnector>>,
    /// Logical name of the default connector.
    pub(crate) default_connector: String,
    /// Optional event stream for emitting LLM and domain events.
    pub(crate) event_tx: Option<EventStream<AnalyticsEvent>>,
    /// Global instructions injected into every LLM call.
    pub(crate) global_instructions: Option<String>,
    /// SQL example files appended to the Solving prompt.
    pub(crate) sql_examples: Vec<String>,
    /// Markdown domain docs injected into Clarifying and Interpreting prompts.
    pub(crate) domain_docs: Vec<String>,
    /// Per-state config overrides (thinking config, max_retries, instructions).
    pub(crate) state_configs: HashMap<String, StateConfig>,
    /// Global thinking config that overrides the per-state default when set.
    pub(crate) global_thinking: Option<ThinkingConfig>,
    /// Human input provider for `ask_user` tool calls.
    ///
    /// Defaults to [`DeferredInputProvider`] (always suspends).
    pub(crate) human_input: HumanInputHandle,
    /// Suspension data stored by the `ask_user` tool handler.
    pub(crate) suspension_data: Option<SuspendedRunData>,
    /// Resume input injected before re-entering the pipeline.
    pub(crate) resume_data: Option<ResumeInput>,
    /// Optional external procedure runner.
    ///
    /// When set, the executing stage delegates `SolutionSource::Procedure`
    /// solutions to this runner instead of the SQL connector.
    pub(crate) procedure_runner: Option<Arc<dyn ProcedureRunner>>,
    /// Configured validator for all three pipeline stages.
    pub(crate) validator: Validator,
    /// Initial max output tokens per LLM call, sourced from `llm.max_tokens` in config.
    ///
    /// When `Some`, used as `max_tokens_override` in every [`ToolLoopConfig`].
    /// Resume continuations take precedence and may double this value.
    pub(crate) max_tokens: Option<u32>,
    /// Optional vendor semantic engine for the VendorEngine execution path.
    ///
    /// When `Some`, the Specifying handler attempts to translate the intent via
    /// the engine before falling back to `try_compile` / LLM.  The Executing
    /// handler dispatches `SolutionPayload::Vendor` queries here.
    pub(crate) engine: Option<Arc<dyn SemanticEngine>>,
    /// Lazy schema cache for database lookup tools (`list_tables`, `describe_table`).
    ///
    /// Populated on first tool call per connector, then reused for the session.
    pub(crate) schema_cache: SchemaCache,
}

impl AnalyticsSolver {
    /// Create a solver with a single connector registered as `"default"`.
    pub fn new(
        client: LlmClient,
        catalog: SemanticCatalog,
        connector: Box<dyn DatabaseConnector>,
    ) -> Self {
        let mut connectors: HashMap<String, Arc<dyn DatabaseConnector>> = HashMap::new();
        connectors.insert("default".to_string(), Arc::from(connector));
        Self {
            client,
            catalog: Arc::new(catalog),
            connectors,
            default_connector: "default".to_string(),
            event_tx: None,
            global_instructions: None,
            sql_examples: vec![],
            domain_docs: vec![],
            state_configs: HashMap::new(),
            global_thinking: None,
            human_input: Arc::new(DeferredInputProvider),
            suspension_data: None,
            resume_data: None,
            procedure_runner: None,
            validator: Validator::default_validator(),
            max_tokens: None,
            engine: None,
            schema_cache: new_schema_cache(),
        }
    }

    /// Create a solver with multiple named database connectors.
    pub fn new_multi(
        client: LlmClient,
        catalog: SemanticCatalog,
        connectors: HashMap<String, Arc<dyn DatabaseConnector>>,
        default_connector: String,
    ) -> Self {
        Self {
            client,
            catalog: Arc::new(catalog),
            connectors,
            default_connector,
            event_tx: None,
            global_instructions: None,
            sql_examples: vec![],
            domain_docs: vec![],
            state_configs: HashMap::new(),
            global_thinking: None,
            human_input: Arc::new(DeferredInputProvider),
            suspension_data: None,
            resume_data: None,
            procedure_runner: None,
            validator: Validator::default_validator(),
            max_tokens: None,
            engine: None,
            schema_cache: new_schema_cache(),
        }
    }

    /// Set the human input provider for `ask_user` tool calls.
    pub fn with_human_input(mut self, provider: HumanInputHandle) -> Self {
        self.human_input = provider;
        self
    }

    /// Attach an event stream.
    pub fn with_events(mut self, tx: EventStream<AnalyticsEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Set global instructions injected into every LLM call.
    pub fn with_global_instructions(mut self, instructions: Option<String>) -> Self {
        self.global_instructions = instructions;
        self
    }

    /// Set SQL example snippets appended to the Solving system prompt.
    pub fn with_sql_examples(mut self, examples: Vec<String>) -> Self {
        self.sql_examples = examples;
        self
    }

    /// Set markdown domain docs injected into Clarifying and Interpreting prompts.
    pub fn with_domain_docs(mut self, docs: Vec<String>) -> Self {
        self.domain_docs = docs;
        self
    }

    /// Set per-state config overrides.
    pub fn with_state_configs(mut self, configs: HashMap<String, StateConfig>) -> Self {
        self.state_configs = configs;
        self
    }

    /// Set a global thinking config applied to every pipeline state.
    pub fn with_global_thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.global_thinking = Some(thinking);
        self
    }

    /// Attach an external procedure runner for `SolutionSource::Procedure` solutions.
    pub fn with_procedure_runner(mut self, runner: Arc<dyn ProcedureRunner>) -> Self {
        self.procedure_runner = Some(runner);
        self
    }

    /// Override the default validator with a custom-configured one.
    pub fn with_validator(mut self, validator: Validator) -> Self {
        self.validator = validator;
        self
    }

    /// Set the initial max output tokens applied to every LLM call.
    pub fn with_max_tokens(mut self, max_tokens: Option<u32>) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Attach a vendor semantic engine for the VendorEngine execution path.
    pub fn with_engine(mut self, engine: Arc<dyn SemanticEngine>) -> Self {
        self.engine = Some(engine);
        self
    }

    /// Build a composite system prompt for a given state.
    ///
    /// Composition order:
    /// 1. Hardcoded `base` system prompt.
    /// 2. `QUESTION_TYPE_DEFS` for clarifying/specifying states.
    /// 3. `global_instructions` from config.
    /// 4. Per-state `instructions` from `state_configs`.
    /// 5. `domain_docs` (clarifying + interpreting).
    /// 6. SQL dialect + `sql_examples` (specifying + solving).
    pub(crate) fn build_system_prompt(
        &self,
        state: &str,
        base: &str,
        dialect: Option<&str>,
    ) -> String {
        let mut parts = vec![base.to_string()];

        match state {
            "clarifying" | "specifying" => {
                parts.push(QUESTION_TYPE_DEFS.to_string());
            }
            _ => {}
        }

        if let Some(global) = &self.global_instructions {
            if !global.trim().is_empty() {
                parts.push(format!(
                    "<global_instructions>\n{}\n</global_instructions>",
                    global.trim()
                ));
            }
        }

        if let Some(state_cfg) = self.state_configs.get(state) {
            if let Some(state_instr) = &state_cfg.instructions {
                if !state_instr.trim().is_empty() {
                    parts.push(format!(
                        "<state_instructions>\n{}\n</state_instructions>",
                        state_instr.trim()
                    ));
                }
            }
        }

        match state {
            "clarifying" | "interpreting" => {
                for doc in &self.domain_docs {
                    if !doc.trim().is_empty() {
                        parts.push(format!(
                            "<domain_context>\n{}\n</domain_context>",
                            doc.trim()
                        ));
                    }
                }
            }
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
    ///
    /// Priority: per-state config > global_thinking override > `default`.
    pub(crate) fn thinking_for_state(
        &self,
        state: &str,
        default: ThinkingConfig,
    ) -> ThinkingConfig {
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
    pub(crate) fn max_tool_rounds_for_state(&self, state: &str, default: u32) -> u32 {
        self.state_configs
            .get(state)
            .and_then(|c| c.max_retries)
            .unwrap_or(default)
    }
}

// ---------------------------------------------------------------------------
// AnalyticsFanoutWorker
// ---------------------------------------------------------------------------

/// A `Send + Sync` worker for concurrent fan-out: solves and executes a single
/// [`QuerySpec`] independently of other workers.
///
/// Holds only shared (`Arc`-wrapped) or `Clone` state cloned from the parent
/// [`AnalyticsSolver`].
pub struct AnalyticsFanoutWorker {
    client: LlmClient,
    catalog: Arc<SemanticCatalog>,
    connectors: HashMap<String, Arc<dyn DatabaseConnector>>,
    default_connector: String,
    event_tx: Option<EventStream<AnalyticsEvent>>,
    global_instructions: Option<String>,
    sql_examples: Vec<String>,
    state_configs: HashMap<String, StateConfig>,
    global_thinking: Option<ThinkingConfig>,
    max_tokens: Option<u32>,
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
            global_thinking: solver.global_thinking.clone(),
            max_tokens: solver.max_tokens,
        }
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

        if let Some(global) = &self.global_instructions {
            if !global.trim().is_empty() {
                parts.push(format!(
                    "<global_instructions>\n{}\n</global_instructions>",
                    global.trim()
                ));
            }
        }

        if let Some(state_cfg) = self.state_configs.get(state) {
            if let Some(state_instr) = &state_cfg.instructions {
                if !state_instr.trim().is_empty() {
                    parts.push(format!(
                        "<state_instructions>\n{}\n</state_instructions>",
                        state_instr.trim()
                    ));
                }
            }
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

        // ── Check for pre-computed skip (SemanticLayer / Procedure / VendorEngine) ──
        let solution = match &spec.solution_source {
            SolutionSource::SemanticLayer => {
                if let Some(payload) = spec.precomputed.clone() {
                    Some(AnalyticsSolution {
                        payload,
                        solution_source: SolutionSource::SemanticLayer,
                        connector_name: spec.connector_name.clone(),
                    })
                } else {
                    None
                }
            }
            SolutionSource::Procedure { file_path } => Some(AnalyticsSolution {
                payload: SolutionPayload::Sql(String::new()),
                solution_source: SolutionSource::Procedure {
                    file_path: file_path.clone(),
                },
                connector_name: spec.connector_name.clone(),
            }),
            SolutionSource::VendorEngine(_) => {
                if let Some(payload) = spec.precomputed.clone() {
                    Some(AnalyticsSolution {
                        payload,
                        solution_source: spec.solution_source.clone(),
                        connector_name: spec.connector_name.clone(),
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

        let result = self.execute_solution(&solution).await;

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
        use super::prompts::{solve_type_addendum, SOLVE_BASE_PROMPT};
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
            .client
            .run_with_tools(
                &system_prompt,
                initial,
                &tools,
                |name: String, params| {
                    let connector = Arc::clone(&connector);
                    Box::pin(async move { execute_solving_tool(&name, params, &*connector).await })
                },
                &self.event_tx,
                ToolLoopConfig {
                    max_tool_rounds: max_rounds,
                    state: "solving".into(),
                    thinking,
                    response_schema: Some(solve_response_schema()),
                    max_tokens_override: self.max_tokens,
                    sub_spec_index,
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
            AnalyticsEvent::QueryGenerated { sql: sql.clone() },
        )
        .await;

        let solution_source = spec.solution_source.clone();
        let connector_name = spec.connector_name.clone();
        Ok(AnalyticsSolution {
            payload: SolutionPayload::Sql(sql),
            solution_source,
            connector_name,
        })
    }

    /// Execute a solution against the appropriate connector.
    ///
    /// Shared-ref version of `AnalyticsSolver::execute_solution`, limited to
    /// the `SolutionPayload::Sql` path (fan-out specs are always SQL).
    async fn execute_solution(
        &self,
        solution: &AnalyticsSolution,
    ) -> Result<AnalyticsResult, (AnalyticsError, BackTarget<AnalyticsDomain>)> {
        const DEFAULT_SAMPLE_LIMIT: u64 = 1_000;
        let start = std::time::Instant::now();

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

        match connector.execute_query(&sql, DEFAULT_SAMPLE_LIMIT).await {
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
                    },
                )
                .await;
                Ok(AnalyticsResult::single(exec.result, Some(exec.summary)))
            }
            Err(e) => {
                let duration_ms = start.elapsed().as_millis() as u64;
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
