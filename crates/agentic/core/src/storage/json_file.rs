//! JSON file-based storage adapter.
//!
//! Layout under `base_dir` (default `~/.agentic/`):
//!
//! ```text
//! sessions.json          — Vec<SessionRow>  (rewritten on create_session)
//! turns/<id>.json        — Vec<PersistedTurn> (rewritten on save_turn)
//! query_log.jsonl        — append-only NDJSON
//! prefs.json             — HashMap<String, String> (rewritten on set)
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::app_storage::{
    PersistedTurn, PreferenceStore, QueryLog, QueryLogEntry, SessionSummary, StorageError,
    SuspendedPipeline, SuspendedPipelineStore, TurnStore, truncate_artifact_content,
};

// ── Internal session row ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionRow {
    id: i64,
    created_at: String,
    turn_count: u32,
    last_question: Option<String>,
    data_dir: Option<String>,
}

impl From<&SessionRow> for SessionSummary {
    fn from(r: &SessionRow) -> Self {
        SessionSummary {
            id: r.id,
            created_at: r.created_at.clone(),
            turn_count: r.turn_count,
            last_question: r.last_question.clone(),
            data_dir: r.data_dir.clone(),
        }
    }
}

// ── JsonFileStorage ───────────────────────────────────────────────────────────

/// JSON file-backed storage adapter.
///
/// All trait methods are async and use `tokio::fs`.  `open` creates the
/// directory structure on first use.
pub struct JsonFileStorage {
    base_dir: PathBuf,
    /// In-process next-ID counter (incremented on every `create_session` /
    /// `save_turn`).  Seeded from the max existing ID at open time.
    next_id: AtomicI64,
}

impl JsonFileStorage {
    /// Open (or create) the storage directory and return a ready adapter.
    pub async fn open(base_dir: PathBuf) -> Result<Self, StorageError> {
        tokio::fs::create_dir_all(&base_dir).await.map_err(io)?;
        tokio::fs::create_dir_all(base_dir.join("turns"))
            .await
            .map_err(io)?;
        tokio::fs::create_dir_all(base_dir.join("suspended"))
            .await
            .map_err(io)?;

        // Seed the ID counter from existing sessions so IDs are always
        // monotonically increasing across restarts.
        let next_id = Self::load_max_session_id(&base_dir).await.unwrap_or(0) + 1;

        Ok(Self {
            base_dir,
            next_id: AtomicI64::new(next_id),
        })
    }

    // ── Helpers ────────────────────────────────────────────────────────────────

    fn sessions_path(&self) -> PathBuf {
        self.base_dir.join("sessions.json")
    }

    fn turns_path(&self, session_id: i64) -> PathBuf {
        self.base_dir
            .join("turns")
            .join(format!("{session_id}.json"))
    }

    fn query_log_path(&self) -> PathBuf {
        self.base_dir.join("query_log.jsonl")
    }

    fn suspended_path(&self, session_id: i64) -> PathBuf {
        self.base_dir
            .join("suspended")
            .join(format!("{session_id}.json"))
    }

    fn prefs_path(&self) -> PathBuf {
        self.base_dir.join("prefs.json")
    }

    async fn load_max_session_id(base_dir: &PathBuf) -> Option<i64> {
        let path = base_dir.join("sessions.json");
        let bytes = tokio::fs::read(&path).await.ok()?;
        let rows: Vec<SessionRow> = serde_json::from_slice(&bytes).ok()?;
        rows.iter().map(|r| r.id).max()
    }

    /// Read a JSON file, returning the default value if missing.
    async fn read_json<T: serde::de::DeserializeOwned + Default>(
        path: &PathBuf,
    ) -> Result<T, StorageError> {
        match tokio::fs::read(path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| io(e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(io(e)),
        }
    }

    /// Write a JSON file atomically (write to `.tmp`, then rename).
    async fn write_json_atomic<T: serde::Serialize>(
        path: &PathBuf,
        value: &T,
    ) -> Result<(), StorageError> {
        let tmp = path.with_extension("tmp");
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| io(e))?;
        tokio::fs::write(&tmp, &bytes).await.map_err(io)?;
        tokio::fs::rename(&tmp, path).await.map_err(io)
    }

    fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

// ── TurnStore ─────────────────────────────────────────────────────────────────

#[async_trait]
impl TurnStore for JsonFileStorage {
    async fn create_session(&self, data_dir: Option<&str>) -> Result<i64, StorageError> {
        let path = self.sessions_path();
        let mut rows: Vec<SessionRow> = Self::read_json(&path).await?;
        let id = self.next_id();
        rows.push(SessionRow {
            id,
            created_at: now_iso8601(),
            turn_count: 0,
            last_question: None,
            data_dir: data_dir.map(str::to_string),
        });
        Self::write_json_atomic(&path, &rows).await?;
        Ok(id)
    }

    async fn list_sessions(&self, limit: u32) -> Result<Vec<SessionSummary>, StorageError> {
        let path = self.sessions_path();
        let mut rows: Vec<SessionRow> = Self::read_json(&path).await?;
        rows.sort_by(|a, b| b.id.cmp(&a.id));
        rows.truncate(limit as usize);
        Ok(rows.iter().map(SessionSummary::from).collect())
    }

    async fn save_turn(&self, turn: &PersistedTurn) -> Result<i64, StorageError> {
        // Truncate artifacts.
        let mut turn = turn.clone();
        for artifact in &mut turn.artifacts {
            artifact.content = truncate_artifact_content(&artifact.content);
        }

        // Write turn file.
        let turn_path = self.turns_path(turn.session_id);
        let mut turns: Vec<PersistedTurn> = Self::read_json(&turn_path).await?;
        let id = self.next_id();
        turn.id = id;
        turns.push(turn.clone());
        Self::write_json_atomic(&turn_path, &turns).await?;

        // Update session metadata.
        let sess_path = self.sessions_path();
        let mut rows: Vec<SessionRow> = Self::read_json(&sess_path).await?;
        if let Some(row) = rows.iter_mut().find(|r| r.id == turn.session_id) {
            row.turn_count += 1;
            row.last_question = Some(turn.question.clone());
        }
        Self::write_json_atomic(&sess_path, &rows).await?;

        Ok(id)
    }

    async fn load_turns(&self, session_id: i64) -> Result<Vec<PersistedTurn>, StorageError> {
        let path = self.turns_path(session_id);
        let mut turns: Vec<PersistedTurn> = Self::read_json(&path).await?;
        turns.sort_by_key(|t| t.turn_index);
        Ok(turns)
    }
}

// ── QueryLog ──────────────────────────────────────────────────────────────────

#[async_trait]
impl QueryLog for JsonFileStorage {
    async fn log_query(&self, entry: &QueryLogEntry) -> Result<(), StorageError> {
        let mut line = serde_json::to_string(entry).map_err(|e| io(e))?;
        line.push('\n');

        let path = self.query_log_path();
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(io)?;
        file.write_all(line.as_bytes()).await.map_err(io)?;
        file.flush().await.map_err(io)?;
        Ok(())
    }
}

// ── PreferenceStore ───────────────────────────────────────────────────────────

#[async_trait]
impl PreferenceStore for JsonFileStorage {
    async fn get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let path = self.prefs_path();
        let map: HashMap<String, String> = Self::read_json(&path).await?;
        Ok(map.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let path = self.prefs_path();
        let mut map: HashMap<String, String> = Self::read_json(&path).await?;
        map.insert(key.to_string(), value.to_string());
        Self::write_json_atomic(&path, &map).await
    }
}

// ── SuspendedPipelineStore ────────────────────────────────────────────────────

#[async_trait]
impl SuspendedPipelineStore for JsonFileStorage {
    async fn save_suspended(&self, sp: &SuspendedPipeline) -> Result<(), StorageError> {
        let path = self.suspended_path(sp.session_id);
        Self::write_json_atomic(&path, sp).await
    }

    async fn take_suspended(
        &self,
        session_id: i64,
    ) -> Result<Option<SuspendedPipeline>, StorageError> {
        let path = self.suspended_path(session_id);
        let sp: Option<SuspendedPipeline> = match tokio::fs::read(&path).await {
            Ok(bytes) => Some(serde_json::from_slice(&bytes).map_err(|e| io(e))?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(io(e)),
        };
        if sp.is_some() {
            // Best-effort removal; ignore errors (e.g. concurrent delete).
            let _ = tokio::fs::remove_file(&path).await;
        }
        Ok(sp)
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn io<E: std::fmt::Display>(e: E) -> StorageError {
    StorageError::Io(e.to_string())
}

fn now_iso8601() -> String {
    // Use std::time for a minimal RFC-3339-like timestamp without extra deps.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as YYYY-MM-DDTHH:MM:SSZ (UTC, second resolution).
    let s = secs;
    let (sec, min, hour, day, mon, year) = {
        let s2 = s % 60;
        let m = (s / 60) % 60;
        let h = (s / 3600) % 24;
        let days = s / 86400;
        // Gregorian calendar approximation.
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
        for &days_in_month in &months {
            if d < days_in_month {
                break;
            }
            d -= days_in_month;
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
    use crate::app_storage::{Artifact, StorageHandle};

    async fn tmp_store() -> (JsonFileStorage, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStorage::open(dir.path().to_path_buf())
            .await
            .unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn session_lifecycle() {
        let (store, _dir) = tmp_store().await;
        let id = store.create_session(Some("/data")).await.unwrap();
        let sessions = store.list_sessions(10).await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
        assert_eq!(sessions[0].data_dir.as_deref(), Some("/data"));
    }

    #[tokio::test]
    async fn save_and_load_turns() {
        let (store, _dir) = tmp_store().await;
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
            created_at: now_iso8601(),
            duration_ms: Some(100),
        };
        store.save_turn(&turn).await.unwrap();

        let turns = store.load_turns(sid).await.unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].question, "q");
        assert_eq!(turns[0].artifacts[0].kind, "sql");

        // Session summary updated.
        let sessions = store.list_sessions(10).await.unwrap();
        assert_eq!(sessions[0].turn_count, 1);
        assert_eq!(sessions[0].last_question.as_deref(), Some("q"));
    }

    #[tokio::test]
    async fn artifact_truncation() {
        let (store, _dir) = tmp_store().await;
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
            created_at: now_iso8601(),
            duration_ms: None,
        };
        store.save_turn(&turn).await.unwrap();
        let turns = store.load_turns(sid).await.unwrap();
        assert!(turns[0].artifacts[0].content.ends_with("…[truncated]"));
        assert!(turns[0].artifacts[0].content.len() < 70_000);
    }

    #[tokio::test]
    async fn query_log() {
        let (store, dir) = tmp_store().await;
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
        let path = dir.path().join("query_log.jsonl");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("SELECT 1"));
    }

    #[tokio::test]
    async fn preferences() {
        let (store, _dir) = tmp_store().await;
        assert_eq!(store.get("thinking").await.unwrap(), None);
        store.set("thinking", "adaptive").await.unwrap();
        assert_eq!(
            store.get("thinking").await.unwrap(),
            Some("adaptive".into())
        );
        store.set("thinking", "disabled").await.unwrap();
        assert_eq!(
            store.get("thinking").await.unwrap(),
            Some("disabled".into())
        );
    }

    #[tokio::test]
    async fn storage_handle_wraps_adapter() {
        let (store, _dir) = tmp_store().await;
        let handle = StorageHandle::from_adapter(store);
        let id = handle.turns.create_session(None).await.unwrap();
        assert!(id > 0);
    }
}
