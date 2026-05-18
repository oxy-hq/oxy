//! Workspace-contained resolution of a caller-supplied `pipeline_ref`.
//!
//! `pipeline_ref` arrives from untrusted HTTP input. Naively doing
//! `workspace_root.join(pipeline_ref)` lets `../../etc/passwd` escape
//! the workspace. This mirrors the workflow guard
//! (`agentic_wiring::project_ctx::resolve_workspace_relative`):
//! syntactic reject of empty/absolute/`..` refs, then a
//! canonical-containment check so symlink escapes are caught too.
//!
//! Errors quote **only** the caller-supplied `pipeline_ref`, never the
//! resolved absolute path, so a failed traversal can't be used to
//! probe workspace/host layout.

use std::path::{Component, Path, PathBuf};

/// Resolve `pipeline_ref` to an absolute path guaranteed to live under
/// `workspace_root`. Rejects absolute paths, `..` traversal, and empty
/// refs; the canonical result must remain under the canonical root.
pub fn resolve_pipeline_ref(workspace_root: &Path, pipeline_ref: &str) -> Result<PathBuf, String> {
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
    use super::resolve_pipeline_ref;
    use std::fs;

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
