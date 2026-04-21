//! Circuit breaker for delegation targets.
//!
//! Tracks consecutive failures per target and prevents delegation to targets
//! that are likely broken. This avoids wasting time and resources on targets
//! that will just fail again.
//!
//! States:
//! - **Closed** (normal): requests flow through.
//! - **Open**: requests are immediately rejected.
//! - **HalfOpen**: one probe request is allowed to test recovery.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Process-level circuit breaker keyed by target identifier.
pub struct CircuitBreaker {
    states: DashMap<String, CircuitState>,
    /// Number of consecutive failures before the circuit opens.
    failure_threshold: u32,
    /// How long the circuit stays open before transitioning to half-open.
    reset_timeout: Duration,
}

#[derive(Debug, Clone)]
struct CircuitState {
    status: CircuitStatus,
    consecutive_failures: u32,
    last_failure_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq)]
enum CircuitStatus {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            states: DashMap::new(),
            failure_threshold,
            reset_timeout,
        })
    }

    /// Create with reasonable defaults (5 failures, 60s reset).
    pub fn with_defaults() -> Arc<Self> {
        Self::new(5, Duration::from_secs(60))
    }

    /// Check if a request to `target_key` should be allowed.
    ///
    /// Returns `Ok(())` if allowed, `Err(reason)` if the circuit is open.
    pub fn check(&self, target_key: &str) -> Result<(), String> {
        let mut entry = self
            .states
            .entry(target_key.to_string())
            .or_insert_with(|| CircuitState {
                status: CircuitStatus::Closed,
                consecutive_failures: 0,
                last_failure_at: None,
            });

        let state = entry.value_mut();
        match state.status {
            CircuitStatus::Closed => Ok(()),
            CircuitStatus::Open => {
                // Check if reset timeout has elapsed → transition to half-open.
                if let Some(last) = state.last_failure_at {
                    if last.elapsed() >= self.reset_timeout {
                        state.status = CircuitStatus::HalfOpen;
                        return Ok(());
                    }
                }
                Err(format!(
                    "circuit breaker open for '{target_key}': {} consecutive failures",
                    state.consecutive_failures
                ))
            }
            CircuitStatus::HalfOpen => {
                // Allow one probe request.
                Ok(())
            }
        }
    }

    /// Record a successful request to `target_key`. Resets the circuit to closed.
    pub fn record_success(&self, target_key: &str) {
        if let Some(mut entry) = self.states.get_mut(target_key) {
            entry.status = CircuitStatus::Closed;
            entry.consecutive_failures = 0;
            entry.last_failure_at = None;
        }
    }

    /// Record a failed request to `target_key`.
    /// May transition the circuit from closed/half-open to open.
    pub fn record_failure(&self, target_key: &str) {
        let mut entry = self
            .states
            .entry(target_key.to_string())
            .or_insert_with(|| CircuitState {
                status: CircuitStatus::Closed,
                consecutive_failures: 0,
                last_failure_at: None,
            });

        let state = entry.value_mut();
        state.consecutive_failures += 1;
        state.last_failure_at = Some(Instant::now());

        match state.status {
            CircuitStatus::Closed => {
                if state.consecutive_failures >= self.failure_threshold {
                    state.status = CircuitStatus::Open;
                    tracing::warn!(
                        target: "circuit_breaker",
                        target_key,
                        failures = state.consecutive_failures,
                        "circuit opened"
                    );
                }
            }
            CircuitStatus::HalfOpen => {
                // Probe failed — back to open.
                state.status = CircuitStatus::Open;
                tracing::warn!(
                    target: "circuit_breaker",
                    target_key,
                    "half-open probe failed, circuit re-opened"
                );
            }
            CircuitStatus::Open => {
                // Already open — just update the timestamp.
            }
        }
    }

    /// Get the current status of a target (for observability).
    pub fn status(&self, target_key: &str) -> &'static str {
        self.states
            .get(target_key)
            .map(|s| match s.status {
                CircuitStatus::Closed => "closed",
                CircuitStatus::Open => "open",
                CircuitStatus::HalfOpen => "half_open",
            })
            .unwrap_or("closed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closed_allows_requests() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));
        assert!(cb.check("agent:analytics").is_ok());
    }

    #[test]
    fn test_opens_after_threshold() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));
        let key = "agent:analytics";

        cb.record_failure(key);
        cb.record_failure(key);
        assert!(
            cb.check(key).is_ok(),
            "should still be closed after 2 failures"
        );

        cb.record_failure(key);
        assert!(cb.check(key).is_err(), "should be open after 3 failures");
        assert_eq!(cb.status(key), "open");
    }

    #[test]
    fn test_success_resets() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));
        let key = "agent:analytics";

        cb.record_failure(key);
        cb.record_failure(key);
        cb.record_success(key);

        assert!(cb.check(key).is_ok(), "should be closed after success");
        assert_eq!(cb.status(key), "closed");
    }

    #[test]
    fn test_half_open_after_timeout() {
        let cb = CircuitBreaker::new(2, Duration::from_millis(1));
        let key = "workflow:test";

        cb.record_failure(key);
        cb.record_failure(key);
        assert!(cb.check(key).is_err());

        // Wait for reset timeout.
        std::thread::sleep(Duration::from_millis(5));

        // Should transition to half-open and allow the probe.
        assert!(cb.check(key).is_ok());
        assert_eq!(cb.status(key), "half_open");
    }

    #[test]
    fn test_half_open_probe_failure_reopens() {
        let cb = CircuitBreaker::new(2, Duration::from_millis(1));
        let key = "workflow:test";

        cb.record_failure(key);
        cb.record_failure(key);
        std::thread::sleep(Duration::from_millis(5));

        // Transition to half-open.
        assert!(cb.check(key).is_ok());

        // Probe fails — back to open.
        cb.record_failure(key);
        assert!(cb.check(key).is_err());
        assert_eq!(cb.status(key), "open");
    }

    #[test]
    fn test_half_open_probe_success_closes() {
        let cb = CircuitBreaker::new(2, Duration::from_millis(1));
        let key = "workflow:test";

        cb.record_failure(key);
        cb.record_failure(key);
        std::thread::sleep(Duration::from_millis(5));

        // Transition to half-open.
        assert!(cb.check(key).is_ok());

        // Probe succeeds — circuit closes.
        cb.record_success(key);
        assert!(cb.check(key).is_ok());
        assert_eq!(cb.status(key), "closed");
    }

    #[test]
    fn test_independent_targets() {
        let cb = CircuitBreaker::new(2, Duration::from_secs(60));

        cb.record_failure("agent:a");
        cb.record_failure("agent:a");
        assert!(cb.check("agent:a").is_err());
        assert!(
            cb.check("agent:b").is_ok(),
            "different target should be unaffected"
        );
    }
}
