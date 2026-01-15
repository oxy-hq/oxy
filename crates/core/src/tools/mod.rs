pub mod create_data_app;
pub mod launcher;
pub mod omni;
pub mod registry;
pub mod retrieval;
pub mod sql;
pub mod tool;
pub mod types;
pub mod v0;
pub mod visualize;

// Re-export commonly used types and executables
pub use launcher::ToolLauncherExecutable;
pub use registry::{ToolExecutor, ToolRegistry, global_registry, register_tool_executor};
pub use retrieval::RetrievalExecutable;
pub use sql::SQLExecutable;
pub use tool::ToolExecutable;
pub use types::{RetrievalInput, SQLInput, ToolRawInput};
pub use visualize::VisualizeExecutable;

// Tool input type
pub use launcher::ToolInput;

/// Tools context - placeholder for template rendering
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolsContext {
    pub agent_name: String,
    #[serde(skip)]
    pub tools: Vec<crate::config::model::ToolType>,
    pub prompt: String,
}

impl ToolsContext {
    pub fn from_execution_context(
        _execution_context: &crate::execute::ExecutionContext,
        agent_name: String,
        tools: Vec<crate::config::model::ToolType>,
        prompt: String,
    ) -> Self {
        Self {
            agent_name,
            tools,
            prompt,
        }
    }
}

impl minijinja::value::Object for ToolsContext {
    fn get_value(
        self: &std::sync::Arc<Self>,
        key: &minijinja::value::Value,
    ) -> Option<minijinja::value::Value> {
        match key.as_str()? {
            "agent_name" => Some(minijinja::value::Value::from(self.agent_name.clone())),
            "prompt" => Some(minijinja::value::Value::from(self.prompt.clone())),
            _ => None,
        }
    }
}
