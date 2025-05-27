use sea_orm_migration::{
    prelude::*,
    schema::*,
    sea_orm::{DbBackend, Statement},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            DbBackend::Sqlite => {
                let stmt = Statement::from_string(
                    DbBackend::Sqlite,
                    r#"PRAGMA foreign_keys=off;
BEGIN TRANSACTION;

ALTER TABLE threads RENAME TO _threads_old;

CREATE TABLE "threads" (
  "id" uuid_text NOT NULL PRIMARY KEY,
  "title" varchar NOT NULL,
  "input" varchar NOT NULL,
  "output" varchar NOT NULL,
  "source" varchar NOT NULL,
  "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "references" text NOT NULL,
  "source_type" varchar NOT NULL DEFAULT '',
  "user_id" uuid_text NULL,

  CONSTRAINT fk_threads_user_id
    FOREIGN KEY (user_id)
    REFERENCES users (id)
);

INSERT INTO threads SELECT *, NULL FROM _threads_old;

DROP TABLE _threads_old;

COMMIT;

PRAGMA foreign_keys=on;

"#,
                );
                manager.get_connection().execute(stmt).await?;
            }
            _ => {
                // Add user_id column to threads table (nullable for existing records)
                manager
                    .alter_table(
                        Table::alter()
                            .table(Threads::Table)
                            .add_column(uuid_null(Threads::UserId))
                            .to_owned(),
                    )
                    .await?;

                // Add foreign key constraint
                manager
                    .create_foreign_key(
                        ForeignKey::create()
                            .name("fk_threads_user_id")
                            .from(Threads::Table, Threads::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::NoAction)
                            .to_owned(),
                    )
                    .await?;
            }
        }

        // Create index on user_id for faster lookups
        manager
            .create_index(
                Index::create()
                    .table(Threads::Table)
                    .name("idx_threads_user_id")
                    .col(Threads::UserId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the index
        manager
            .drop_index(
                Index::drop()
                    .table(Threads::Table)
                    .name("idx_threads_user_id")
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        match manager.get_database_backend() {
            DbBackend::Sqlite => {
                let stmt = Statement::from_string(
                    DbBackend::Sqlite,
                    r#"
PRAGMA foreign_keys=off;

BEGIN TRANSACTION;

ALTER TABLE threads RENAME TO _threads_old;

CREATE TABLE "threads" (
  "id" uuid_text NOT NULL PRIMARY KEY,
  "title" varchar NOT NULL,
  "input" varchar NOT NULL,
  "output" varchar NOT NULL,
  "source" varchar NOT NULL,
  "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "references" text NOT NULL,
  "source_type" varchar NOT NULL DEFAULT ''
);

INSERT INTO threads
    (
        id,
        title,
        input,
        output,
        source,
        created_at,
        'references',
        source_type
    )
    SELECT
        id,
        title,
        input,
        output,
        source,
        created_at,
        'references',
        source_type
    FROM _threads_old;

DROP TABLE _threads_old;

COMMIT;

PRAGMA foreign_keys=on;
"#,
                );
                manager.get_connection().execute(stmt).await?;
            }
            _ => {
                // Drop the foreign key
                manager
                    .drop_foreign_key(
                        ForeignKey::drop()
                            .table(Threads::Table)
                            .name("fk_threads_user_id")
                            .to_owned(),
                    )
                    .await?;

                // Drop the column
                manager
                    .alter_table(
                        Table::alter()
                            .table(Threads::Table)
                            .drop_column(Threads::UserId)
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    UserId,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
