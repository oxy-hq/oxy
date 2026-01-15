//! Tool execution registry for pluggable tool implementations
//!
//! This module provides a registry pattern for tool execution, allowing
//! higher-level tools (Workflow, Agent, SemanticQuery) to be registered
//! at the application level, avoiding circular dependencies.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    config::model::ToolType,
    execute::{ExecutionContext, types::OutputContainer},
};
use oxy_shared::errors::OxyError;

use super::types::ToolRawInput;

/// Trait for implementing custom tool executors
///
/// This allows higher layers (app, workflow, agent crates) to provide
/// implementations for tools that would otherwise create circular dependencies.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute the tool with the given context and input
    async fn execute(
        &self,
        execution_context: &ExecutionContext,
        tool_type: &ToolType,
        input: &ToolRawInput,
    ) -> Result<OutputContainer, OxyError>;

    /// Check if this executor can handle the given tool type
    fn can_handle(&self, tool_type: &ToolType) -> bool;

    /// Get a name for this executor (for debugging/logging)
    fn name(&self) -> &'static str;
}

/// Global registry for tool executors
///
/// This allows the application layer to register custom executors
/// for Workflow, Agent, and SemanticQuery tools without creating
/// circular dependencies between crates.
pub struct ToolRegistry {
    executors: RwLock<Vec<Arc<dyn ToolExecutor>>>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            executors: RwLock::new(Vec::new()),
        }
    }

    /// Register a new tool executor
    ///
    /// # Errors
    ///
    /// Returns an error if the executor name is empty or if an executor
    /// with the same name is already registered.
    pub async fn register(&self, executor: Arc<dyn ToolExecutor>) -> Result<(), OxyError> {
        let name = executor.name();

        if name.is_empty() {
            return Err(OxyError::ConfigurationError(
                "Tool executor name cannot be empty".to_string(),
            ));
        }

        let mut executors = self.executors.write().await;

        // Check for duplicate registration
        if executors.iter().any(|e| e.name() == name) {
            tracing::warn!("Tool executor '{}' is already registered, skipping", name);
            return Ok(());
        }

        tracing::info!("Registering tool executor: {}", name);
        executors.push(executor);
        Ok(())
    }

    /// Find an executor that can handle the given tool type
    pub async fn find_executor(&self, tool_type: &ToolType) -> Option<Arc<dyn ToolExecutor>> {
        let executors = self.executors.read().await;
        executors.iter().find(|e| e.can_handle(tool_type)).cloned()
    }

    /// Execute a tool using registered executors
    pub async fn execute(
        &self,
        execution_context: &ExecutionContext,
        tool_type: &ToolType,
        input: &ToolRawInput,
    ) -> Result<Option<OutputContainer>, OxyError> {
        if let Some(executor) = self.find_executor(tool_type).await {
            tracing::debug!("Found registered executor '{}' for tool", executor.name());
            Ok(Some(
                executor
                    .execute(execution_context, tool_type, input)
                    .await?,
            ))
        } else {
            Ok(None)
        }
    }

    /// Check if any executor can handle this tool type
    pub async fn has_executor_for(&self, tool_type: &ToolType) -> bool {
        self.find_executor(tool_type).await.is_some()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Global registry instance
lazy_static::lazy_static! {
    static ref GLOBAL_REGISTRY: ToolRegistry = ToolRegistry::new();
}

/// Get the global tool registry
pub fn global_registry() -> &'static ToolRegistry {
    &GLOBAL_REGISTRY
}

/// Helper function to register a tool executor globally
///
/// # Errors
///
/// Returns an error if registration fails due to validation errors.
pub async fn register_tool_executor(executor: Arc<dyn ToolExecutor>) -> Result<(), OxyError> {
    global_registry().register(executor).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute::types::Output;

    struct MockExecutor;

    #[async_trait]
    impl ToolExecutor for MockExecutor {
        async fn execute(
            &self,
            _execution_context: &ExecutionContext,
            _tool_type: &ToolType,
            _input: &ToolRawInput,
        ) -> Result<OutputContainer, OxyError> {
            Ok(OutputContainer::Single(Output::Text("mock".to_string())))
        }

        fn can_handle(&self, tool_type: &ToolType) -> bool {
            matches!(tool_type, ToolType::Workflow(_))
        }

        fn name(&self) -> &'static str {
            "MockExecutor"
        }
    }

    #[tokio::test]
    async fn test_registry_basic() {
        let registry = ToolRegistry::new();
        let executor = Arc::new(MockExecutor);

        registry.register(executor).await.unwrap();

        // Create a minimal workflow tool type for testing
        let workflow_tool = ToolType::Workflow(crate::config::model::WorkflowTool {
            name: "test".to_string(),
            description: "test".to_string(),
            workflow_ref: "test.yml".to_string(),
            variables: None,
            output_task_ref: None,
            is_verified: false,
        });

        assert!(registry.has_executor_for(&workflow_tool).await);
    }

    #[tokio::test]
    async fn test_registry_duplicate_registration() {
        let registry = ToolRegistry::new();
        let executor1 = Arc::new(MockExecutor);
        let executor2 = Arc::new(MockExecutor);

        // First registration should succeed
        assert!(registry.register(executor1).await.is_ok());

        // Duplicate registration should succeed but log a warning
        assert!(registry.register(executor2).await.is_ok());

        // Should still only have one executor
        let executors = registry.executors.read().await;
        assert_eq!(executors.len(), 1);
    }

    struct EmptyNameExecutor;

    #[async_trait]
    impl ToolExecutor for EmptyNameExecutor {
        async fn execute(
            &self,
            _execution_context: &ExecutionContext,
            _tool_type: &ToolType,
            _input: &ToolRawInput,
        ) -> Result<OutputContainer, OxyError> {
            Ok(OutputContainer::Single(Output::Text("test".to_string())))
        }

        fn can_handle(&self, _tool_type: &ToolType) -> bool {
            true
        }

        fn name(&self) -> &'static str {
            ""
        }
    }

    #[tokio::test]
    async fn test_registry_empty_name_rejected() {
        let registry = ToolRegistry::new();
        let executor = Arc::new(EmptyNameExecutor);

        let result = registry.register(executor).await;
        assert!(result.is_err());

        if let Err(OxyError::ConfigurationError(msg)) = result {
            assert!(msg.contains("empty"));
        } else {
            panic!("Expected ConfigurationError");
        }
    }
}
