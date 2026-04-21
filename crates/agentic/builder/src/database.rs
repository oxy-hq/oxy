//! Database provider abstraction for the builder domain.
//!
//! Replaces direct `oxy::config::ConfigBuilder` + `oxy::connector::Connector`
//! usage with a trait that the pipeline layer can implement.

use std::sync::Arc;

use agentic_connector::DatabaseConnector;
use agentic_core::tools::ToolError;
use async_trait::async_trait;

/// Provides database listing and connector construction for builder tools.
///
/// The builder domain uses this trait instead of depending on oxy config/connector
/// directly, keeping a clean domain boundary. The pipeline layer supplies the
/// implementation that bridges to oxy's `ConfigManager` and `SecretsManager`.
#[async_trait]
pub trait BuilderDatabaseProvider: Send + Sync {
    /// List available database names (from the project's config.yml).
    async fn list_databases(&self) -> Result<Vec<String>, ToolError>;

    /// Get a connector for the named database.
    ///
    /// The implementation handles config loading, secret resolution, and
    /// connector construction.
    async fn get_connector(
        &self,
        database_name: &str,
    ) -> Result<Arc<dyn DatabaseConnector>, ToolError>;
}
