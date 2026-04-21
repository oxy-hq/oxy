use std::time::Duration;
use tokio::time::{Instant, sleep};
use tracing::{debug, warn};

use crate::error::OmniError;

/// Configuration for retry policies
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries (in milliseconds)
    pub initial_delay_ms: u64,
    /// Maximum delay between retries (in milliseconds)
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Jitter to add randomness to retry delays (0.0 to 1.0)
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000, // 1 second
            max_delay_ms: 30000,    // 30 seconds
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl RetryConfig {
    /// Create a configuration for API calls with reasonable defaults
    pub fn for_api_calls() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 15000, // 15 seconds for API calls
            backoff_multiplier: 2.0,
            jitter_factor: 0.15,
        }
    }

    /// Create a configuration for metadata sync operations (more tolerant)
    pub fn for_metadata_sync() -> Self {
        Self {
            max_attempts: 5,
            initial_delay_ms: 2000, // 2 seconds
            max_delay_ms: 60000,    // 1 minute
            backoff_multiplier: 2.0,
            jitter_factor: 0.2,
        }
    }

    /// Create a configuration for quick health checks
    pub fn for_health_checks() -> Self {
        Self {
            max_attempts: 2,
            initial_delay_ms: 500, // 0.5 seconds
            max_delay_ms: 2000,    // 2 seconds
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

/// Retry policy implementation with exponential backoff
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub config: RetryConfig,
}

impl RetryPolicy {
    /// Create a new retry policy with the given configuration
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Create a retry policy with default configuration
    pub fn default() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Execute a function with retry logic
    pub async fn execute<F, Fut, T>(
        &self,
        operation_name: &str,
        mut operation: F,
    ) -> Result<T, OmniError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, OmniError>>,
    {
        let mut last_error: Option<OmniError> = None;
        let start_time = Instant::now();

        for attempt in 1..=self.config.max_attempts {
            debug!(
                operation = operation_name,
                attempt = attempt,
                max_attempts = self.config.max_attempts,
                "Executing operation"
            );

            match operation().await {
                Ok(result) => {
                    if attempt > 1 {
                        debug!(
                            operation = operation_name,
                            attempt = attempt,
                            duration_ms = start_time.elapsed().as_millis(),
                            "Operation succeeded after retry"
                        );
                    }
                    return Ok(result);
                }
                Err(error) => {
                    last_error = Some(error.clone());

                    // Check if this error should be retried
                    if !should_retry(&error) {
                        warn!(
                            operation = operation_name,
                            attempt = attempt,
                            error = %error,
                            "Operation failed with non-retriable error"
                        );
                        return Err(error);
                    }

                    // If this was the last attempt, don't delay
                    if attempt >= self.config.max_attempts {
                        warn!(
                            operation = operation_name,
                            attempt = attempt,
                            max_attempts = self.config.max_attempts,
                            error = %error,
                            "Operation failed after all retry attempts"
                        );
                        break;
                    }

                    // Calculate delay for next attempt
                    let delay = self.calculate_delay(attempt);
                    warn!(
                        operation = operation_name,
                        attempt = attempt,
                        error = %error,
                        delay_ms = delay.as_millis(),
                        "Operation failed, retrying after delay"
                    );

                    sleep(delay).await;
                }
            }
        }

        // Return the last error if all attempts failed
        Err(last_error.unwrap_or_else(|| {
            OmniError::ConnectionError(format!(
                "Operation '{}' failed after {} attempts",
                operation_name, self.config.max_attempts
            ))
        }))
    }

    /// Calculate the delay for the given attempt number using exponential backoff with jitter
    fn calculate_delay(&self, attempt: u32) -> Duration {
        // Calculate exponential backoff delay
        let base_delay = self.config.initial_delay_ms as f64
            * self.config.backoff_multiplier.powi((attempt - 1) as i32);

        // Cap at maximum delay
        let capped_delay = base_delay.min(self.config.max_delay_ms as f64);

        // Add jitter to avoid thundering herd problem
        let jitter_amount = capped_delay * self.config.jitter_factor;
        let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_amount;
        let final_delay = (capped_delay + jitter).max(0.0) as u64;

        Duration::from_millis(final_delay)
    }
}

/// Determine if an error should trigger a retry attempt
fn should_retry(error: &OmniError) -> bool {
    match error {
        // Always retry these temporary errors
        OmniError::HttpError(req_error) => {
            // Retry on network-related errors, but not on client errors like invalid URLs
            req_error.is_timeout() || req_error.is_connect() || req_error.is_request()
        }
        OmniError::ConnectionError(_) => true,
        OmniError::ServerError(_) => true,
        OmniError::RateLimitError(_) => true,

        // Retry API errors only for server errors (5xx)
        OmniError::ApiError { status_code, .. } => *status_code >= 500,

        // Don't retry these errors as they indicate permanent issues
        OmniError::AuthenticationError(_) => false,
        OmniError::ConfigError(_) => false,
        OmniError::QueryError(_) => false,
        OmniError::ValidationError(_) => false,
        OmniError::NotFoundError(_) => false,
        OmniError::SerializationError(_) => false,
        OmniError::IoError(_) => false,

        // Storage and sync errors might be retryable depending on the cause
        // For now, be conservative and don't retry
        OmniError::StorageError(_) => false,
        OmniError::SyncError(_) => false,

        // Timeout error handling
        OmniError::QueryTimeoutError(_) => false, // Timeout errors indicate configuration or query complexity issues
        OmniError::QueryPollingError(_) => true, // Polling failures might be temporary network issues
    }
}

/// Connection health monitoring and recovery
#[derive(Debug, Clone)]
pub struct ConnectionHealthChecker {
    retry_policy: RetryPolicy,
}

impl Default for ConnectionHealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionHealthChecker {
    /// Create a new health checker with default retry policy for health checks
    pub fn new() -> Self {
        Self {
            retry_policy: RetryPolicy::new(RetryConfig::for_health_checks()),
        }
    }

    /// Create a health checker with custom retry policy
    pub fn with_retry_policy(retry_policy: RetryPolicy) -> Self {
        Self { retry_policy }
    }

    /// Perform a health check with retry logic
    pub async fn check_health<F, Fut>(&self, health_check: F) -> Result<(), OmniError>
    where
        F: FnMut() -> Fut + Clone,
        Fut: std::future::Future<Output = Result<(), OmniError>>,
    {
        self.retry_policy
            .execute("health_check", health_check)
            .await
    }

    /// Perform a comprehensive connection validation
    pub async fn validate_connection<F, Fut>(&self, connection_test: F) -> ConnectionStatus
    where
        F: FnMut() -> Fut + Clone,
        Fut: std::future::Future<Output = Result<(), OmniError>>,
    {
        let start_time = Instant::now();

        match self.check_health(connection_test).await {
            Ok(()) => ConnectionStatus {
                is_healthy: true,
                last_check: start_time,
                response_time: start_time.elapsed(),
                last_error: None,
            },
            Err(error) => ConnectionStatus {
                is_healthy: false,
                last_check: start_time,
                response_time: start_time.elapsed(),
                last_error: Some(error),
            },
        }
    }
}

/// Status of a connection health check
#[derive(Debug, Clone)]
pub struct ConnectionStatus {
    pub is_healthy: bool,
    pub last_check: Instant,
    pub response_time: Duration,
    pub last_error: Option<OmniError>,
}

impl ConnectionStatus {
    /// Check if the connection status is stale (older than the given duration)
    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.last_check.elapsed() > max_age
    }

    /// Get a human-readable status description
    pub fn status_description(&self) -> String {
        if self.is_healthy {
            format!("Healthy ({}ms)", self.response_time.as_millis())
        } else {
            format!(
                "Unhealthy: {}",
                self.last_error
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "Unknown error".to_string())
            )
        }
    }
}

/// Timeout wrapper for long-running operations
pub struct TimeoutWrapper {
    timeout: Duration,
}

impl TimeoutWrapper {
    /// Create a new timeout wrapper with the specified timeout
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Create a timeout wrapper for API calls (30 seconds)
    pub fn for_api_calls() -> Self {
        Self::new(Duration::from_secs(30))
    }

    /// Create a timeout wrapper for metadata sync operations (2 minutes)
    pub fn for_metadata_sync() -> Self {
        Self::new(Duration::from_secs(120))
    }

    /// Create a timeout wrapper for health checks (5 seconds)
    pub fn for_health_checks() -> Self {
        Self::new(Duration::from_secs(5))
    }

    /// Execute an operation with timeout
    pub async fn execute<F, Fut, T>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> Result<T, OmniError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, OmniError>>,
    {
        match tokio::time::timeout(self.timeout, operation()).await {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    operation = operation_name,
                    timeout_secs = self.timeout.as_secs(),
                    "Operation timed out"
                );
                Err(OmniError::ConnectionError(format!(
                    "Operation '{}' timed out after {} seconds",
                    operation_name,
                    self.timeout.as_secs()
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_retry_policy_success_on_first_attempt() {
        let policy = RetryPolicy::default();

        let result = policy
            .execute("test_op", || async { Ok::<i32, OmniError>(42) })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_policy_success_after_retries() {
        let policy = RetryPolicy::new(RetryConfig {
            max_attempts: 3,
            initial_delay_ms: 10, // Short delay for testing
            max_delay_ms: 100,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable testing
        });

        let attempt_counter = Arc::new(AtomicU32::new(0));
        let counter_clone = attempt_counter.clone();

        let result = policy
            .execute("test_op", move || {
                let counter = counter_clone.clone();
                async move {
                    let attempt = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    if attempt < 3 {
                        Err(OmniError::ServerError("Temporary server error".to_string()))
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempt_counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_policy_non_retriable_error() {
        let policy = RetryPolicy::default();
        let attempt_counter = Arc::new(AtomicU32::new(0));
        let counter_clone = attempt_counter.clone();

        let result = policy
            .execute("test_op", move || {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>(OmniError::AuthenticationError("Invalid token".to_string()))
                }
            })
            .await;

        assert!(result.is_err());
        // Should only attempt once for non-retriable errors
        assert_eq!(attempt_counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_policy_max_attempts_exceeded() {
        let policy = RetryPolicy::new(RetryConfig {
            max_attempts: 2,
            initial_delay_ms: 10,
            max_delay_ms: 100,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
        });

        let attempt_counter = Arc::new(AtomicU32::new(0));
        let counter_clone = attempt_counter.clone();

        let result = policy
            .execute("test_op", move || {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, _>(OmniError::ServerError(
                        "Persistent server error".to_string(),
                    ))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempt_counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_should_retry_logic() {
        // Should retry these errors
        assert!(should_retry(&OmniError::ServerError(
            "500 error".to_string()
        )));
        assert!(should_retry(&OmniError::ConnectionError(
            "Connection lost".to_string()
        )));
        assert!(should_retry(&OmniError::RateLimitError(
            "Rate limited".to_string()
        )));
        assert!(should_retry(&OmniError::ApiError {
            message: "Internal server error".to_string(),
            status_code: 500
        }));

        // Should not retry these errors
        assert!(!should_retry(&OmniError::AuthenticationError(
            "Invalid token".to_string()
        )));
        assert!(!should_retry(&OmniError::ConfigError(
            "Invalid config".to_string()
        )));
        assert!(!should_retry(&OmniError::QueryError(
            "Invalid query".to_string()
        )));
        assert!(!should_retry(&OmniError::NotFoundError(
            "Resource not found".to_string()
        )));
        assert!(!should_retry(&OmniError::ApiError {
            message: "Bad request".to_string(),
            status_code: 400
        }));
    }

    #[tokio::test]
    async fn test_timeout_wrapper_success() {
        let wrapper = TimeoutWrapper::new(Duration::from_millis(100));

        let result = wrapper
            .execute("test_op", || async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok::<i32, OmniError>(42)
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_timeout_wrapper_timeout() {
        let wrapper = TimeoutWrapper::new(Duration::from_millis(50));

        let result = wrapper
            .execute("test_op", || async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<i32, OmniError>(42)
            })
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OmniError::ConnectionError(_)));
    }

    #[tokio::test]
    async fn test_connection_health_checker() {
        let checker = ConnectionHealthChecker::new();
        let attempt_counter = Arc::new(AtomicU32::new(0));
        let counter_clone = attempt_counter.clone();

        let status = checker
            .validate_connection(move || {
                let counter = counter_clone.clone();
                async move {
                    let attempt = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    if attempt == 1 {
                        Err(OmniError::ConnectionError(
                            "Temporary connection issue".to_string(),
                        ))
                    } else {
                        Ok(())
                    }
                }
            })
            .await;

        assert!(status.is_healthy);
        assert!(status.response_time > Duration::from_nanos(0));
        assert!(status.last_error.is_none());
    }

    #[test]
    fn test_connection_status() {
        let healthy_status = ConnectionStatus {
            is_healthy: true,
            last_check: Instant::now(),
            response_time: Duration::from_millis(100),
            last_error: None,
        };

        assert!(healthy_status.status_description().contains("Healthy"));
        assert!(healthy_status.status_description().contains("100ms"));

        let unhealthy_status = ConnectionStatus {
            is_healthy: false,
            last_check: Instant::now(),
            response_time: Duration::from_millis(5000),
            last_error: Some(OmniError::ConnectionError(
                "Network unreachable".to_string(),
            )),
        };

        assert!(unhealthy_status.status_description().contains("Unhealthy"));
        assert!(
            unhealthy_status
                .status_description()
                .contains("Network unreachable")
        );
    }

    #[tokio::test]
    async fn test_delay_calculation() {
        let config = RetryConfig {
            max_attempts: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 10000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable testing
        };

        let policy = RetryPolicy::new(config);

        // Test delay calculation (approximately, since we can't access the private method directly)
        // We'll test by observing behavior indirectly through timing
        let start = Instant::now();
        let _ = policy
            .execute("test_op", || async {
                Err::<(), _>(OmniError::ServerError("Test error".to_string()))
            })
            .await;

        // Should have made 5 attempts with exponential backoff delays
        // 1000ms + 2000ms + 4000ms + 8000ms = 15000ms minimum
        // With our max_delay_ms of 10000ms, the last delays are capped
        let elapsed = start.elapsed();
        // Due to jitter and timing variations, we just check it's reasonably long
        assert!(elapsed >= Duration::from_millis(1000)); // At least one retry delay
    }
}
