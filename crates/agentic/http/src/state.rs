use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use agentic_pipeline::platform::ThreadOwnerLookup;
use agentic_pipeline::{AnalyticsSchemaCatalog, BuilderTestRunnerTrait};
use agentic_runtime::event_registry::EventRegistry;
use sea_orm::DatabaseConnection;
use tokio_util::sync::CancellationToken;

pub use agentic_runtime::state::{RunError, RunStatus, RuntimeState};

/// Shared state for all agentic routes.
///
/// Wraps the transport-agnostic [`RuntimeState`] and adds domain-specific
/// extensions (schema cache, builder test runner, event registry). Holds the
/// shared SeaORM [`DatabaseConnection`] so handlers don't open a new one per
/// request, and a [`ThreadOwnerLookup`] so thread-ownership auth checks do
/// not reach into the platform `threads` table from this crate.
pub struct AgenticState {
    pub runtime: Arc<RuntimeState>,
    pub schema_cache: Arc<Mutex<HashMap<String, AnalyticsSchemaCatalog>>>,
    pub builder_test_runner: Option<Arc<dyn BuilderTestRunnerTrait>>,
    pub event_registry: Arc<EventRegistry>,
    pub shutdown_token: CancellationToken,
    pub db: DatabaseConnection,
    pub thread_owner: Arc<dyn ThreadOwnerLookup>,
}

impl AgenticState {
    pub fn new(
        shutdown_token: CancellationToken,
        db: DatabaseConnection,
        thread_owner: Arc<dyn ThreadOwnerLookup>,
    ) -> Self {
        Self {
            runtime: Arc::new(RuntimeState::new()),
            schema_cache: Arc::new(Mutex::new(HashMap::new())),
            builder_test_runner: None,
            event_registry: Arc::new(agentic_pipeline::build_event_registry()),
            shutdown_token,
            db,
            thread_owner,
        }
    }

    pub fn with_builder_test_runner(mut self, runner: Arc<dyn BuilderTestRunnerTrait>) -> Self {
        self.builder_test_runner = Some(runner);
        self
    }
}

impl Deref for AgenticState {
    type Target = RuntimeState;

    fn deref(&self) -> &RuntimeState {
        &self.runtime
    }
}
