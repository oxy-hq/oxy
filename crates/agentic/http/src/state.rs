use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentic_builder::BuilderTestRunner;
use dashmap::DashMap;
use tokio::sync::{Notify, mpsc, watch};

/// Shared state for all agentic routes.
///
/// Mount this on the analytics sub-router via `.with_state(Arc::new(AgenticState::new()))`.
/// It holds no database connection — routes obtain one via `establish_connection()` per request.
pub struct AgenticState {
    /// Woken when new events are written to `agentic_run_events` for a run.
    /// SSE handlers park on this instead of polling.
    pub notifiers: DashMap<String, Arc<Notify>>,

    /// Delivers user answers to the suspended orchestrator task.
    /// Keyed by run_id; present only while the run is active.
    pub answer_txs: DashMap<String, mpsc::Sender<String>>,

    /// Cancellation signal for running pipeline tasks.
    /// Keyed by run_id; present only while the run is active.
    pub cancel_txs: DashMap<String, watch::Sender<bool>>,

    /// In-memory run status — avoids a DB round-trip in the answer route.
    pub statuses: DashMap<String, RunStatus>,
    /// Cached schema introspection results, keyed by connector name.
    ///
    /// Shared with [`BuildContext`] so that `build_solver_with_context` can
    /// skip re-introspecting a database whose schema hasn't changed.
    pub schema_cache: Arc<Mutex<HashMap<String, agentic_analytics::SchemaCatalog>>>,

    /// Optional test runner injected by `oxy-app` so the builder copilot can
    /// execute `.test.yml` files via the eval pipeline.
    pub builder_test_runner: Option<Arc<dyn BuilderTestRunner>>,
}

impl Default for AgenticState {
    fn default() -> Self {
        Self::new()
    }
}

impl AgenticState {
    pub fn new() -> Self {
        Self {
            notifiers: DashMap::new(),
            answer_txs: DashMap::new(),
            cancel_txs: DashMap::new(),
            statuses: DashMap::new(),
            schema_cache: Arc::new(Mutex::new(HashMap::new())),
            builder_test_runner: None,
        }
    }

    /// Attach a test runner to this state.  Call before mounting routes.
    pub fn with_builder_test_runner(mut self, runner: Arc<dyn BuilderTestRunner>) -> Self {
        self.builder_test_runner = Some(runner);
        self
    }

    /// Register a new active run; called from `create_run` before spawning the task.
    pub fn register(
        &self,
        run_id: &str,
        answer_tx: mpsc::Sender<String>,
        cancel_tx: watch::Sender<bool>,
    ) {
        self.notifiers
            .insert(run_id.to_string(), Arc::new(Notify::new()));
        self.answer_txs.insert(run_id.to_string(), answer_tx);
        self.cancel_txs.insert(run_id.to_string(), cancel_tx);
        self.statuses.insert(run_id.to_string(), RunStatus::Running);
    }

    /// Signal a running pipeline task to cancel; returns false if the run is not active.
    pub fn cancel(&self, run_id: &str) -> bool {
        if let Some(tx) = self.cancel_txs.get(run_id) {
            tx.send(true).is_ok()
        } else {
            false
        }
    }

    /// Remove all in-memory state for a completed (done/failed) run.
    pub fn deregister(&self, run_id: &str) {
        self.notifiers.remove(run_id);
        self.answer_txs.remove(run_id);
        self.cancel_txs.remove(run_id);
        // Leave status so late-connecting SSE subscribers can read it.
    }

    /// Wake SSE subscribers waiting on new events for this run.
    ///
    /// Uses `notify_one()` (not `notify_waiters()`) so the permit is stored
    /// when no waiter is currently parked.  With `notify_waiters()` a
    /// notification that fires between the DB query and the `.await` in the
    /// SSE loop would be silently dropped, causing the stream to stall after
    /// the orchestrator resumes from suspension.
    pub fn notify(&self, run_id: &str) {
        if let Some(n) = self.notifiers.get(run_id) {
            n.notify_one();
        }
    }
}

/// Current state of a pipeline run (in-memory cache).
#[derive(Debug, Clone)]
pub enum RunStatus {
    Running,
    /// The LLM called `ask_user`; the questions are presented to the user.
    Suspended {
        questions: Vec<agentic_core::HumanInputQuestion>,
    },
    Done,
    Failed(String),
}

impl RunStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunStatus::Done | RunStatus::Failed(_))
    }
}
