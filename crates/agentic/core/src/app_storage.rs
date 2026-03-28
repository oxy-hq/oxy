//! Application storage ports (traits) and domain-agnostic models.
//!
//! Enabled via the `storage` feature flag.  The CLI layer wires a concrete
//! adapter at startup; everything else depends only on [`StorageHandle`].

use async_trait::async_trait;
use std::sync::Arc;

use crate::human_input::SuspendedRunData;

// ── Error ──────────────────────────────────────────────────────────────────────

/// Errors returned by storage operations.
#[derive(Debug)]
pub enum StorageError {
    /// OS-level or backend-level failure (disk full, locked, corrupt, etc.).
    Io(String),
    /// A specific record was not found.
    NotFound(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Io(msg) => write!(f, "storage I/O error: {msg}"),
            StorageError::NotFound(msg) => write!(f, "not found: {msg}"),
        }
    }
}

impl std::error::Error for StorageError {}

// ── Models ─────────────────────────────────────────────────────────────────────

/// A domain-agnostic artifact produced during a pipeline run.
///
/// `kind` is open-ended and domain-defined: `"sql"`, `"python"`, `"chart_spec"`, …
/// `content` is truncated at 64 KB by the adapter; a `…[truncated]` suffix is
/// appended when truncation occurs. Never deserialized back into typed state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Artifact {
    pub kind: String,
    pub content: String,
}

/// A single persisted turn — only the fields needed for prompt hydration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedTurn {
    pub id: i64,
    pub session_id: i64,
    pub turn_index: u32,
    pub trace_id: String,
    pub question: String,
    pub answer: String,
    /// All artifacts produced during this run.
    pub artifacts: Vec<Artifact>,
    pub created_at: String,
    pub duration_ms: Option<u64>,
}

/// Summary of a persisted session (no full turn data).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSummary {
    pub id: i64,
    pub created_at: String,
    pub turn_count: u32,
    pub last_question: Option<String>,
    pub data_dir: Option<String>,
}

/// A query log entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryLogEntry {
    pub session_id: i64,
    pub turn_index: Option<u32>,
    /// Domain-agnostic query string (SQL, Python, shell command, etc.).
    pub query: String,
    pub success: bool,
    pub row_count: Option<u64>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// A persisted suspended pipeline — stored when the orchestrator returns
/// [`OrchestratorError::Suspended`] and retrieved on the user's next turn to
/// call [`Orchestrator::resume`].
///
/// [`OrchestratorError::Suspended`]: crate::orchestrator::OrchestratorError::Suspended
/// [`Orchestrator::resume`]: crate::orchestrator::Orchestrator::resume
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SuspendedPipeline {
    pub session_id: i64,
    /// The question the LLM asked the user.
    pub prompt: String,
    /// Optional LLM-generated answer suggestions.
    pub suggestions: Vec<String>,
    /// Minimal payload needed to call `Orchestrator::resume`.
    pub resume_data: SuspendedRunData,
    pub created_at: String,
}

// ── Ports (traits) ─────────────────────────────────────────────────────────────

/// Storage port for conversation turns and sessions.
#[async_trait]
pub trait TurnStore: Send + Sync {
    /// Create a new session and return its ID.
    async fn create_session(&self, data_dir: Option<&str>) -> Result<i64, StorageError>;
    /// List sessions, most recent first.
    async fn list_sessions(&self, limit: u32) -> Result<Vec<SessionSummary>, StorageError>;
    /// Persist a turn and return its ID.
    async fn save_turn(&self, turn: &PersistedTurn) -> Result<i64, StorageError>;
    /// Load all turns for a session, ordered by `turn_index`.
    async fn load_turns(&self, session_id: i64) -> Result<Vec<PersistedTurn>, StorageError>;
}

/// Storage port for the query audit log.
#[async_trait]
pub trait QueryLog: Send + Sync {
    /// Append a query log entry.
    async fn log_query(&self, entry: &QueryLogEntry) -> Result<(), StorageError>;
}

/// Storage port for suspended pipelines awaiting user input.
#[async_trait]
pub trait SuspendedPipelineStore: Send + Sync {
    /// Persist a suspended pipeline for the given session.
    ///
    /// Any existing suspended pipeline for the same session is replaced.
    async fn save_suspended(&self, sp: &SuspendedPipeline) -> Result<(), StorageError>;

    /// Atomically load and remove the suspended pipeline for the given session.
    ///
    /// Returns `None` if there is no pending suspension for this session.
    async fn take_suspended(
        &self,
        session_id: i64,
    ) -> Result<Option<SuspendedPipeline>, StorageError>;
}

/// Storage port for user preferences.
#[async_trait]
pub trait PreferenceStore: Send + Sync {
    /// Get a preference value by key.
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError>;
    /// Set a preference value.
    async fn set(&self, key: &str, value: &str) -> Result<(), StorageError>;
}

// ── StorageHandle ──────────────────────────────────────────────────────────────

/// Bundle of storage trait-object arcs.  Cheap to clone (Arc ref-count bumps).
///
/// This is the only type the CLI passes around — no concrete adapter name
/// leaks past the wiring point in `main.rs`.
#[derive(Clone)]
pub struct StorageHandle {
    pub turns: Arc<dyn TurnStore>,
    pub queries: Arc<dyn QueryLog>,
    pub prefs: Arc<dyn PreferenceStore>,
    pub suspended: Arc<dyn SuspendedPipelineStore>,
}

impl StorageHandle {
    /// Wrap any adapter that implements all four traits.
    pub fn from_adapter<A>(adapter: A) -> Self
    where
        A: TurnStore + QueryLog + PreferenceStore + SuspendedPipelineStore + 'static,
    {
        let arc = Arc::new(adapter);
        Self {
            turns: arc.clone(),
            queries: arc.clone(),
            prefs: arc.clone(),
            suspended: arc,
        }
    }
}

// ── Shared helper ──────────────────────────────────────────────────────────────

/// Truncate artifact content at 64 KB, appending a `…[truncated]` suffix.
pub fn truncate_artifact_content(content: &str) -> String {
    const LIMIT: usize = 65_536;
    if content.len() <= LIMIT {
        content.to_string()
    } else {
        // Find a valid UTF-8 boundary at or before LIMIT.
        let mut end = LIMIT;
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…[truncated]", &content[..end])
    }
}
