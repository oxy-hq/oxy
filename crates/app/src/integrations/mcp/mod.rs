// Module declarations
mod connections;
mod context;
mod executor;
mod filters;
mod server;
mod tools;
mod types;
mod utils;
mod variables;

// Re-export the main service API
pub use connections::extract_connection_overrides;
pub use context::ToolExecutionContext;
pub use filters::extract_session_filters;
pub use types::{OxyMcpServer, OxyTool, ToolType};
pub use variables::{extract_meta_variables, merge_variables, validate_variables};
