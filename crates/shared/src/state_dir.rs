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
    let path = state_dir_path(fallback);
    ensure_dir_exists(&path);
    path
}

/// The same resolution, creating nothing.
///
/// Constructing a workspace manager must not bring a workspace root into
/// existence. The fallback state dir lives *inside* the workspace root, so
/// creating it on a node that has never cloned that workspace leaves an empty
/// root behind — and from then on "this node holds no working copy" and "the
/// customer configured nothing" are the same directory, which is the shape
/// behind both shipped incidents. Whoever writes into the state dir calls
/// [`resolve_state_dir_with_fallback`] and gets it created then.
pub fn state_dir_path(fallback: Option<PathBuf>) -> PathBuf {
    if let Ok(env_dir) = std::env::var("OXY_STATE_DIR") {
        return PathBuf::from(env_dir);
    }

    fallback.unwrap_or_else(|| {
        let homedir = home::home_dir().unwrap_or_else(|| {
            eprintln!("Error: Could not determine home directory.");
            std::process::exit(1);
        });
        homedir.join(".local/share/oxy")
    })
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

/// The cache key for one workspace's pre-aggregations: its workspace id.
///
/// Deliberately NOT a hash of the workspace path, which is what this was until
/// the Pre-aggregation tab made the difference visible. A path is not stable
/// per workspace: the request path resolves `?branch=` to a
/// `.worktrees/<branch>` checkout while the rebuild cycle always runs against
/// the default-branch root, so the reader and the writer hashed to two
/// different directories and every rollup read "Not cached" on any feature
/// branch — the IDE's normal state. It is also not tenant-safe as a blob key:
/// `preagg_blob` puts these under one shared multi-tenant bucket prefix, where
/// a workspace id is the identity that belongs there.
///
/// One workspace has one cache, whichever branch is checked out and whichever
/// node built it. Rollup hashes already fold in view + rollup + grain, so
/// nothing below this key needs the branch to disambiguate.
pub fn airlayer_cache_key(workspace_id: uuid::Uuid) -> String {
    workspace_id.to_string()
}

/// Returns the airlayer pre-aggregation cache directory for the given workspace.
///
/// Path: `<oxy_state>/airlayer/cache/<workspace_id>/`
///
/// Using the state directory (not the workspace) ensures the cache persists
/// in cloud deployments where the workspace directory is ephemeral.
pub fn get_airlayer_cache_dir(workspace_id: uuid::Uuid) -> PathBuf {
    get_state_dir()
        .join("airlayer")
        .join("cache")
        .join(airlayer_cache_key(workspace_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airlayer_cache_dir_is_under_state_dir() {
        let cache = get_airlayer_cache_dir(uuid::Uuid::nil());
        assert!(
            cache.starts_with(get_state_dir()),
            "cache dir {cache:?} should be under state dir {:?}",
            get_state_dir()
        );
    }

    #[test]
    fn different_workspaces_produce_different_cache_dirs() {
        let a = get_airlayer_cache_dir(uuid::Uuid::from_u128(1));
        let b = get_airlayer_cache_dir(uuid::Uuid::from_u128(2));
        assert_ne!(a, b);
    }

    #[test]
    fn the_key_does_not_vary_with_the_checked_out_branch() {
        // The whole point of keying on the id: the rebuild cycle (default
        // branch root) and a `?branch=`-scoped read (a `.worktrees/<branch>`
        // root) must land on the SAME directory.
        let id = uuid::Uuid::from_u128(7);
        assert_eq!(get_airlayer_cache_dir(id), get_airlayer_cache_dir(id));
        assert_eq!(
            get_airlayer_cache_dir(id).file_name().unwrap(),
            std::ffi::OsStr::new(&id.to_string())
        );
    }
}
