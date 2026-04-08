use std::path::{Path, PathBuf};

use agentic_core::tools::ToolError;

// ── Constants ────────────────────────────────────────────────────────────────

pub const MAX_FILE_LINES: usize = 500;
pub const MAX_SEARCH_RESULTS: usize = 50;
pub const MAX_FILE_RESULTS: usize = 100;

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Validate that `path` is within `workspace_root`. Returns the resolved absolute path.
pub fn safe_path(workspace_root: &Path, path: &str) -> Result<PathBuf, ToolError> {
    // Reject obviously dangerous paths before canonicalize (which requires file to exist).
    if path.contains("..") {
        return Err(ToolError::BadParams(format!(
            "path traversal not allowed: {path}"
        )));
    }

    let joined = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        workspace_root.join(path)
    };

    // Check prefix before canonicalize so we can give a clear error.
    let canonical_root = workspace_root
        .canonicalize()
        .map_err(|e| ToolError::Execution(format!("failed to resolve project root: {e}")))?;

    // Resolve symlinks only if the file exists; otherwise just check the path prefix.
    let abs = if joined.exists() {
        joined
            .canonicalize()
            .map_err(|e| ToolError::Execution(format!("failed to resolve path: {e}")))?
    } else {
        // For non-existent files (e.g. propose_change creating a new file),
        // normalize the path without canonicalize.
        let mut components = Vec::new();
        for c in joined.components() {
            use std::path::Component;
            match c {
                Component::ParentDir => {
                    components.pop();
                }
                Component::CurDir => {}
                other => components.push(other),
            }
        }
        components.iter().collect::<PathBuf>()
    };

    if !abs.starts_with(&canonical_root) {
        return Err(ToolError::BadParams(format!(
            "path is outside project root: {path}"
        )));
    }

    Ok(abs)
}
