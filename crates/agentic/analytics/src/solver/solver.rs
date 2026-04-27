use std::collections::HashMap;
use std::sync::Arc;

use agentic_connector::DatabaseConnector;
use agentic_core::{
    events::EventStream,
    human_input::{DeferredInputProvider, HumanInputHandle, ResumeInput, SuspendedRunData},
};

use crate::config::StateConfig;
use crate::engine::SemanticEngine;
use crate::events::AnalyticsEvent;
use crate::llm::{LlmClient, ThinkingConfig};
use crate::metric_sink::SharedMetricSink;
use crate::procedure::ProcedureRunner;
use crate::semantic::SemanticCatalog;
use crate::tools::{SchemaCache, new_schema_cache};
use crate::validation::Validator;

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
    /// Per-state LLM clients built from `states.<name>.model` overrides.
    ///
    /// Only populated for states that specify an explicit `model:` in their
    /// `StateConfig`.  Falls back to `self.client` for all other states.
    pub(crate) state_clients: HashMap<String, LlmClient>,
    /// Global thinking config that overrides the per-state default when set.
    pub(crate) global_thinking: Option<ThinkingConfig>,
    /// When true, `global_thinking` takes absolute precedence over per-state
    /// thinking configs.  Set by extended thinking mode to ensure the runtime override
    /// wins everywhere.
    pub(crate) extended_thinking_active: bool,
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
    /// Identifier of the agentic config driving this run. Used as
    /// `source_ref` when writing metric-usage records to the Metrics tab.
    pub(crate) agent_id: String,
    /// Original user question. Captured on the metric-usage context JSON
    /// so the Metrics detail view can show it.
    pub(crate) question: String,
    /// Optional port for recording Tier 1 metric usage (measures +
    /// dimensions) to whatever backend the host has wired up. `None`
    /// disables metric recording — the pipeline still runs normally.
    pub(crate) metric_sink: Option<SharedMetricSink>,
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
            state_clients: HashMap::new(),
            global_thinking: None,
            extended_thinking_active: false,
            human_input: Arc::new(DeferredInputProvider),
            suspension_data: None,
            resume_data: None,
            procedure_runner: None,
            validator: Validator::default_validator(),
            max_tokens: None,
            engine: None,
            schema_cache: new_schema_cache(),
            agent_id: String::new(),
            question: String::new(),
            metric_sink: None,
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
            state_clients: HashMap::new(),
            global_thinking: None,
            extended_thinking_active: false,
            human_input: Arc::new(DeferredInputProvider),
            suspension_data: None,
            resume_data: None,
            procedure_runner: None,
            validator: Validator::default_validator(),
            max_tokens: None,
            engine: None,
            schema_cache: new_schema_cache(),
            agent_id: String::new(),
            question: String::new(),
            metric_sink: None,
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

    /// Record the agentic config id and user question driving this run.
    /// Both feed the metric-usage records emitted during the Executing stage
    /// so the Metrics observability tab can attribute analytics activity.
    pub fn with_source_attribution(
        mut self,
        agent_id: impl Into<String>,
        question: impl Into<String>,
    ) -> Self {
        self.agent_id = agent_id.into();
        self.question = question.into();
        self
    }

    /// Attach a [`SharedMetricSink`] so the Executing stage can record
    /// the measures and dimensions touched by each query. When
    /// unset, metric recording is a silent no-op.
    pub fn with_metric_sink(mut self, sink: Option<SharedMetricSink>) -> Self {
        self.metric_sink = sink;
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

    /// Set per-state LLM client overrides built from `states.<name>.model`.
    pub fn with_state_clients(mut self, clients: HashMap<String, LlmClient>) -> Self {
        self.state_clients = clients;
        self
    }

    /// Return the LLM client for `state`, falling back to the global client.
    pub(crate) fn client_for_state(&self, state: &str) -> &LlmClient {
        self.state_clients.get(state).unwrap_or(&self.client)
    }

    /// Set a global thinking config applied to every pipeline state.
    pub fn with_global_thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.global_thinking = Some(thinking);
        self
    }

    /// Override the global LLM client and clear per-state model overrides.
    ///
    /// Used for runtime preset overrides (e.g. "extended thinking" mode from the UI).
    /// Per-state **model** clients are cleared so all states use the
    /// overridden client.  Sets `extended_thinking_active` so that the extended thinking
    /// config also takes precedence over per-state overrides.
    pub fn with_client_override(mut self, client: LlmClient) -> Self {
        self.client = client;
        self.state_clients.clear();
        self.extended_thinking_active = true;
        self
    }

    /// Attach an external procedure runner for `SolutionSource::Procedure` solutions.
    pub fn with_procedure_runner(mut self, runner: Arc<dyn ProcedureRunner>) -> Self {
        self.procedure_runner = Some(runner);
        self
    }

    /// Replace the semantic catalog at runtime.
    ///
    /// Used by the coordinator-worker resume path to reload the catalog
    /// after a builder delegation has modified `.view.yml` files on disk.
    pub fn replace_catalog(&mut self, catalog: Arc<SemanticCatalog>) {
        self.catalog = catalog;
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

    /// Day-only date hint that is appended to the system prompt as a
    /// separate, uncached content block (Anthropic) or concatenated
    /// (other providers).  Kept out of the cached prefix so cross-thread
    /// runs can share the system+tools cache.
    pub(crate) fn current_date_hint() -> String {
        chrono::Utc::now().format("Today is %Y-%m-%d.").to_string()
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
    /// When extended thinking mode is active the global thinking override wins
    /// unconditionally.  Otherwise the priority is:
    /// per-state config > global_thinking override > `default`.
    pub(crate) fn thinking_for_state(
        &self,
        state: &str,
        default: ThinkingConfig,
    ) -> ThinkingConfig {
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
    pub(crate) fn max_tool_rounds_for_state(&self, state: &str, default: u32) -> u32 {
        self.state_configs
            .get(state)
            .and_then(|c| c.max_retries)
            .unwrap_or(default)
    }
}
