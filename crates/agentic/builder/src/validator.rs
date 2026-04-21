//! Project validation abstraction for the builder domain.
//!
//! Replaces direct use of `oxy::config::ConfigBuilder`, `oxy_globals`, and
//! `oxy_semantic` for project file validation.

use std::path::Path;

use agentic_core::tools::ToolError;
use async_trait::async_trait;
use serde::Serialize;

/// Result of validating a single project file.
#[derive(Debug, Serialize)]
pub struct ValidatedFile {
    pub relative_path: String,
    pub error: Option<String>,
}

/// Summary of a full project validation run.
#[derive(Debug, Serialize)]
pub struct ValidationReport {
    pub valid_count: usize,
    pub errors: Vec<ValidatedFile>,
}

/// Validates project configuration files against the Oxy schema.
///
/// The builder domain uses this trait instead of depending on oxy config/semantic
/// crates directly. The pipeline layer supplies the implementation that bridges
/// to oxy's `ConfigManager`, `GlobalRegistry`, and `SemanticLayerParser`.
#[async_trait]
pub trait BuilderProjectValidator: Send + Sync {
    /// Validate a single file by its absolute path.
    /// Returns `Ok(())` on success, `Err(message)` on validation failure.
    async fn validate_file(&self, abs_path: &Path) -> Result<(), String>;

    /// Validate all project files (agents, workflows, apps, semantic).
    async fn validate_all(&self) -> Result<ValidationReport, ToolError>;
}
