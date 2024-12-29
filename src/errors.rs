use thiserror::Error;

#[derive(Error, Debug)]
pub enum OnyxError {
    #[error("Invalid configuration:\n{0}")]
    ConfigurationError(String),
    #[error("Invalid argument:\n{0}")]
    ArgumentError(String),
    #[error("Runtime error:\n{0}")]
    RuntimeError(String),
    #[error("LLM error:\n{0}")]
    LLMError(String),
    #[error("Agent error:\n{0}")]
    AgentError(String),
    #[error("Anonymizer error:\n{0}")]
    AnonymizerError(String),
}

impl From<Box<dyn std::error::Error>> for OnyxError {
    fn from(error: Box<dyn std::error::Error>) -> Self {
        OnyxError::RuntimeError(error.to_string())
    }
}

impl From<anyhow::Error> for OnyxError {
    fn from(error: anyhow::Error) -> Self {
        OnyxError::RuntimeError(error.to_string())
    }
}

impl From<String> for OnyxError {
    fn from(error: String) -> Self {
        OnyxError::RuntimeError(error)
    }
}
