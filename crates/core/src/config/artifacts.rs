use oxy_shared::errors::OxyError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub name: String,
    pub file_path: String,
    pub title: Option<String>,
    pub published: bool,
}

/// An Airway pipeline as a listing row. `name` follows the same rule as every
/// other compiled entity: the YAML `name:`, else derived from the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineEntry {
    pub name: String,
    pub file_path: String,
    /// `source.kind` — what the UI keys source-specific surfaces on. Both
    /// origins fill it the same way ([`pipeline_source_kind`]), so the compiled
    /// row and the working-copy read answer the same kind for the same
    /// pipeline. `None` means the definition could not be read or did not
    /// parse — a surface gated on it stays hidden rather than appearing and
    /// then failing.
    pub source_kind: Option<String>,
}

/// `source.kind` out of a pipeline definition, if it names one.
pub fn pipeline_source_kind(definition: &serde_json::Value) -> Option<String> {
    definition
        .get("source")
        .and_then(|src| src.get("kind"))
        .and_then(|k| k.as_str())
        .filter(|k| !k.is_empty())
        .map(str::to_string)
}

/// An analytics agent as a listing row. `model_ref` and `timezone` are pulled
/// out so the home page can flag a missing LLM key for the agent chat will
/// actually use, and render the workspace's local clock, without a caller
/// re-parsing the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    pub name: String,
    pub file_path: String,
    pub model_ref: Option<String>,
    pub timezone: Option<String>,
}

/// An automation as a listing row.
///
/// `extension` is carried because three of them are accepted — `.automation.yml`
/// is canonical, `.procedure.yml` is legacy-but-live, and `.workflow.yml` is no
/// longer a recognised file kind, so the walker never compiles one and it
/// resolves from the working copy or not at all. The file tree groups by it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationEntry {
    pub name: String,
    pub file_path: String,
    pub extension: String,
}

/// One compiled artifact with its full body — the shape every kind shares
/// (semantic views, topics, automations), as opposed to the listing rows above
/// which carry only what a list needs.
///
/// `blob_key` rather than the body: when a large body lives in S3 the row keeps
/// only the key, and fetching it needs the S3 client, which lives above this
/// crate. Core reports what the row says; the caller resolves the blob and
/// falls back to `definition` when the bucket is unset or the object is gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledArtifact {
    pub name: String,
    pub file_path: String,
    pub definition: serde_json::Value,
    pub blob_key: Option<String>,
}

/// A verified query (`.sql`). No parsed `definition` — its body IS the SQL,
/// carried verbatim with the hash the compile worker recorded so a reader can
/// check the Postgres/S3 round-trip did not corrupt it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedQueryEntry {
    pub file_path: String,
    pub content_sha256: String,
    pub content: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("workspace is not available on this node: {0}")]
    WorkspaceUnavailable(String),
    #[error("not in the compiled revision, and this process holds no working copy")]
    NoSource,
    #[error("compile boundary unavailable: {0}")]
    Backend(String),
    #[error(transparent)]
    Config(#[from] OxyError),
}

/// So a caller still on `OxyError` can use `?` without restating the mapping.
/// The wrapped variant passes through unchanged; the three that describe a
/// source being unavailable become a runtime error, because that is what they
/// are to code that cannot act on the distinction.
impl From<ArtifactError> for OxyError {
    fn from(e: ArtifactError) -> Self {
        match e {
            ArtifactError::Config(inner) => inner,
            other => OxyError::RuntimeError(other.to_string()),
        }
    }
}

impl ArtifactError {
    pub fn retryable(&self) -> bool {
        !matches!(self, Self::Config(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_config_fault_is_permanent() {
        assert!(ArtifactError::NoSource.retryable());
        assert!(ArtifactError::WorkspaceUnavailable("x".into()).retryable());
        assert!(ArtifactError::Backend("db down".into()).retryable());
        assert!(
            !ArtifactError::Config(OxyError::ConfigurationError("bad yaml".into())).retryable(),
            "a broken file is the caller's fault and will not fix itself on retry"
        );
    }
}
