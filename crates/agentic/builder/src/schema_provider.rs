//! Schema provider abstraction for the builder domain.
//!
//! Replaces direct use of `schemars::schema_for!()` on oxy config/semantic
//! types. The pipeline layer supplies an implementation that owns the oxy types.

/// Provides JSON schema definitions for Oxy object types.
///
/// The builder's `lookup_schema` tool delegates to this trait instead of
/// importing oxy config model types directly.
pub trait BuilderSchemaProvider: Send + Sync {
    /// Get the JSON schema for a named object type (e.g. "AgentConfig", "View").
    /// Returns `None` if the type is not recognized.
    fn get_schema(&self, object_name: &str) -> Option<serde_json::Value>;

    /// List all supported type names.
    fn supported_types(&self) -> &[&str];
}

/// A no-op provider that reports no supported types.
pub(crate) struct EmptySchemaProvider;

impl BuilderSchemaProvider for EmptySchemaProvider {
    fn get_schema(&self, _object_name: &str) -> Option<serde_json::Value> {
        None
    }

    fn supported_types(&self) -> &[&str] {
        &[]
    }
}
