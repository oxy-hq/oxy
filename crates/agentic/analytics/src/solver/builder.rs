use std::collections::HashMap;
use std::sync::Arc;

use agentic_connector::DatabaseConnector;

use crate::engine::SemanticEngine;
use crate::llm::LlmClient;
use crate::semantic::SemanticCatalog;

use super::AnalyticsSolver;

// ---------------------------------------------------------------------------
// AnalyticsSolverBuilder
// ---------------------------------------------------------------------------

/// Fluent builder for [`AnalyticsSolver`].
///
/// # Single-connector (common case)
///
/// ```ignore
/// let solver = AnalyticsSolverBuilder::new()
///     .connector(my_connector)
///     .llm(my_client)
///     .catalog(my_catalog)
///     .build();
/// ```
///
/// # Multi-connector
///
/// ```ignore
/// let solver = AnalyticsSolverBuilder::new()
///     .add_connector("warehouse", sqlite_connector)
///     .add_connector_with_dialect("events", duckdb_connector, "DuckDB")
///     .default_connector("warehouse")
///     .llm(my_client)
///     .catalog(my_catalog)
///     .build();
/// ```
#[derive(Default)]
pub struct AnalyticsSolverBuilder {
    connectors: HashMap<String, Arc<dyn DatabaseConnector>>,
    default_connector: Option<String>,
    client: Option<LlmClient>,
    catalog: Option<SemanticCatalog>,
    engine: Option<Arc<dyn SemanticEngine>>,
}

impl AnalyticsSolverBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a named connector.  The first connector added becomes the default
    /// unless [`Self::default_connector`] is called explicitly.
    pub fn add_connector(
        mut self,
        name: impl Into<String>,
        c: impl DatabaseConnector + 'static,
    ) -> Self {
        let name = name.into();
        if self.default_connector.is_none() {
            self.default_connector = Some(name.clone());
        }
        self.connectors.insert(name, Arc::new(c));
        self
    }

    /// Override which connector name is used as the default.
    pub fn default_connector(mut self, name: impl Into<String>) -> Self {
        self.default_connector = Some(name.into());
        self
    }

    /// Convenience: add a single connector under the name `"default"`.
    pub fn connector(self, c: impl DatabaseConnector + 'static) -> Self {
        self.add_connector("default", c)
    }

    pub fn llm(mut self, client: LlmClient) -> Self {
        self.client = Some(client);
        self
    }

    pub fn catalog(mut self, catalog: SemanticCatalog) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Attach a vendor semantic engine (e.g. `CubeEngine`, `LookerEngine`).
    ///
    /// When set, the Specifying handler attempts vendor translation before
    /// falling back to the internal compiler or LLM.
    pub fn engine(mut self, e: impl SemanticEngine + 'static) -> Self {
        self.engine = Some(Arc::new(e));
        self
    }

    /// Attach a pre-boxed vendor engine.
    pub fn engine_arc(mut self, e: Arc<dyn SemanticEngine>) -> Self {
        self.engine = Some(e);
        self
    }

    /// Build the solver.  Panics if no connector or LLM client was provided.
    pub fn build(self) -> AnalyticsSolver {
        let client = self
            .client
            .expect("AnalyticsSolverBuilder: LLM client is required");
        let catalog = self.catalog.unwrap_or_else(SemanticCatalog::empty);
        let default_connector = self
            .default_connector
            .unwrap_or_else(|| "default".to_string());
        let mut solver =
            AnalyticsSolver::new_multi(client, catalog, self.connectors, default_connector);
        if let Some(engine) = self.engine {
            solver = solver.with_engine(engine);
        }
        solver
    }
}
