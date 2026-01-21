use crate::adapters::project::manager::ProjectManager;
// Note: Agent types are not imported here to avoid circular dependencies
// Functions use generic serialization via serde::Serialize trait
use opentelemetry::trace::TraceContextExt as _;
use tracing::{Level, event, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

/// Service-level events (no access to ExecutionContext - just tracing)
pub mod run_agent {
    use crate::execute::types::OutputContainer;

    use super::*;

    pub static NAME: &str = "agent.run_agent";
    pub static TYPE: &str = "agent";
    pub static INPUT: &str = "run_agent.input";
    pub static OUTPUT: &str = "run_agent.output";

    /// Record agent input event (service layer - no metrics, just tracing and intent classification)
    pub fn input(
        project: &ProjectManager,
        agent_ref_str: &str,
        project_path: &str,
        prompt: &str,
        memory: &impl serde::Serialize,
        variables: &Option<std::collections::HashMap<String, serde_json::Value>>,
        source: &impl serde::Serialize,
    ) {
        // Get trace_id from current OpenTelemetry span context
        let trace_id = tracing::Span::current()
            .context()
            .span()
            .span_context()
            .trace_id()
            .to_string();

        // Trigger intent classification if classifier is available
        if let Some(classifier) = &project.intent_classifier {
            println!("Triggering intent classification...");
            let classifier = classifier.clone();
            let trace_id = trace_id.clone();
            let prompt = prompt.to_string();
            let agent_ref = agent_ref_str.to_string();
            tokio::spawn(async move {
                if let Err(e) = classifier
                    .classify_with_learning(&trace_id, &prompt, "agent", &agent_ref)
                    .await
                {
                    warn!("Intent classification failed: {}", e);
                }
            });
        }

        event!(Level::INFO, is_visible = true, name = INPUT, agent_ref = %agent_ref_str, prompt = %prompt, memory = %serde_json::to_string(&memory).unwrap_or_default(), variables = %serde_json::to_string(&variables).unwrap_or_default(), project_path = %project_path, source = %serde_json::to_string(&source).unwrap_or_default());
    }

    /// Record agent output event (service layer - no metrics, just tracing)
    pub fn output(output: &OutputContainer) {
        let output_json = serde_json::to_string(output).unwrap_or_default();

        event!(
            Level::INFO,
            name = OUTPUT,
            is_visible = true,
            status = "success",
            output = %output_json
        );
    }
}

/// Launcher-level events (has access to ExecutionContext for metrics)
pub mod launcher {
    use crate::execute::{ExecutionContext, types::OutputContainer};

    use super::*;

    pub static NAME: &str = "agent.launcher";
    pub static TYPE: &str = "agent";
    pub static INPUT: &str = "agent.launcher.input";
    pub static OUTPUT: &str = "agent.launcher.output";

    /// Record agent input event and track question in metrics
    pub fn input(execution_context: &ExecutionContext, prompt: &str) {
        // Record question in metric context
        execution_context.record_question(prompt);

        event!(
            Level::INFO,
            name = INPUT,
            is_visible = true,
            prompt = %prompt
        );
    }

    /// Record agent output event, track response in metrics, and finalize
    pub fn output(execution_context: &ExecutionContext, output: &OutputContainer) {
        // Record response in metric context
        execution_context.record_response(&output.to_string());

        // Finalize metrics (triggers async storage)
        execution_context.finalize_metrics();

        let output_json = serde_json::to_string(output).unwrap_or_default();

        event!(
            Level::INFO,
            name = OUTPUT,
            is_visible = true,
            status = "success",
            output = %output_json
        );
    }
}

pub mod agent {
    use std::collections::HashMap;

    use crate::{
        config::model::{AgentType, DefaultAgent},
        execute::{ExecutionContext, types::OutputContainer},
    };

    use super::*;

    pub static NAME: &str = "agent.execute";
    pub static TYPE: &str = "agent";
    pub static INPUT: &str = "agent.input";
    pub static OUTPUT: &str = "agent.output";
    pub static VARIABLES: &str = "agent.variables";
    pub static METADATA: &str = "agent.metadata";
    pub static ROUTING_CONTEXT: &str = "agent.routing_context";
    pub static AGENT_TYPE: &str = "agent.agent_type";
    pub static DEFAULT_AGENT: &str = "agent.default_agent";

    pub fn input(input: impl serde::Serialize) {
        event!(Level::INFO, name = INPUT, is_visible = true, input = %serde_json::to_string(&input).unwrap_or_default());
    }

    pub fn output(output: OutputContainer) {
        event!(
            Level::INFO,
            name = OUTPUT,
            is_visible = true,
            status = "success",
            output = %serde_json::to_string(&output).unwrap_or_default()
        );
    }

    pub fn variables(variables: HashMap<String, serde_json::Value>) {
        event!(
            Level::INFO,
            name = VARIABLES,
            is_visible = true,
            variables = %serde_json::to_string(&variables).unwrap_or_default()
        );
    }

    pub fn metadata(metadata: HashMap<String, String>) {
        event!(
            Level::INFO,
            name = METADATA,
            is_visible = true,
            metadata = %serde_json::to_string(&metadata).unwrap_or_default()
        );
    }

    pub fn routing_context(routing_context: ExecutionContext) {
        event!(Level::INFO, name = ROUTING_CONTEXT, is_visible = true, routing_context = ?routing_context);
    }

    pub fn agent_type(agent_type: AgentType) {
        event!(Level::INFO, name = AGENT_TYPE, is_visible = true, agent_type = %agent_type);
    }

    pub fn default_agent(default_agent: DefaultAgent) {
        event!(
            Level::INFO,
            name = DEFAULT_AGENT,
            is_visible = true,
            default_agent = %serde_json::to_string(&default_agent).unwrap_or_default()
        );
    }
}

pub mod default_agent {
    use async_openai::types::chat::ChatCompletionRequestMessage;

    use crate::{
        config::model::{Model, ToolType},
        execute::types::OutputContainer,
    };

    use super::*;

    pub static NAME: &str = "agent.default_agent.execute";
    pub static TYPE: &str = "agent";
    pub static INPUT: &str = "agent.default_agent.input";
    pub static OUTPUT: &str = "agent.default_agent.output";
    pub static MODEL_CONFIG: &str = "agent.default_agent.model_config";
    pub static SYSTEM_INSTRUCTIONS: &str = "agent.default_agent.system_instructions";
    pub static TOOLS: &str = "agent.default_agent.tools";
    pub static MESSAGES: &str = "agent.default_agent.messages";

    pub fn input(input: impl serde::Serialize) {
        event!(
            Level::INFO,
            name = INPUT,
            is_visible = true,
            input = %serde_json::to_string(&input).unwrap_or_default()
        );
    }

    pub fn model_config(model_config: Model) {
        event!(
            Level::INFO,
            name = MODEL_CONFIG,
            is_visible = true,
            model_config = %serde_json::to_string(&model_config).unwrap_or_default()
        );
    }

    pub fn system_instructions(system_instructions: String) {
        event!(
            Level::INFO,
            name = SYSTEM_INSTRUCTIONS,
            is_visible = true,
            system_instructions = %system_instructions
        );
    }

    pub fn tools(tools: Vec<ToolType>) {
        event!(
            Level::INFO,
            name = TOOLS,
            is_visible = true,
            tools = %serde_json::to_string(&tools).unwrap_or_default()
        );
    }

    pub fn messages(messages: Vec<ChatCompletionRequestMessage>) {
        event!(
            Level::INFO,
            name = MESSAGES,
            is_visible = true,
            messages = %serde_json::to_string(&messages).unwrap_or_default()
        );
    }

    pub fn output(output: &OutputContainer) {
        event!(
            Level::INFO,
            name = OUTPUT,
            is_visible = true,
            status = "success",
            output = %serde_json::to_string(&output).unwrap_or_default()
        );
    }
}

pub mod load_agent_config {
    use crate::config::model::AgentConfig;

    use super::*;

    pub static NAME: &str = "agent.load_config";
    pub static TYPE: &str = "agent";
    pub static INPUT: &str = "load_agent_config.input";
    pub static OUTPUT: &str = "load_agent_config.output";

    pub fn input(agent_name: &str) {
        event!(Level::INFO, name = INPUT, is_visible = true, agent_name = %agent_name);
    }

    pub fn output(config: &AgentConfig) {
        event!(Level::INFO, name = OUTPUT, is_visible = true, config = %serde_json::to_string(config).unwrap_or_default(), status = "success");
    }
}

pub mod get_global_context {
    use crate::config::model::Config;

    use super::*;

    pub static NAME: &str = "agent.get_global_context";
    pub static TYPE: &str = "agent";
    pub static INPUT: &str = "get_global_context.input";
    pub static OUTPUT: &str = "get_global_context.output";

    pub fn input(config: &Config) {
        event!(Level::INFO, name = INPUT, is_visible = true, config = %serde_json::to_string(config).unwrap_or_default());
    }

    pub fn output(context: &minijinja::Value) {
        event!(
            Level::INFO,
            name = OUTPUT,
            is_visible = true,
            status = "success",
            context = %format!("{:?}", context)
        );
    }
}

pub mod routing_agent {
    use crate::{
        config::model::{Model, ToolType},
        execute::types::OutputContainer,
    };

    use super::*;

    pub static NAME: &str = "agent.routing_agent.execute";
    pub static TYPE: &str = "agent";
    pub static INPUT: &str = "routing_agent.input";
    pub static OUTPUT: &str = "routing_agent.output";
    pub static MODEL_CONFIG: &str = "routing_agent.model_config";
    pub static SYSTEM_INSTRUCTIONS: &str = "routing_agent.system_instructions";
    pub static TOOLS: &str = "routing_agent.tools";
    pub static RESOLVED_ROUTES: &str = "routing_agent.resolved_routes";
    pub static FALLBACK_CONFIGURED: &str = "routing_agent.fallback_configured";
    pub static FALLBACK_TRIGGERED: &str = "routing_agent.fallback_triggered";

    pub fn input(input: &impl serde::Serialize) {
        event!(
            Level::INFO,
            name = INPUT,
            is_visible = true,
            input = %serde_json::to_string(&input).unwrap_or_default()
        );
    }

    pub fn model_config(model_config: &Model) {
        event!(
            Level::INFO,
            name = MODEL_CONFIG,
            is_visible = true,
            model_config = %serde_json::to_string(&model_config).unwrap_or_default()
        );
    }

    pub fn system_instructions(system_instructions: &str) {
        event!(
            Level::INFO,
            name = SYSTEM_INSTRUCTIONS,
            is_visible = true,
            system_instructions = %system_instructions
        );
    }

    pub fn tools(tools: &[ToolType]) {
        event!(
            Level::INFO,
            name = TOOLS,
            is_visible = true,
            tools = %serde_json::to_string(&tools).unwrap_or_default()
        );
    }

    pub fn resolved_routes(count: usize, routes: &[ToolType]) {
        event!(
            Level::INFO,
            name = RESOLVED_ROUTES,
            is_visible = true,
            count = %count,
            routes = %serde_json::to_string(&routes).unwrap_or_default(),
            message = "Routes resolved from vector search"
        );
    }

    pub fn fallback_configured(fallback_route: &str) {
        event!(
            Level::INFO,
            name = FALLBACK_CONFIGURED,
            is_visible = true,
            fallback_route = %fallback_route,
            message = "Routing agent fallback route configured"
        );
    }

    pub fn fallback_triggered() {
        event!(
            Level::INFO,
            name = FALLBACK_TRIGGERED,
            is_visible = true,
            message = "Routing agent fallback triggered - no tool calls in response"
        );
    }

    pub fn output(output: &OutputContainer) {
        event!(
            Level::INFO,
            name = OUTPUT,
            is_visible = true,
            status = "success",
            output = %serde_json::to_string(&output).unwrap_or_default()
        );
    }
}

pub mod fallback_agent {
    use async_openai::types::chat::ChatCompletionRequestMessage;

    // Note: Agent types removed to avoid circular dependencies
    // Functions use generic serde::Serialize instead

    use super::*;

    pub static FALLBACK_NAME: &str = "agent.fallback_agent.execute";
    pub static FALLBACK_TYPE: &str = "agent";
    pub static INPUT: &str = "fallback_agent.input";
    pub static OUTPUT: &str = "fallback_agent.output";
    pub static AGENT: &str = "fallback_agent.agent";

    pub fn agent(agent: &impl serde::Serialize) {
        event!(
            Level::INFO,
            name = AGENT,
            is_visible = true,
            agent = %serde_json::to_string(&agent).unwrap_or_default()
        );
    }

    pub fn input(input: &[ChatCompletionRequestMessage]) {
        event!(
            Level::INFO,
            name = INPUT,
            is_visible = true,
            messages = %serde_json::to_string(&input).unwrap_or_default()
        );
    }

    pub fn output(output: &impl serde::Serialize) {
        event!(
            Level::INFO,
            name = OUTPUT,
            is_visible = true,
            status = "success",
            output = %serde_json::to_string(&output).unwrap_or_default()
        );
    }
}
