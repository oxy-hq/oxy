//! Shared retry policy for LLM calls.
//!
//! The analytics and builder solvers both retry LLM failures on the same two
//! independent budgets — rate limits (429) and transient transport errors
//! (connection reset, timeout, HTTP 5xx). The caps, backoff bases, and the
//! jittered backoff function used to be copy-pasted into each solver, so a
//! policy fix to one never propagated to the other. They live here now, in the
//! infrastructure crate both domains already depend on, so there is exactly
//! one definition.
//!
//! This module owns the *policy numbers and delay math* only — the retry
//! control flow (which back-target to re-enter, which suspension to raise)
//! stays in each domain because it is expressed in domain-specific error and
//! state types. Pair these with [`crate::constants::parse_retry_after`] (via
//! [`LlmError::RateLimit::retry_after`]) at the call site.

/// Maximum number of rate-limit (429) retries before surfacing the failure to
/// the user.
pub const MAX_RATE_LIMIT_RETRIES: u32 = 5;

/// Base delay in seconds for rate-limit exponential backoff: `BASE * 2^attempt`.
pub const RATE_LIMIT_BACKOFF_BASE_SECS: u64 = 5;

/// Maximum number of retries for transient LLM failures (connection errors,
/// timeouts, HTTP 5xx) before surfacing the failure to the user.
pub const MAX_TRANSIENT_RETRIES: u32 = 3;

/// Base delay in seconds for transient-error exponential backoff.  Smaller than
/// the rate-limit base because a transient network blip usually clears fast.
pub const TRANSIENT_BACKOFF_BASE_SECS: f64 = 1.0;

/// Computed exponential backoff in whole seconds for a rate-limit retry
/// `attempt`: `BASE * 2^attempt`, with the shift capped so an out-of-range
/// attempt can never overflow. Callers prefer a provider `Retry-After` hint
/// over this when present.
pub fn rate_limit_backoff_secs(attempt: u32) -> u64 {
    RATE_LIMIT_BACKOFF_BASE_SECS * (1u64 << attempt.min(6)).min(64)
}

/// Exponential backoff with full jitter for a transient retry `attempt`.
///
/// Returns `BASE * 2^attempt` scaled by a random factor in `[0.5, 1.5)` so
/// concurrent clients don't retry in lock-step.  The jitter is derived from the
/// wall-clock nanosecond fraction to avoid pulling in a `rand` dependency.
pub fn transient_backoff_secs(attempt: u32) -> f64 {
    let base = TRANSIENT_BACKOFF_BASE_SECS * 2f64.powi(attempt.min(6) as i32);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let jitter = 0.5 + (nanos % 1_000_000_000) as f64 / 1_000_000_000.0;
    base * jitter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_backoff_doubles_per_attempt() {
        // BASE * 2^attempt for the real (0..MAX) range.
        assert_eq!(rate_limit_backoff_secs(0), 5);
        assert_eq!(rate_limit_backoff_secs(1), 10);
        assert_eq!(rate_limit_backoff_secs(2), 20);
        assert_eq!(rate_limit_backoff_secs(3), 40);
        assert_eq!(rate_limit_backoff_secs(4), 80);
    }

    #[test]
    fn rate_limit_backoff_does_not_overflow_out_of_range() {
        // An absurd attempt count must not panic on shift overflow.
        let _ = rate_limit_backoff_secs(u32::MAX);
    }

    #[test]
    fn transient_backoff_within_jitter_bounds() {
        for attempt in 0..=MAX_TRANSIENT_RETRIES {
            let base = TRANSIENT_BACKOFF_BASE_SECS * 2f64.powi(attempt as i32);
            let d = transient_backoff_secs(attempt);
            assert!(d >= base * 0.5, "attempt {attempt}: {d} < {}", base * 0.5);
            assert!(d < base * 1.5, "attempt {attempt}: {d} >= {}", base * 1.5);
        }
    }
}
