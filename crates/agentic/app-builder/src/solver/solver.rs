//! [`AppBuilderSolver`] struct definition.

use std::collections::HashMap;
use std::sync::Arc;

use agentic_analytics::config::StateConfig;
use agentic_analytics::{LlmClient, SemanticCatalog, ThinkingConfig};
use agentic_connector::DatabaseConnector;
use agentic_core::{
    events::EventStream,
    human_input::{ResumeInput, SuspendedRunData},
};

use crate::events::AppBuilderEvent;
use crate::types::AppValidator;

// ---------------------------------------------------------------------------
// AppBuilderSolver
// ---------------------------------------------------------------------------

/// Solver for the app builder domain.
///
/// Pipeline stages are split across per-state submodules.  This struct holds
/// all shared state.
pub struct AppBuilderSolver {
    pub(crate) client: LlmClient,
    pub(crate) catalog: Arc<SemanticCatalog>,
    pub(crate) connectors: HashMap<String, Arc<dyn DatabaseConnector>>,
    pub(crate) default_connector: String,
    pub(crate) instructions: Option<String>,
    pub(crate) state_configs: HashMap<String, StateConfig>,
    /// Optional global thinking config — fallback when no per-state override is set.
    pub(crate) global_thinking: Option<ThinkingConfig>,
    /// Initial max output tokens per LLM call, sourced from `llm.max_tokens` in config.
    pub(crate) max_tokens: Option<u32>,
    pub(crate) suspension_data: Option<SuspendedRunData>,
    pub(crate) resume_data: Option<ResumeInput>,
    pub(crate) event_tx: Option<EventStream<AppBuilderEvent>>,
    /// Pre-computed per-task specs for retry (short-circuits `specify()`).
    pub(crate) pre_computed_specs: Option<Vec<crate::types::AppSpec>>,
    /// Pre-solved SQL for sub-specs that succeeded in a prior run.
    /// Maps sub_spec_index → SQL string.  The fanout worker skips LLM
    /// solving for these indices and goes straight to execution.
    pub(crate) pre_solved_sqls: HashMap<usize, String>,
    /// Optional validator called after YAML assembly in the interpreting phase.
    pub(crate) validator: Option<Arc<dyn AppValidator>>,
}

impl AppBuilderSolver {
    /// Create a solver with a single connector named `"default"`.
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
            instructions: None,
            state_configs: HashMap::new(),
            global_thinking: None,
            max_tokens: None,
            suspension_data: None,
            resume_data: None,
            event_tx: None,
            pre_computed_specs: None,
            pre_solved_sqls: HashMap::new(),
            validator: None,
        }
    }

    /// Create a solver with multiple named connectors.
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
            instructions: None,
            state_configs: HashMap::new(),
            global_thinking: None,
            max_tokens: None,
            suspension_data: None,
            resume_data: None,
            event_tx: None,
            pre_computed_specs: None,
            pre_solved_sqls: HashMap::new(),
            validator: None,
        }
    }

    /// Set the global instructions injected into every LLM call.
    pub fn with_instructions(mut self, instructions: Option<String>) -> Self {
        self.instructions = instructions;
        self
    }

    /// Set per-state config overrides.
    pub fn with_state_configs(mut self, configs: HashMap<String, StateConfig>) -> Self {
        self.state_configs = configs;
        self
    }

    /// Set a global thinking config applied when no per-state override is present.
    pub fn with_global_thinking(mut self, thinking: ThinkingConfig) -> Self {
        self.global_thinking = Some(thinking);
        self
    }

    /// Set the initial max output tokens applied to every LLM call.
    pub fn with_max_tokens(mut self, max_tokens: Option<u32>) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Attach an event stream.
    pub fn with_events(mut self, tx: EventStream<AppBuilderEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Set pre-computed per-task specs for retry (short-circuits `specify()`).
    pub fn with_pre_computed_specs(mut self, specs: Vec<crate::types::AppSpec>) -> Self {
        self.pre_computed_specs = Some(specs);
        self
    }

    /// Set pre-solved SQL for sub-specs that succeeded in a prior run.
    pub fn with_pre_solved_sqls(mut self, sqls: HashMap<usize, String>) -> Self {
        self.pre_solved_sqls = sqls;
        self
    }

    /// Attach a validator called after YAML assembly in the interpreting phase.
    pub fn with_validator(mut self, validator: Arc<dyn AppValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    /// Resolve the thinking config for a state.
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

    /// Resolve the max tool rounds for a state, falling back to `default`.
    pub(crate) fn max_tool_rounds_for_state(&self, state: &str, default: u32) -> u32 {
        self.state_configs
            .get(state)
            .and_then(|c| c.max_retries)
            .unwrap_or(default)
    }

    /// Build a system prompt, appending global and state-level instructions.
    pub(crate) fn build_system_prompt(&self, state: &str, base: &str) -> String {
        let mut parts = vec![base.to_string()];

        if let Some(global) = &self.instructions
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

        parts.join("\n\n")
    }
}

// ---------------------------------------------------------------------------
// AppBuilderFanoutWorker
// ---------------------------------------------------------------------------

/// A lightweight, `Send + Sync` worker that can solve and execute a single
/// per-task spec concurrently with other workers during a fan-out.
///
/// Holds only shared / cloneable state — never `&mut self`.
pub struct AppBuilderFanoutWorker {
    pub(crate) client: LlmClient,
    pub(crate) catalog: Arc<SemanticCatalog>,
    pub(crate) connectors: HashMap<String, Arc<dyn DatabaseConnector>>,
    pub(crate) default_connector: String,
    pub(crate) instructions: Option<String>,
    pub(crate) state_configs: HashMap<String, StateConfig>,
    pub(crate) global_thinking: Option<ThinkingConfig>,
    pub(crate) max_tokens: Option<u32>,
    pub(crate) event_tx: Option<EventStream<AppBuilderEvent>>,
    /// Pre-solved SQL for sub-specs that don't need LLM solving (retry).
    pub(crate) pre_solved_sqls: HashMap<usize, String>,
}

impl AppBuilderFanoutWorker {
    /// Resolve the thinking config for a state.
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

    /// Resolve the max tool rounds for a state, falling back to `default`.
    pub(crate) fn max_tool_rounds_for_state(&self, state: &str, default: u32) -> u32 {
        self.state_configs
            .get(state)
            .and_then(|c| c.max_retries)
            .unwrap_or(default)
    }

    /// Build a system prompt, appending global and state-level instructions.
    pub(crate) fn build_system_prompt(&self, state: &str, base: &str) -> String {
        let mut parts = vec![base.to_string()];

        if let Some(global) = &self.instructions
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

        parts.join("\n\n")
    }
}

impl AppBuilderSolver {
    /// Construct a fanout worker from the solver's shared state.
    pub(crate) fn build_fanout_worker(&self) -> Arc<AppBuilderFanoutWorker> {
        Arc::new(AppBuilderFanoutWorker {
            client: self.client.clone(),
            catalog: Arc::clone(&self.catalog),
            connectors: self.connectors.clone(),
            default_connector: self.default_connector.clone(),
            instructions: self.instructions.clone(),
            state_configs: self.state_configs.clone(),
            global_thinking: self.global_thinking.clone(),
            max_tokens: self.max_tokens,
            event_tx: self.event_tx.clone(),
            pre_solved_sqls: self.pre_solved_sqls.clone(),
        })
    }
}
