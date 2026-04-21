//! Workflow error types.

/// Errors produced by the workflow execution engine.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("step execution error: {0}")]
    StepExecution(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for WorkflowError {
    fn from(e: serde_json::Error) -> Self {
        WorkflowError::Serialization(e.to_string())
    }
}

impl From<serde_yaml::Error> for WorkflowError {
    fn from(e: serde_yaml::Error) -> Self {
        WorkflowError::Serialization(e.to_string())
    }
}
