use once_cell::sync::Lazy;
use sea_orm::{Database, DatabaseConnection};
use std::fs;
use std::path::PathBuf;

static STATE_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let homedir = match home::home_dir() {
        Some(dir) => dir,
        None => {
            eprintln!("Error: Could not determine home directory.");
            std::process::exit(1);
        }
    };

    let state_dir = homedir.join(".local/share/onyx");
    if !state_dir.exists() {
        if let Err(e) = fs::create_dir_all(&state_dir) {
            eprintln!("Error: Could not create state directory: {}", e);
            std::process::exit(1);
        }
    }
    state_dir
});

pub fn get_state_dir() -> String {
    STATE_DIR.as_path().to_str().unwrap().to_string()
}

pub async fn establish_connection() -> DatabaseConnection {
    let db_path = format!("sqlite://{}/db.sqlite?mode=rwc", get_state_dir());
    let db: DatabaseConnection = Database::connect(db_path).await.unwrap();
    db
}
