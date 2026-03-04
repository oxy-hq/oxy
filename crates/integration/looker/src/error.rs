//! Error types for Looker integration

use thiserror::Error;

/// Errors that can occur when interacting with the Looker API.
#[derive(Debug, Error)]
pub enum LookerError {
    /// Authentication with the Looker API failed.
    #[error("Authentication failed: {message}")]
    AuthenticationError { message: String },

    /// An error response was received from the Looker API.
    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    /// The request was rate limited by the Looker API.
    #[error("Rate limited, retry after {retry_after_seconds}s")]
    RateLimitError { retry_after_seconds: u64 },

    /// An error occurred while executing a query.
    #[error("Query error: {message}")]
    QueryError { message: String },

    /// An error occurred during metadata synchronization.
    #[error("Sync error: {message}")]
    SyncError { message: String },

    /// The configuration is invalid or missing required fields.
    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    /// A network or connection error occurred.
    #[error("Connection error: {message}")]
    ConnectionError { message: String },

    /// The requested resource was not found.
    #[error("Not found: {resource}")]
    NotFoundError { resource: String },
}

impl LookerError {
    /// Returns true if this error is temporary and the operation might succeed if retried.
    pub fn is_temporary(&self) -> bool {
        matches!(
            self,
            LookerError::RateLimitError { .. } | LookerError::ConnectionError { .. }
        )
    }

    /// Returns the recommended delay in seconds before retrying, if applicable.
    pub fn retry_delay_seconds(&self) -> Option<u64> {
        match self {
            LookerError::RateLimitError {
                retry_after_seconds,
            } => Some(*retry_after_seconds),
            LookerError::ConnectionError { .. } => Some(5), // Default retry delay for connection errors
            _ => None,
        }
    }

    /// Returns a user-friendly error message suitable for display.
    pub fn user_friendly_message(&self) -> String {
        match self {
            LookerError::AuthenticationError { .. } => {
                "Failed to authenticate with Looker. Please check your credentials.".to_string()
            }
            LookerError::ApiError { status, message } => {
                format!(
                    "Looker API returned an error (HTTP {}): {}",
                    status, message
                )
            }
            LookerError::RateLimitError {
                retry_after_seconds,
            } => {
                format!(
                    "Too many requests to Looker API. Please wait {} seconds.",
                    retry_after_seconds
                )
            }
            LookerError::QueryError { message } => {
                format!("Query execution failed: {}", message)
            }
            LookerError::SyncError { message } => {
                format!("Metadata sync failed: {}", message)
            }
            LookerError::ConfigError { message } => {
                format!("Configuration error: {}", message)
            }
            LookerError::ConnectionError { .. } => {
                "Unable to connect to Looker. Please check your network connection.".to_string()
            }
            LookerError::NotFoundError { resource } => {
                format!("The requested {} was not found in Looker.", resource)
            }
        }
    }
}

impl From<reqwest::Error> for LookerError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_connect() || err.is_timeout() {
            LookerError::ConnectionError {
                message: err.to_string(),
            }
        } else if err.is_status() {
            if let Some(status) = err.status() {
                LookerError::ApiError {
                    status: status.as_u16(),
                    message: err.to_string(),
                }
            } else {
                LookerError::ConnectionError {
                    message: err.to_string(),
                }
            }
        } else {
            LookerError::ConnectionError {
                message: err.to_string(),
            }
        }
    }
}

impl From<serde_json::Error> for LookerError {
    fn from(err: serde_json::Error) -> Self {
        LookerError::ApiError {
            status: 0,
            message: format!("JSON parsing error: {}", err),
        }
    }
}

impl From<std::io::Error> for LookerError {
    fn from(err: std::io::Error) -> Self {
        LookerError::SyncError {
            message: err.to_string(),
        }
    }
}
