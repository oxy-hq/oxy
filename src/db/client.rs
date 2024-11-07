use sea_orm::{Database, DatabaseConnection};

pub async fn establish_connection() -> DatabaseConnection {
    let database_url = "sqlite://./db.sqlite?mode=rwc"; // hard coded for now, use db file in the root directory

    let db: DatabaseConnection = Database::connect(database_url).await.unwrap();
    db
}
