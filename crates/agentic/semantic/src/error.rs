use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}
