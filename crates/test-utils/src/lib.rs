//! Test utilities, fixtures, and mocks for Oxy crates.
//!
//! This crate provides shared test infrastructure including:
//! - Mock implementations for external services (LLM APIs, Docker, etc.)
//! - Fixture management utilities
//! - Common test helpers

pub mod fixtures;
pub mod mocks;

// Re-export commonly used test dependencies for convenience
pub use serial_test::serial;
pub use tempfile::{Builder as TempFileBuilder, TempDir};
pub use wiremock::matchers::{body_json, method, path};
pub use wiremock::{Mock, MockServer, ResponseTemplate};

/// Convenience macro for skipping tests when an environment variable is not set.
///
/// # Example
/// ```ignore
/// skip_if_env_missing!("OPENAI_API_KEY");
/// ```
#[macro_export]
macro_rules! skip_if_env_missing {
    ($var:expr) => {
        if std::env::var($var).is_err() {
            eprintln!("Skipping test: {} not set", $var);
            return;
        }
    };
}

/// Convenience macro for skipping async tests when an environment variable is not set.
#[macro_export]
macro_rules! skip_if_env_missing_async {
    ($var:expr) => {
        if std::env::var($var).is_err() {
            eprintln!("Skipping test: {} not set", $var);
            return Ok(());
        }
    };
}
