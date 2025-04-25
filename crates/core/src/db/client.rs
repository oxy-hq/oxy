use once_cell::sync::Lazy;
use sea_orm::{Database, DatabaseConnection};
use std::fs;
use std::path::{Path, PathBuf};

fn resolve_state_dir() -> PathBuf {
    if let Ok(env_dir) = std::env::var("OXY_STATE_DIR") {
        let path = PathBuf::from(env_dir);
        ensure_dir_exists(&path);
        return path;
    }
    let homedir = home::home_dir().unwrap_or_else(|| {
        eprintln!("Error: Could not determine home directory.");
        std::process::exit(1);
    });
    let path = homedir.join(".local/share/oxy");
    ensure_dir_exists(&path);
    path
}

fn ensure_dir_exists(path: &Path) {
    if !path.exists() {
        if let Err(e) = fs::create_dir_all(path) {
            eprintln!("Error: Could not create directory: {}", e);
            std::process::exit(1);
        }
    }
}

static STATE_DIR: Lazy<PathBuf> = Lazy::new(resolve_state_dir);

/// Returns a reference to the state directory path.
pub fn get_state_dir() -> &'static Path {
    STATE_DIR.as_path()
}

static CHARTS_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let charts_dir = get_state_dir().join("charts");
    ensure_dir_exists(&charts_dir);
    charts_dir
});

pub fn get_charts_dir() -> &'static Path {
    CHARTS_DIR.as_path()
}

pub async fn establish_connection() -> DatabaseConnection {
    let db_url = std::env::var("OXY_DATABASE_URL").ok();
    if let Some(url) = db_url {
        Database::connect(url).await.unwrap()
    } else {
        let db_path = format!(
            "sqlite://{}/db.sqlite?mode=rwc",
            get_state_dir().to_str().unwrap()
        );
        Database::connect(db_path).await.unwrap()
    }
}
