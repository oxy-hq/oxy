//! What `compile_workspace` returns + the per-file detail it records
//! into the `revisions` row's `error_summary` JSONB.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Result of a full compile run. Whether `status` is `Ready` or
/// `Failed`, the `revisions` row was written successfully and the
/// per-entity tables hold the rows for every file that compiled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompileOutcome {
    pub revision_id: Uuid,
    pub status: RevisionStatus,
    pub git_sha: String,
    pub branch: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub file_count_seen: u32,
    pub file_count_compiled: u32,
    pub file_count_failed: u32,
    /// Per-file failure details. Empty when status is Ready.
    pub failures: Vec<FileFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionStatus {
    Ready,
    Failed,
}

impl RevisionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RevisionStatus::Ready => "ready",
            RevisionStatus::Failed => "failed",
        }
    }
}

/// Per-file failure recorded into the revision's `error_summary`. Plain
/// JSON so the admin UI can read it back without a typed deserialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFailure {
    /// Workspace-relative path. Forward slashes regardless of OS so
    /// the JSON is portable.
    pub path: String,
    /// Compile-time classification. Lets the UI group by failure
    /// shape (the same trick the Internal Jobs dead-letter inspector
    /// uses).
    pub kind: FailureKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// `serde_yaml::from_str` returned an error — the YAML is
    /// malformed.
    Yaml,
    /// File could not be read from disk.
    Io,
    /// The YAML parsed but required field was missing (e.g. an
    /// `.app.yml` with no top-level structure).
    Shape,
    /// Two files of the same kind compiled to the same identifier
    /// (e.g. two agents named "foo"). Both are recorded as failures
    /// so the operator sees both paths.
    Duplicate,
}
