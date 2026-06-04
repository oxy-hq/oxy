use thiserror::Error;

pub type UnifiResult<T> = Result<T, UnifiError>;

#[derive(Debug, Error)]
pub enum UnifiError {
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(String),

    #[error("transport error: {0}")]
    Transport(String),

    /// 401 / 403 — caller's API key is invalid or lacks permission. The
    /// connector proxy returns 403 with "user is not the owner of this
    /// host" for admin-level keys; we surface that verbatim.
    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("rate limited (retry after {retry_after_secs:?}s)")]
    RateLimited { retry_after_secs: Option<u64> },

    #[error("unexpected status {status}: {body}")]
    Unexpected { status: u16, body: String },

    #[error("malformed response: {0}")]
    Decode(String),
}
