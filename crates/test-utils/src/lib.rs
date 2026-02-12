//! Test utilities, fixtures, and mocks for Oxy crates.
//!
//! This crate provides shared test infrastructure including:
//! - Mock implementations for external services (LLM APIs, Docker, etc.)
//! - Fixture management utilities
//! - Common test helpers

use std::path::PathBuf;

pub mod fixtures;
pub mod mocks;

// Re-export commonly used test dependencies for convenience
pub use serial_test::serial;
pub use tempfile::{Builder as TempFileBuilder, TempDir};
pub use wiremock::matchers::{body_json, method, path};
pub use wiremock::{Mock, MockServer, ResponseTemplate};

/// Returns the path to the oxy binary for integration tests.
///
/// Resolution order:
/// 1. `OXY_BIN` env var (set by CI or user)
/// 2. `target/llvm-cov-target/debug/oxy` (coverage builds)
/// 3. `target/ci/oxy` (CI profile)
/// 4. `target/debug/oxy` (regular debug)
pub fn get_oxy_binary() -> PathBuf {
    if let Ok(bin) = std::env::var("OXY_BIN") {
        return PathBuf::from(bin);
    }

    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    for subdir in [
        "target/llvm-cov-target/debug/oxy",
        "target/ci/oxy",
        "target/debug/oxy",
    ] {
        let p = workspace_dir.join(subdir);
        if p.exists() {
            return p;
        }
    }

    workspace_dir.join("target/debug/oxy")
}

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
