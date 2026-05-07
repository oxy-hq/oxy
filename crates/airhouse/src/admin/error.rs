use thiserror::Error;

#[derive(Debug, Error)]
pub enum AirhouseError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("resource already exists: {0}")]
    AlreadyExists(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("provisioning failed: {0}")]
    Provisioning(String),
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
}
