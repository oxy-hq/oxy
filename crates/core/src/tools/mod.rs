pub mod create_data_app;
pub mod edit_data_app;
pub mod launcher;
pub mod omni;
pub mod read_data_app;
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
    /// The file path of the data app being edited in the current thread (if any).
    /// Available in templates as `{{ tools.data_app_file_path }}`.
    pub data_app_file_path: Option<String>,
}

impl ToolsContext {
    pub fn from_execution_context(
        execution_context: &crate::execute::ExecutionContext,
        agent_name: String,
        tools: Vec<crate::config::model::ToolType>,
        prompt: String,
    ) -> Self {
        Self {
            agent_name,
            tools,
            prompt,
            data_app_file_path: execution_context.data_app_file_path.clone(),
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
            "data_app_file_path" => self
                .data_app_file_path
                .as_ref()
                .map(|p| minijinja::value::Value::from(p.clone())),
            _ => None,
        }
    }
}
