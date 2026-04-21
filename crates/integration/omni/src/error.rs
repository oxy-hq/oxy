use thiserror::Error;

/// Comprehensive error types for Omni integration operations
#[derive(Debug, Error)]
pub enum OmniError {
    #[error("Omni API error: {message} (status: {status_code})")]
    ApiError { message: String, status_code: u16 },

    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    #[error("Metadata sync failed: {0}")]
    SyncError(String),

    #[error("Invalid query structure: {0}")]
    QueryError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON serialization/deserialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimitError(String),

    #[error("Resource not found: {0}")]
    NotFoundError(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Query timeout: {0}")]
    QueryTimeoutError(String),

    #[error("Query polling failed: {0}")]
    QueryPollingError(String),
}

impl Clone for OmniError {
    fn clone(&self) -> Self {
        match self {
            OmniError::ApiError {
                message,
                status_code,
            } => OmniError::ApiError {
                message: message.clone(),
                status_code: *status_code,
            },
            OmniError::AuthenticationError(msg) => OmniError::AuthenticationError(msg.clone()),
            OmniError::SyncError(msg) => OmniError::SyncError(msg.clone()),
            OmniError::QueryError(msg) => OmniError::QueryError(msg.clone()),
            OmniError::ConfigError(msg) => OmniError::ConfigError(msg.clone()),
            // For foreign error types, we create a simple string representation
            OmniError::HttpError(e) => OmniError::ConnectionError(format!("HTTP error: {}", e)),
            OmniError::SerializationError(e) => {
                OmniError::ValidationError(format!("Serialization error: {}", e))
            }
            OmniError::IoError(e) => OmniError::StorageError(format!("IO error: {}", e)),
            OmniError::StorageError(msg) => OmniError::StorageError(msg.clone()),
            OmniError::ValidationError(msg) => OmniError::ValidationError(msg.clone()),
            OmniError::ConnectionError(msg) => OmniError::ConnectionError(msg.clone()),
            OmniError::RateLimitError(msg) => OmniError::RateLimitError(msg.clone()),
            OmniError::NotFoundError(msg) => OmniError::NotFoundError(msg.clone()),
            OmniError::ServerError(msg) => OmniError::ServerError(msg.clone()),
            OmniError::QueryTimeoutError(msg) => OmniError::QueryTimeoutError(msg.clone()),
            OmniError::QueryPollingError(msg) => OmniError::QueryPollingError(msg.clone()),
        }
    }
}

impl OmniError {
    /// Check if this error is temporary and might succeed if retried
    pub fn is_temporary(&self) -> bool {
        match self {
            OmniError::HttpError(_) => true,
            OmniError::ConnectionError(_) => true,
            OmniError::RateLimitError(_) => true,
            OmniError::ServerError(_) => true,
            OmniError::ApiError { status_code, .. } => *status_code >= 500,
            OmniError::QueryPollingError(_) => true, // Polling failures might be temporary network issues
            OmniError::QueryTimeoutError(_) => false, // Timeout errors indicate configuration or query complexity issues
            _ => false,
        }
    }

    /// Get suggested retry delay in seconds for temporary errors
    pub fn retry_delay_seconds(&self) -> Option<u64> {
        match self {
            OmniError::RateLimitError(_) => Some(60), // 1 minute for rate limits
            OmniError::ServerError(_) => Some(5),     // 5 seconds for server errors
            OmniError::HttpError(_) => Some(2),       // 2 seconds for HTTP errors
            OmniError::ConnectionError(_) => Some(3), // 3 seconds for connection errors
            OmniError::QueryPollingError(_) => Some(5), // 5 seconds for polling failures
            _ => None,
        }
    }

    /// Get user-friendly error message with context and suggestions
    pub fn user_friendly_message(&self) -> String {
        match self {
            OmniError::ApiError {
                message,
                status_code,
            } => {
                format!("Omni API returned an error (HTTP {status_code}): {message}")
            }
            OmniError::AuthenticationError(message) => {
                format!(
                    "Authentication failed: {message}\nSuggestion: Verify your API token is valid and has not expired"
                )
            }
            OmniError::SyncError(message) => {
                format!("Metadata synchronization failed: {message}")
            }
            OmniError::QueryError(message) => {
                format!("Query validation failed: {message}")
            }
            OmniError::ConfigError(message) => {
                format!(
                    "Configuration error: {message}\nSuggestion: Check your Omni integration settings"
                )
            }
            OmniError::ConnectionError(message) => {
                format!(
                    "Connection error: {message}\nTroubleshooting: Check that the server is running and accessible"
                )
            }
            OmniError::ValidationError(message) => {
                format!("Validation error: {message}")
            }
            OmniError::RateLimitError(message) => {
                format!("Rate limit exceeded: {message}\nSuggestion: Wait a moment before retrying")
            }
            OmniError::NotFoundError(message) => {
                format!("Resource not found: {message}")
            }
            OmniError::ServerError(message) => {
                format!(
                    "Server error: {message}\nNote: This may be a temporary issue, try again later"
                )
            }
            OmniError::QueryTimeoutError(message) => {
                format!(
                    "Query execution timed out: {message}\nSuggestion: Consider increasing timeout limits, simplifying the query, or using filters to reduce data volume"
                )
            }
            OmniError::QueryPollingError(message) => {
                format!(
                    "Query polling failed: {message}\nNote: This may be a temporary network issue, the operation will be retried automatically"
                )
            }
            _ => self.to_string(),
        }
    }

    /// Create a connection error with troubleshooting guidance
    pub fn connection_failed(url: &str, underlying_error: &str) -> Self {
        Self::ConnectionError(format!(
            "Failed to connect to Omni server at '{}': {}. \
            Verify the URL is correct and the service is running.",
            url, underlying_error
        ))
    }

    /// Create an authentication error with helpful guidance
    pub fn auth_failed(message: &str) -> Self {
        Self::AuthenticationError(format!(
            "{}. Verify your API token is valid and has the required permissions.",
            message
        ))
    }

    /// Create a configuration error with field-specific guidance
    pub fn config_invalid(field: &str, issue: &str) -> Self {
        let suggestion = match field {
            "base_url" => "Provide a valid Omni server URL (e.g., https://your-omni-instance.com)",
            "api_token" => "Generate an API token in your Omni account settings",
            "model_id" => "Specify the Omni model ID you want to connect to",
            _ => "Check your Omni configuration in the project settings",
        };

        Self::ConfigError(format!("{}: {}. Suggestion: {}", field, issue, suggestion))
    }

    /// Create a query validation error with guidance
    pub fn query_invalid(field: &str, issue: &str) -> Self {
        let suggestion = match field {
            "fields" => "Specify at least one field from the available dimensions or measures",
            "topic" => "Provide a valid topic name from your synchronized metadata",
            "limit" => "Set limit to a value between 1 and 10,000",
            "sorts" => "Use valid field names and sort directions (ASC/DESC)",
            _ => "Check the query parameters against available metadata",
        };

        Self::QueryError(format!("{}: {}. Suggestion: {}", field, issue, suggestion))
    }

    /// Create a sync error with topic context
    pub fn sync_failed(topic: &str, operation: &str, cause: &str) -> Self {
        Self::SyncError(format!(
            "Failed to {} topic '{}': {}",
            operation, topic, cause
        ))
    }

    /// Create a validation error with context
    pub fn validation_failed(context: &str, issue: &str) -> Self {
        Self::ValidationError(format!("{}: {}", context, issue))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_error_types() {
        let timeout_error =
            OmniError::QueryTimeoutError("Query exceeded 5 minute limit".to_string());
        let polling_error =
            OmniError::QueryPollingError("Network error during polling".to_string());

        // Test error display
        assert!(timeout_error.to_string().contains("Query timeout"));
        assert!(polling_error.to_string().contains("Query polling failed"));

        // Test is_temporary behavior
        assert!(!timeout_error.is_temporary()); // Timeout errors are not temporary
        assert!(polling_error.is_temporary()); // Polling errors are temporary

        // Test retry delay
        assert_eq!(timeout_error.retry_delay_seconds(), None);
        assert_eq!(polling_error.retry_delay_seconds(), Some(5));

        // Test user-friendly messages
        let timeout_message = timeout_error.user_friendly_message();
        let polling_message = polling_error.user_friendly_message();

        assert!(timeout_message.contains("Query execution timed out"));
        assert!(timeout_message.contains("increasing timeout limits"));
        assert!(polling_message.contains("Query polling failed"));
        assert!(polling_message.contains("retried automatically"));
    }

    #[test]
    fn test_timeout_error_cloning() {
        let timeout_error = OmniError::QueryTimeoutError("Test timeout".to_string());
        let polling_error = OmniError::QueryPollingError("Test polling".to_string());

        let cloned_timeout = timeout_error.clone();
        let cloned_polling = polling_error.clone();

        assert_eq!(timeout_error.to_string(), cloned_timeout.to_string());
        assert_eq!(polling_error.to_string(), cloned_polling.to_string());
    }

    #[test]
    fn test_timeout_error_classification() {
        // Test QueryTimeoutError classification
        let timeout_error =
            OmniError::QueryTimeoutError("Polling exceeded maximum attempts".to_string());
        assert!(!timeout_error.is_temporary());
        assert_eq!(timeout_error.retry_delay_seconds(), None);

        // Test QueryPollingError classification
        let polling_error = OmniError::QueryPollingError("HTTP connection failed".to_string());
        assert!(polling_error.is_temporary());
        assert_eq!(polling_error.retry_delay_seconds(), Some(5));

        // Test other error types for comparison
        let server_error = OmniError::ServerError("Internal server error".to_string());
        assert!(server_error.is_temporary());
        assert_eq!(server_error.retry_delay_seconds(), Some(5));

        let auth_error = OmniError::AuthenticationError("Invalid token".to_string());
        assert!(!auth_error.is_temporary());
        assert_eq!(auth_error.retry_delay_seconds(), None);
    }

    #[test]
    fn test_timeout_error_user_friendly_messages() {
        // Test QueryTimeoutError user-friendly message
        let timeout_error =
            OmniError::QueryTimeoutError("Query exceeded 5 minute limit".to_string());
        let message = timeout_error.user_friendly_message();

        assert!(message.contains("Query execution timed out"));
        assert!(message.contains("Query exceeded 5 minute limit"));
        assert!(message.contains("increasing timeout limits"));
        assert!(message.contains("simplifying the query"));
        assert!(message.contains("using filters"));

        // Test QueryPollingError user-friendly message
        let polling_error =
            OmniError::QueryPollingError("Network timeout during polling".to_string());
        let message = polling_error.user_friendly_message();

        assert!(message.contains("Query polling failed"));
        assert!(message.contains("Network timeout during polling"));
        assert!(message.contains("temporary network issue"));
        assert!(message.contains("retried automatically"));
    }

    #[test]
    fn test_all_error_types_is_temporary() {
        // Test temporary errors (using ConnectionError instead of HttpError for simplicity)
        assert!(OmniError::ConnectionError("Connection failed".to_string()).is_temporary());
        assert!(OmniError::RateLimitError("Rate limit exceeded".to_string()).is_temporary());
        assert!(OmniError::ServerError("Internal server error".to_string()).is_temporary());
        assert!(OmniError::QueryPollingError("Polling failed".to_string()).is_temporary());
        assert!(
            OmniError::ApiError {
                message: "Server error".to_string(),
                status_code: 500
            }
            .is_temporary()
        );
        assert!(
            OmniError::ApiError {
                message: "Bad gateway".to_string(),
                status_code: 502
            }
            .is_temporary()
        );

        // Test non-temporary errors
        assert!(!OmniError::AuthenticationError("Invalid token".to_string()).is_temporary());
        assert!(!OmniError::SyncError("Sync failed".to_string()).is_temporary());
        assert!(!OmniError::QueryError("Invalid query".to_string()).is_temporary());
        assert!(!OmniError::ConfigError("Invalid config".to_string()).is_temporary());
        assert!(!OmniError::ValidationError("Validation failed".to_string()).is_temporary());
        assert!(!OmniError::NotFoundError("Resource not found".to_string()).is_temporary());
        assert!(!OmniError::QueryTimeoutError("Query timed out".to_string()).is_temporary());
        assert!(
            !OmniError::ApiError {
                message: "Bad request".to_string(),
                status_code: 400
            }
            .is_temporary()
        );
        assert!(
            !OmniError::ApiError {
                message: "Unauthorized".to_string(),
                status_code: 401
            }
            .is_temporary()
        );
        assert!(
            !OmniError::ApiError {
                message: "Not found".to_string(),
                status_code: 404
            }
            .is_temporary()
        );
    }

    #[test]
    fn test_retry_delay_seconds() {
        assert_eq!(
            OmniError::RateLimitError("Rate limit".to_string()).retry_delay_seconds(),
            Some(60)
        );
        assert_eq!(
            OmniError::ServerError("Server error".to_string()).retry_delay_seconds(),
            Some(5)
        );
        assert_eq!(
            OmniError::ConnectionError("Connection failed".to_string()).retry_delay_seconds(),
            Some(3)
        );
        assert_eq!(
            OmniError::QueryPollingError("Polling failed".to_string()).retry_delay_seconds(),
            Some(5)
        );

        // Non-temporary errors should return None
        assert_eq!(
            OmniError::AuthenticationError("Auth failed".to_string()).retry_delay_seconds(),
            None
        );
        assert_eq!(
            OmniError::QueryTimeoutError("Timeout".to_string()).retry_delay_seconds(),
            None
        );
        assert_eq!(
            OmniError::ValidationError("Validation failed".to_string()).retry_delay_seconds(),
            None
        );
    }

    #[test]
    fn test_error_helper_constructors() {
        // Test connection_failed helper
        let conn_error =
            OmniError::connection_failed("https://api.example.com", "Connection refused");
        assert!(
            conn_error
                .to_string()
                .contains("Failed to connect to Omni server")
        );
        assert!(conn_error.to_string().contains("https://api.example.com"));
        assert!(conn_error.to_string().contains("Connection refused"));
        assert!(conn_error.to_string().contains("Verify the URL is correct"));

        // Test auth_failed helper
        let auth_error = OmniError::auth_failed("Token expired");
        assert!(auth_error.to_string().contains("Token expired"));
        assert!(
            auth_error
                .to_string()
                .contains("Verify your API token is valid")
        );

        // Test config_invalid helper
        let config_error = OmniError::config_invalid("base_url", "Invalid URL format");
        assert!(
            config_error
                .to_string()
                .contains("base_url: Invalid URL format")
        );
        assert!(config_error.to_string().contains("valid Omni server URL"));

        let token_config_error = OmniError::config_invalid("api_token", "Token is empty");
        assert!(
            token_config_error
                .to_string()
                .contains("Generate an API token")
        );

        // Test query_invalid helper
        let query_error = OmniError::query_invalid("fields", "No fields specified");
        assert!(
            query_error
                .to_string()
                .contains("fields: No fields specified")
        );
        assert!(query_error.to_string().contains("at least one field"));

        let limit_query_error = OmniError::query_invalid("limit", "Limit too high");
        assert!(
            limit_query_error
                .to_string()
                .contains("between 1 and 10,000")
        );

        // Test sync_failed helper
        let sync_error = OmniError::sync_failed("users", "fetch metadata", "API timeout");
        assert!(
            sync_error
                .to_string()
                .contains("Failed to fetch metadata topic 'users'")
        );
        assert!(sync_error.to_string().contains("API timeout"));

        // Test validation_failed helper
        let validation_error =
            OmniError::validation_failed("TimeoutConfig", "Invalid polling interval");
        assert!(
            validation_error
                .to_string()
                .contains("TimeoutConfig: Invalid polling interval")
        );
    }

    #[test]
    fn test_error_cloning_comprehensive() {
        let errors = vec![
            OmniError::ApiError {
                message: "API error".to_string(),
                status_code: 500,
            },
            OmniError::AuthenticationError("Auth error".to_string()),
            OmniError::SyncError("Sync error".to_string()),
            OmniError::QueryError("Query error".to_string()),
            OmniError::ConfigError("Config error".to_string()),
            OmniError::StorageError("Storage error".to_string()),
            OmniError::ValidationError("Validation error".to_string()),
            OmniError::ConnectionError("Connection error".to_string()),
            OmniError::RateLimitError("Rate limit error".to_string()),
            OmniError::NotFoundError("Not found error".to_string()),
            OmniError::ServerError("Server error".to_string()),
            OmniError::QueryTimeoutError("Timeout error".to_string()),
            OmniError::QueryPollingError("Polling error".to_string()),
        ];

        for error in errors {
            let cloned = error.clone();
            assert_eq!(error.to_string(), cloned.to_string());
            assert_eq!(error.is_temporary(), cloned.is_temporary());
            assert_eq!(error.retry_delay_seconds(), cloned.retry_delay_seconds());
        }
    }

    #[test]
    fn test_error_conversion_from_foreign_types() {
        // Test IO error cloning
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let omni_io_error = OmniError::IoError(io_error);
        let cloned_io = omni_io_error.clone();

        match cloned_io {
            OmniError::StorageError(msg) => {
                assert!(msg.contains("IO error"));
                assert!(msg.contains("File not found"));
            }
            _ => panic!("Expected StorageError after cloning IoError"),
        }

        // Test serialization error cloning using a simple JSON parsing error
        let invalid_json = "{ invalid json }";
        let json_result: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_str(invalid_json);
        if let Err(json_error) = json_result {
            let ser_error = OmniError::SerializationError(json_error);
            let cloned_ser = ser_error.clone();

            match cloned_ser {
                OmniError::ValidationError(msg) => {
                    assert!(msg.contains("Serialization error"));
                }
                _ => panic!("Expected ValidationError after cloning SerializationError"),
            }
        }
    }

    #[test]
    fn test_timeout_scenarios_user_messages() {
        // Test different timeout scenarios
        let scenarios = vec![
            (
                "Query exceeded maximum polling attempts",
                "increasing timeout limits",
            ),
            (
                "Total timeout of 300 seconds exceeded",
                "simplifying the query",
            ),
            (
                "Polling failed after 20 attempts",
                "using filters to reduce data volume",
            ),
        ];

        for (error_msg, expected_suggestion) in scenarios {
            let timeout_error = OmniError::QueryTimeoutError(error_msg.to_string());
            let user_message = timeout_error.user_friendly_message();

            assert!(user_message.contains("Query execution timed out"));
            assert!(user_message.contains(error_msg));
            assert!(user_message.contains(expected_suggestion));
        }

        // Test different polling error scenarios
        let polling_scenarios = vec![
            ("Network connection timeout", "temporary network issue"),
            ("HTTP 503 Service Unavailable", "retried automatically"),
            ("Connection reset by peer", "temporary network issue"),
        ];

        for (error_msg, expected_context) in polling_scenarios {
            let polling_error = OmniError::QueryPollingError(error_msg.to_string());
            let user_message = polling_error.user_friendly_message();

            assert!(user_message.contains("Query polling failed"));
            assert!(user_message.contains(error_msg));
            assert!(user_message.contains(expected_context));
        }
    }
}
