use std::path::{Path, PathBuf};

use oxy_shared::errors::OxyError;

/// Validates that `file_path` (relative to `root`) stays inside `root` after
/// canonicalisation, guarding against path traversal (e.g. `../../etc/shadow`).
///
/// For files that may not yet exist, the parent directory is canonicalised
/// instead and the file name re-joined.
///
/// Returns the canonicalised absolute path on success.
pub fn validate_file_path(root: &Path, file_path: &str) -> Result<PathBuf, OxyError> {
    let full_path = root.join(file_path);
    let canonical = full_path
        .canonicalize()
        .or_else(|_| {
            full_path
                .parent()
                .ok_or_else(|| std::io::Error::other("no parent directory"))
                .and_then(|p| p.canonicalize())
                .map(|p| p.join(full_path.file_name().unwrap_or_default()))
        })
        .map_err(|e| OxyError::ArgumentError(format!("Invalid file path: {e}")))?;

    if !canonical.starts_with(root) {
        return Err(OxyError::ArgumentError(format!(
            "File path escapes project root: {file_path}"
        )));
    }
    Ok(canonical)
}
