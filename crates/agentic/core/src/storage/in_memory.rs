//! In-memory storage adapter.
//!
//! Used in unit/integration tests and when `--no-persist` is passed.
//! No disk I/O, no setup, no runtime constraints.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use crate::app_storage::{
    truncate_artifact_content, PersistedTurn, PreferenceStore, QueryLog, QueryLogEntry,
    SessionSummary, StorageError, SuspendedPipeline, SuspendedPipelineStore, TurnStore,
};

// ── Internal session row ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SessionRow {
    id: i64,
    created_at: String,
    turn_count: u32,
    last_question: Option<String>,
    data_dir: Option<String>,
}

// ── InMemoryStorage ───────────────────────────────────────────────────────────

/// In-memory storage adapter.  Implements all four storage traits.
pub struct InMemoryStorage {
    sessions: Mutex<Vec<SessionRow>>,
    turns: Mutex<Vec<PersistedTurn>>,
    queries: Mutex<Vec<QueryLogEntry>>,
    prefs: Mutex<HashMap<String, String>>,
    suspended: Mutex<HashMap<i64, SuspendedPipeline>>,
    next_id: AtomicI64,
}

impl InMemoryStorage {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(Vec::new()),
            turns: Mutex::new(Vec::new()),
            queries: Mutex::new(Vec::new()),
            prefs: Mutex::new(HashMap::new()),
            suspended: Mutex::new(HashMap::new()),
            next_id: AtomicI64::new(1),
        }
    }

    fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

// ── TurnStore ─────────────────────────────────────────────────────────────────

#[async_trait]
impl TurnStore for InMemoryStorage {
    async fn create_session(&self, data_dir: Option<&str>) -> Result<i64, StorageError> {
        let id = self.next_id();
        self.sessions.lock().unwrap().push(SessionRow {
            id,
            created_at: now_iso8601(),
            turn_count: 0,
            last_question: None,
            data_dir: data_dir.map(str::to_string),
        });
        Ok(id)
    }

    async fn list_sessions(&self, limit: u32) -> Result<Vec<SessionSummary>, StorageError> {
        let sessions = self.sessions.lock().unwrap();
        let mut rows: Vec<&SessionRow> = sessions.iter().collect();
        rows.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(rows
            .into_iter()
            .take(limit as usize)
            .map(|r| SessionSummary {
                id: r.id,
                created_at: r.created_at.clone(),
                turn_count: r.turn_count,
                last_question: r.last_question.clone(),
                data_dir: r.data_dir.clone(),
            })
            .collect())
    }

    async fn save_turn(&self, turn: &PersistedTurn) -> Result<i64, StorageError> {
        let mut turn = turn.clone();
        for artifact in &mut turn.artifacts {
            artifact.content = truncate_artifact_content(&artifact.content);
        }
        let id = self.next_id();
        turn.id = id;

        // Update session metadata.
        {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(row) = sessions.iter_mut().find(|r| r.id == turn.session_id) {
                row.turn_count += 1;
                row.last_question = Some(turn.question.clone());
            }
        }

        self.turns.lock().unwrap().push(turn);
        Ok(id)
    }

    async fn load_turns(&self, session_id: i64) -> Result<Vec<PersistedTurn>, StorageError> {
        let turns = self.turns.lock().unwrap();
        let mut result: Vec<PersistedTurn> = turns
            .iter()
            .filter(|t| t.session_id == session_id)
            .cloned()
            .collect();
        result.sort_by_key(|t| t.turn_index);
        Ok(result)
    }
}

// ── QueryLog ──────────────────────────────────────────────────────────────────

#[async_trait]
impl QueryLog for InMemoryStorage {
    async fn log_query(&self, entry: &QueryLogEntry) -> Result<(), StorageError> {
        self.queries.lock().unwrap().push(entry.clone());
        Ok(())
    }
}

// ── PreferenceStore ───────────────────────────────────────────────────────────

#[async_trait]
impl PreferenceStore for InMemoryStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self.prefs.lock().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.prefs
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }
}

// ── SuspendedPipelineStore ────────────────────────────────────────────────────

#[async_trait]
impl SuspendedPipelineStore for InMemoryStorage {
    async fn save_suspended(&self, sp: &SuspendedPipeline) -> Result<(), StorageError> {
        self.suspended
            .lock()
            .unwrap()
            .insert(sp.session_id, sp.clone());
        Ok(())
    }

    async fn take_suspended(
        &self,
        session_id: i64,
    ) -> Result<Option<SuspendedPipeline>, StorageError> {
        Ok(self.suspended.lock().unwrap().remove(&session_id))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs;
    let (sec, min, hour, day, mon, year) = {
        let s2 = s % 60;
        let m = (s / 60) % 60;
        let h = (s / 3600) % 24;
        let days = s / 86400;
        let year = 1970 + days / 365;
        let leap = |y: u64| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
        let days_in_year: u64 = (1970..year).map(|y| if leap(y) { 366 } else { 365 }).sum();
        let mut d = days - days_in_year;
        let months = [
            31u64,
            if leap(year) { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        let mut mon = 1u64;
        for &dim in &months {
            if d < dim {
                break;
            }
            d -= dim;
            mon += 1;
        }
        (s2, m, h, d + 1, mon, year)
    };
    format!("{year:04}-{mon:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_storage::Artifact;
    use crate::app_storage::StorageHandle;

    #[tokio::test]
    async fn session_lifecycle() {
        let store = InMemoryStorage::new();
        let id = store.create_session(Some("/data")).await.unwrap();
        let sessions = store.list_sessions(10).await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
        assert_eq!(sessions[0].data_dir.as_deref(), Some("/data"));
    }

    #[tokio::test]
    async fn save_and_load_turns() {
        let store = InMemoryStorage::new();
        let sid = store.create_session(None).await.unwrap();
        let turn = PersistedTurn {
            id: 0,
            session_id: sid,
            turn_index: 0,
            trace_id: "t1".into(),
            question: "q".into(),
            answer: "a".into(),
            artifacts: vec![Artifact {
                kind: "sql".into(),
                content: "SELECT 1".into(),
            }],
            created_at: "2026-01-01T00:00:00Z".into(),
            duration_ms: Some(100),
        };
        store.save_turn(&turn).await.unwrap();
        let turns = store.load_turns(sid).await.unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].question, "q");

        let sessions = store.list_sessions(10).await.unwrap();
        assert_eq!(sessions[0].turn_count, 1);
    }

    #[tokio::test]
    async fn artifact_truncation() {
        let store = InMemoryStorage::new();
        let sid = store.create_session(None).await.unwrap();
        let big = "x".repeat(70_000);
        let turn = PersistedTurn {
            id: 0,
            session_id: sid,
            turn_index: 0,
            trace_id: "t".into(),
            question: "q".into(),
            answer: "a".into(),
            artifacts: vec![Artifact {
                kind: "sql".into(),
                content: big,
            }],
            created_at: "2026-01-01T00:00:00Z".into(),
            duration_ms: None,
        };
        store.save_turn(&turn).await.unwrap();
        let turns = store.load_turns(sid).await.unwrap();
        assert!(turns[0].artifacts[0].content.ends_with("…[truncated]"));
    }

    #[tokio::test]
    async fn preferences() {
        let store = InMemoryStorage::new();
        assert_eq!(store.get("thinking").await.unwrap(), None);
        store.set("thinking", "adaptive").await.unwrap();
        assert_eq!(
            store.get("thinking").await.unwrap(),
            Some("adaptive".into())
        );
    }

    #[tokio::test]
    async fn query_log() {
        let store = InMemoryStorage::new();
        let entry = QueryLogEntry {
            session_id: 1,
            turn_index: Some(0),
            query: "SELECT 1".into(),
            success: true,
            row_count: Some(1),
            duration_ms: Some(5),
            error: None,
        };
        store.log_query(&entry).await.unwrap();
        assert_eq!(store.queries.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn storage_handle_wraps_adapter() {
        let handle = StorageHandle::from_adapter(InMemoryStorage::new());
        let id = handle.turns.create_session(None).await.unwrap();
        assert!(id > 0);
        handle.prefs.set("k", "v").await.unwrap();
        assert_eq!(handle.prefs.get("k").await.unwrap(), Some("v".into()));
    }
}
