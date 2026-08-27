//! Workspace-contained resolution of a caller-supplied `pipeline_ref`,
//! and the one place that decides *where* a pipeline's YAML comes from.
//!
//! `pipeline_ref` arrives from untrusted HTTP input. Naively doing
//! `workspace_root.join(pipeline_ref)` lets `../../etc/passwd` escape
//! the workspace. This mirrors the automation guard
//! (`agentic_wiring::project_ctx::resolve_workspace_relative`):
//! syntactic reject of empty/absolute/`..` refs, then a
//! canonical-containment check so symlink escapes are caught too.
//!
//! Errors quote **only** the caller-supplied `pipeline_ref`, never the
//! resolved absolute path, so a failed traversal can't be used to
//! probe workspace/host layout.
//!
//! [`load_pipeline_yaml`] is the compile-boundary entry point: it asks the
//! host for the compiled body first and only reads the workspace filesystem
//! when the host declines. Every airway read site goes through it, so the
//! containment guarantee above is unconditional — the syntactic guard runs
//! before *either* backend, and the canonical-containment check runs on the
//! FS backend.

use std::path::{Component, Path, PathBuf};

use agentic_automation::WorkspaceContext;

/// Why a `pipeline_ref` could not be turned into YAML.
///
/// The first two are diagnostic, not a status-code mapping — `agentic-http`'s
/// airway route matches them together and answers 400 (`routes/airway.rs`).
/// `Io` is close to unreachable now that `resolve_pipeline_ref` canonicalises
/// first, so a missing file is `Invalid` before any read is attempted; it stays
/// because "resolved but unreadable" (permissions, a racing delete) is a
/// genuinely different fact from "never resolved".
///
/// [`Unavailable`](Self::Unavailable) is the one that carries weight. It says
/// *this node could not answer*, which is neither of the above, and callers key
/// their retry on it — see `airway_config::PipelineAirwayAdmissionResolver`.
/// Before it existed, a compile-boundary blip on a stateless worker arrived
/// here as `Invalid("workspace root is not accessible")` and read as caller
/// input: the host laundered its `Err` into "not compiled", the FS fallback
/// found no working copy, and a retryable condition killed the run on attempt
/// one. `product-context.md` states the requirement this closes — a
/// not-yet-compiled workspace must return a **retryable** state, and mid-deploy
/// "workspace directory not found" is transient.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PipelineRefError {
    /// Ref rejected by the containment guard, or no such pipeline.
    #[error("{0}")]
    Invalid(String),
    /// The ref resolved but its bytes could not be read.
    #[error("{0}")]
    Io(String),
    /// Neither backend could answer *here*: the host could not be asked (a
    /// compile-boundary lookup error) or there is nothing compiled and this
    /// process holds no working copy. Retryable — the ref may be perfectly
    /// good, and another node or another moment may resolve it.
    #[error("{0}")]
    Unavailable(String),
}

/// Syntactic half of the containment guard: reject empty, absolute, and
/// `..`-bearing refs. Runs on **both** the compiled-row and the filesystem
/// backend — a ref that could escape a workspace on disk must not be allowed
/// to address a row either. Returns the trimmed ref on success.
///
/// Errors quote only the caller-supplied ref.
fn validate_pipeline_ref(pipeline_ref: &str) -> Result<&str, String> {
    let trimmed = pipeline_ref.trim();
    if trimmed.is_empty() {
        return Err("pipeline_ref must not be empty".to_string());
    }
    let candidate = Path::new(trimmed);
    if candidate.is_absolute() {
        return Err(format!(
            "pipeline_ref {pipeline_ref:?} must be relative to the workspace"
        ));
    }
    if candidate
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(format!(
            "pipeline_ref {pipeline_ref:?} must not contain `..` segments"
        ));
    }
    Ok(trimmed)
}

/// Load a pipeline's YAML for `pipeline_ref`, compile boundary first.
///
/// 1. Contain the ref (syntactic guard above) — untrusted input, both paths.
/// 2. Ask the host: [`WorkspaceContext::resolve_pipeline_yaml`]. `Ok(Some(yaml))`
///    means it came from the workspace's compiled `airway_pipelines` rows,
///    scoped to that workspace's promoted revision. No filesystem involved —
///    this is the path a stateless durable worker takes.
/// 3. `Ok(None)` means "read the FS" (host doesn't do the boundary, workspace
///    is the legacy local one, branch is a draft, or nothing is compiled yet).
///    Fall through to the canonical-containment-checked read.
/// 4. `Err` means the host could not be *asked* — a database blip, not an
///    answer. That is [`PipelineRefError::Unavailable`] and never a fall-through
///    to the FS: a working copy that happens to exist would answer a question
///    the boundary was supposed to answer, which is the instance-affinity
///    divergence this module exists to remove.
///
/// Step 3 has its own `Unavailable` case, and it is the one that used to be
/// mis-classified: if there is no compiled row *and* this process holds no
/// working copy, nothing here can answer, so the root being inaccessible is a
/// fact about the node and not about the caller's ref.
///
/// Rendering is deliberately **not** done here: callers pass their own
/// `variables` to `AirwayPipelineSpec::from_yaml_with_vars`, so the submitter
/// and the worker each render the same document independently.
pub async fn load_pipeline_yaml(
    workspace: &dyn WorkspaceContext,
    pipeline_ref: &str,
) -> Result<String, PipelineRefError> {
    let trimmed = validate_pipeline_ref(pipeline_ref).map_err(PipelineRefError::Invalid)?;

    match workspace.resolve_pipeline_yaml(trimmed).await {
        Ok(Some(yaml)) => return Ok(yaml),
        Ok(None) => {}
        Err(e) => {
            return Err(PipelineRefError::Unavailable(format!(
                "compile boundary unavailable for pipeline_ref `{pipeline_ref}`: {e}"
            )));
        }
    }

    // `workspace_path()` is an `Option`: `None` is a node that declares no
    // working copy, `Some(p)` with `!p.is_dir()` is one that declares one and
    // does not have it. Both mean the same thing here.
    let root = workspace.workspace_path().filter(|p| p.is_dir());
    let Some(root) = root else {
        // Deliberately checked before `resolve_pipeline_ref`, which folds this
        // into the same `Err(String)` as "not found" and so cannot be told
        // apart downstream.
        // Says only what is known. "not compiled for this revision" would be
        // false whenever the host declined for another reason — a draft branch,
        // or a row it found and could not re-serialise.
        return Err(PipelineRefError::Unavailable(format!(
            "pipeline_ref `{pipeline_ref}` could not be resolved on this node (nothing served from \
             the compile boundary, no working copy here)"
        )));
    };

    let path = resolve_pipeline_ref(root, trimmed).map_err(PipelineRefError::Invalid)?;
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| PipelineRefError::Io(format!("read pipeline_ref `{pipeline_ref}`: {e}")))
}

/// Resolve `pipeline_ref` to an absolute path guaranteed to live under
/// `workspace_root`. Rejects absolute paths, `..` traversal, and empty
/// refs; the canonical result must remain under the canonical root.
pub fn resolve_pipeline_ref(workspace_root: &Path, pipeline_ref: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(validate_pipeline_ref(pipeline_ref)?);

    // Defence-in-depth: even with the syntactic checks above, a symlink
    // inside the workspace could still point out of it. Canonicalise
    // both sides and require containment. Canonicalisation also
    // resolves the (now provably in-workspace) path we read from.
    let root = workspace_root
        .canonicalize()
        .map_err(|_| "workspace root is not accessible".to_string())?;
    let resolved = root
        .join(candidate)
        .canonicalize()
        .map_err(|_| format!("pipeline_ref {pipeline_ref:?} not found"))?;
    if !resolved.starts_with(&root) {
        return Err(format!(
            "pipeline_ref {pipeline_ref:?} escapes the workspace"
        ));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::{PipelineRefError, load_pipeline_yaml, resolve_pipeline_ref};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// Minimal host: `workspace_path()` points at a directory that does not
    /// exist (a stateless durable worker), and the compile-boundary hook
    /// answers with `compiled`.
    struct FakeHost {
        root: PathBuf,
        /// Exactly the port's three answers: served, nothing here, could not
        /// look. The last one is the case that has no `Option` spelling, which
        /// is why the port is a `Result`.
        compiled: Result<Option<String>, String>,
    }

    #[async_trait::async_trait]
    impl agentic_automation::WorkspaceContext for FakeHost {
        fn workspace_path(&self) -> Option<&Path> {
            Some(&self.root)
        }
        fn database_configs(&self) -> Vec<airlayer::DatabaseConfig> {
            vec![]
        }
        async fn get_connector(
            &self,
            _name: &str,
        ) -> Result<Arc<dyn agentic_connector::DatabaseConnector>, String> {
            Err("unused".into())
        }
        async fn get_integration(
            &self,
            _name: &str,
        ) -> Result<agentic_automation::workspace::IntegrationConfig, String> {
            Err("unused".into())
        }
        async fn list_automation_files(&self) -> Result<Vec<PathBuf>, String> {
            Ok(vec![])
        }
        async fn resolve_automation_yaml(
            &self,
            _r: &str,
        ) -> Result<String, crate::WorkspaceReadError> {
            Err("unused".into())
        }
        async fn resolve_pipeline_yaml(
            &self,
            _pipeline_ref: &str,
        ) -> Result<Option<String>, String> {
            self.compiled.clone()
        }
    }

    /// The executor's production shape: no workspace directory anywhere on
    /// disk. Before the compile boundary landed this could only fail —
    /// `resolve_pipeline_ref` canonicalises the root first ("workspace root is
    /// not accessible"), which is the instance-affinity symptom on a stateless
    /// replica.
    #[tokio::test]
    async fn compiled_body_is_served_without_any_workspace_directory() {
        let host = FakeHost {
            root: PathBuf::from("/nonexistent-oxy-workspace/does/not/exist"),
            compiled: Ok(Some("name: from_boundary\n".to_string())),
        };
        assert!(!host.root.exists(), "precondition: no working copy");

        let yaml = load_pipeline_yaml(&host, "pipelines/p.airway.yml")
            .await
            .expect("compiled row must satisfy the read with no filesystem");
        assert_eq!(yaml, "name: from_boundary\n");
    }

    /// Host declines (unpromoted / draft branch / local workspace) → the FS
    /// read, unchanged.
    #[tokio::test]
    async fn falls_through_to_the_filesystem_when_the_host_declines() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("pipelines")).unwrap();
        fs::write(dir.path().join("pipelines/p.airway.yml"), "name: from_fs\n").unwrap();
        let host = FakeHost {
            root: dir.path().to_path_buf(),
            compiled: Ok(None),
        };

        let yaml = load_pipeline_yaml(&host, "pipelines/p.airway.yml")
            .await
            .expect("FS fallback");
        assert_eq!(yaml, "name: from_fs\n");

        // A miss on the FS path fails in `resolve_pipeline_ref`, which
        // canonicalises before reading — so it is `Invalid` (the path never
        // resolved), not `Io` (resolved but unreadable). Either way it quotes
        // only the ref.
        let err = load_pipeline_yaml(&host, "pipelines/nope.airway.yml")
            .await
            .unwrap_err();
        assert!(matches!(err, PipelineRefError::Invalid(_)));
        assert!(!err.to_string().contains(&*dir.path().to_string_lossy()));
    }

    /// An `Err` from the compile boundary must NOT fall through to a working
    /// copy that happens to exist.
    ///
    /// The root here is real and DOES contain the pipeline, so a fall-through
    /// implementation returns `Ok(yaml)` and this fails. Every other
    /// `Unavailable` test points at a nonexistent root, where both
    /// implementations return `Unavailable` for different reasons and the
    /// assertion proves nothing about this rule.
    ///
    /// The rule matters because the alternative is two nodes giving two answers
    /// for one revision — an IDE box serving its working copy while a replica
    /// serves the compiled row — which is the divergence the boundary exists to
    /// remove.
    #[tokio::test]
    async fn a_boundary_error_does_not_fall_through_to_an_existing_working_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("pipelines")).expect("mkdir");
        fs::write(
            dir.path().join("pipelines/p.airway.yml"),
            "name: from_working_copy\n",
        )
        .expect("write");

        let host = FakeHost {
            root: dir.path().to_path_buf(),
            compiled: Err("connection reset by peer".into()),
        };

        let err = load_pipeline_yaml(&host, "pipelines/p.airway.yml")
            .await
            .expect_err("a boundary error must not be answered from the working copy");
        assert!(
            matches!(err, PipelineRefError::Unavailable(_)),
            "got {err:?}"
        );
    }

    /// Containment is enforced BEFORE the backend choice, so a traversal ref
    /// can't address a compiled row either — even a host that would happily
    /// return a body never sees the ref.
    #[tokio::test]
    async fn containment_applies_to_the_compiled_path_too() {
        let host = FakeHost {
            root: PathBuf::from("/nonexistent-oxy-workspace"),
            compiled: Ok(Some("name: attacker\n".to_string())),
        };
        for bad in ["", "   ", "/etc/passwd", "../../etc/passwd", "a/../../b"] {
            let err = load_pipeline_yaml(&host, bad)
                .await
                .expect_err("traversal/empty refs must be rejected before any backend");
            assert!(matches!(err, PipelineRefError::Invalid(_)));
        }
        // And the error still never leaks a resolved absolute path.
        let err = load_pipeline_yaml(&host, "../secret").await.unwrap_err();
        assert!(err.to_string().contains("\"../secret\""));
        assert!(!err.to_string().contains("nonexistent-oxy-workspace"));
    }

    #[test]
    fn rejects_empty_absolute_and_parent_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(resolve_pipeline_ref(root, "").is_err());
        assert!(resolve_pipeline_ref(root, "   ").is_err());
        assert!(resolve_pipeline_ref(root, "/etc/passwd").is_err());
        assert!(resolve_pipeline_ref(root, "../../etc/passwd").is_err());
        assert!(resolve_pipeline_ref(root, "a/../../b").is_err());
        // Error must not leak the resolved absolute path.
        let err = resolve_pipeline_ref(root, "../secret").unwrap_err();
        assert!(err.contains("\"../secret\""));
        assert!(!err.contains(&*root.to_string_lossy()));
    }

    #[test]
    fn resolves_a_real_in_workspace_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("pipelines")).unwrap();
        fs::write(root.join("pipelines/p.airway.yml"), "name: p").unwrap();
        let p = resolve_pipeline_ref(root, "pipelines/p.airway.yml").unwrap();
        assert!(p.starts_with(root.canonicalize().unwrap()));
        assert!(p.ends_with("pipelines/p.airway.yml"));
    }

    #[test]
    fn missing_file_errors_without_leaking_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = resolve_pipeline_ref(dir.path(), "pipelines/nope.airway.yml").unwrap_err();
        assert!(err.contains("not found"));
        assert!(!err.contains(&*dir.path().to_string_lossy()));
    }
}
