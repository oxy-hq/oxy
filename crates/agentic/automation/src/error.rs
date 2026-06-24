//! Automation error types.

/// Errors produced by the automation execution engine.
#[derive(Debug, thiserror::Error)]
pub enum AutomationError {
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("step execution error: {0}")]
    StepExecution(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<serde_json::Error> for AutomationError {
    fn from(e: serde_json::Error) -> Self {
        AutomationError::Serialization(e.to_string())
    }
}

impl From<serde_yaml::Error> for AutomationError {
    fn from(e: serde_yaml::Error) -> Self {
        AutomationError::Serialization(e.to_string())
    }
}
