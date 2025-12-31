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

/// Returns a reference to the state directory path.
pub fn get_state_dir() -> &'static Path {
    STATE_DIR.as_path()
}
