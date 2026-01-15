// MCP Tool Execution Context
//
// This module provides a unified context for tool execution, bundling
// all the runtime parameters needed to execute any tool type.

use std::collections::HashMap;

use serde_json::Value;

use oxy::{adapters::session_filters::SessionFilters, config::model::ConnectionOverrides};

/// Context for tool execution containing all runtime parameters.
///
/// This struct bundles together all the parameters that are commonly
/// passed to tool execution functions, providing a cleaner API and
/// making it easier to add new context parameters in the future.
#[derive(Debug, Clone, Default)]
pub struct ToolExecutionContext {
    /// Session filters for row-level security and data filtering
    pub session_filters: Option<SessionFilters>,

    /// Connection overrides for runtime database configuration
    pub connection_overrides: Option<ConnectionOverrides>,

    /// Variables from the _meta parameter (highest precedence)
    pub meta_variables: HashMap<String, Value>,
}

impl ToolExecutionContext {
    /// Creates a new empty ToolExecutionContext
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to set session filters
    pub fn with_session_filters(mut self, filters: Option<SessionFilters>) -> Self {
        self.session_filters = filters;
        self
    }

    /// Builder method to set connection overrides
    pub fn with_connection_overrides(mut self, overrides: Option<ConnectionOverrides>) -> Self {
        self.connection_overrides = overrides;
        self
    }

    /// Builder method to set meta variables
    pub fn with_meta_variables(mut self, variables: HashMap<String, Value>) -> Self {
        self.meta_variables = variables;
        self
    }
}
