use sea_orm::{Database, DatabaseConnection};

pub fn get_db_directory() -> String {
    let homedir = home::home_dir().unwrap();
    let db_directory = homedir.join(".local/share/onyx");
    return db_directory.as_path().to_str().unwrap().to_string();
}

pub async fn establish_connection() -> DatabaseConnection {
    let db_path = format!("sqlite://{}/db.sqlite?mode=rwc", get_db_directory());
    let db: DatabaseConnection = Database::connect(db_path).await.unwrap();
    db
}
