use once_cell::sync::Lazy;
use std::fs;
use std::path::{Path, PathBuf};

/// Resolves the state directory path with optional fallback.
/// First checks OXY_STATE_DIR environment variable.
/// If not set, uses the provided fallback path.
///
/// # Arguments
/// * `fallback` - Optional fallback path to use if OXY_STATE_DIR is not set
pub fn resolve_state_dir_with_fallback(fallback: Option<PathBuf>) -> PathBuf {
    if let Ok(env_dir) = std::env::var("OXY_STATE_DIR") {
        let path = PathBuf::from(env_dir);
        ensure_dir_exists(&path);
        return path;
    }

    let path = fallback.unwrap_or_else(|| {
        let homedir = home::home_dir().unwrap_or_else(|| {
            eprintln!("Error: Could not determine home directory.");
            std::process::exit(1);
        });
        homedir.join(".local/share/oxy")
    });

    ensure_dir_exists(&path);
    path
}

fn resolve_state_dir() -> PathBuf {
    resolve_state_dir_with_fallback(None)
}

fn ensure_dir_exists(path: &Path) {
    if !path.exists()
        && let Err(e) = fs::create_dir_all(path)
    {
        eprintln!("Error: Could not create directory: {e}");
        std::process::exit(1);
    }
}

static STATE_DIR: Lazy<PathBuf> = Lazy::new(resolve_state_dir);

pub fn get_state_dir() -> &'static Path {
    STATE_DIR.as_path()
}

/// Returns the airlayer pre-aggregation cache directory for the given workspace.
///
/// Path: `<oxy_state>/airlayer/cache/<sha256_of_workspace_path[:12]>/`
///
/// Using the state directory (not the workspace) ensures the cache persists
/// in cloud deployments where the workspace directory is ephemeral.
pub fn get_airlayer_cache_dir(workspace_path: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(workspace_path.as_os_str().as_encoded_bytes());
    let hash = hex::encode(hasher.finalize());
    let key = &hash[..12];
    get_state_dir().join("airlayer").join("cache").join(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airlayer_cache_dir_is_under_state_dir() {
        let cache = get_airlayer_cache_dir(std::path::Path::new("/some/workspace"));
        assert!(
            cache.starts_with(get_state_dir()),
            "cache dir {cache:?} should be under state dir {:?}",
            get_state_dir()
        );
    }

    #[test]
    fn different_workspace_paths_produce_different_cache_dirs() {
        let a = get_airlayer_cache_dir(std::path::Path::new("/workspace/a"));
        let b = get_airlayer_cache_dir(std::path::Path::new("/workspace/b"));
        assert_ne!(a, b);
    }

    #[test]
    fn same_workspace_path_is_deterministic() {
        let p = std::path::Path::new("/workspace/foo");
        assert_eq!(get_airlayer_cache_dir(p), get_airlayer_cache_dir(p));
    }
}
