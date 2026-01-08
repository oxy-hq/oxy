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

ALTER TABLE runs RENAME TO _runs_old;

CREATE TABLE "runs" (
  "id" uuid_text NOT NULL PRIMARY KEY,
  "source_id" varchar NOT NULL,
  "run_index" integer NULL,
  "root_source_id" varchar NULL,
  "root_run_index" integer NULL,
  "root_replay_ref" varchar NULL,
  "metadata" text NULL,
  "children" text NULL,
  "blocks" text NULL,
  "variables" text NULL,
  "output" text NULL,
  "error" varchar NULL,
  "project_id" uuid_text NOT NULL,
  "branch_id" uuid_text NOT NULL,
  "lookup_id" uuid_text NULL,
  "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updated_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "user_id" uuid_text NULL,

  CONSTRAINT fk_runs_user_id
    FOREIGN KEY (user_id)
    REFERENCES users (id)
    ON DELETE CASCADE
    ON UPDATE NO ACTION,

  CONSTRAINT fk_runs_project_id
    FOREIGN KEY (project_id)
    REFERENCES projects (id)
    ON DELETE CASCADE
    ON UPDATE NO ACTION,

  CONSTRAINT fk_runs_lookup_id
    FOREIGN KEY (lookup_id)
    REFERENCES messages (id)
    ON DELETE SET NULL
    ON UPDATE NO ACTION
);

INSERT INTO runs
    (
        id,
        source_id,
        run_index,
        root_source_id,
        root_run_index,
        root_replay_ref,
        metadata,
        children,
        blocks,
        variables,
        output,
        error,
        project_id,
        branch_id,
        lookup_id,
        created_at,
        updated_at,
        user_id
    )
    SELECT
        id,
        source_id,
        run_index,
        root_source_id,
        root_run_index,
        root_replay_ref,
        metadata,
        children,
        blocks,
        variables,
        output,
        error,
        project_id,
        branch_id,
        lookup_id,
        created_at,
        updated_at,
        NULL
    FROM _runs_old;

DROP TABLE _runs_old;

COMMIT;

PRAGMA foreign_keys=on;

"#,
                );
                manager.get_connection().execute(stmt).await?;
            }
            _ => {
                // Add user_id column to runs table (nullable for existing records)
                manager
                    .alter_table(
                        Table::alter()
                            .table(Runs::Table)
                            .add_column(uuid_null(Runs::UserId))
                            .to_owned(),
                    )
                    .await?;

                // Add foreign key constraint
                manager
                    .create_foreign_key(
                        ForeignKey::create()
                            .name("fk_runs_user_id")
                            .from(Runs::Table, Runs::UserId)
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
                    .table(Runs::Table)
                    .name("idx_runs_user_id")
                    .col(Runs::UserId)
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
                    .table(Runs::Table)
                    .name("idx_runs_user_id")
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

ALTER TABLE runs RENAME TO _runs_old;

CREATE TABLE "runs" (
  "id" uuid_text NOT NULL PRIMARY KEY,
  "source_id" varchar NOT NULL,
  "run_index" integer NULL,
  "root_source_id" varchar NULL,
  "root_run_index" integer NULL,
  "root_replay_ref" varchar NULL,
  "metadata" text NULL,
  "children" text NULL,
  "blocks" text NULL,
  "variables" text NULL,
  "output" text NULL,
  "error" varchar NULL,
  "project_id" uuid_text NOT NULL,
  "branch_id" uuid_text NOT NULL,
  "lookup_id" uuid_text NULL,
  "created_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP,
  "updated_at" timestamp_with_timezone_text NOT NULL DEFAULT CURRENT_TIMESTAMP,

  CONSTRAINT fk_runs_project_id
    FOREIGN KEY (project_id)
    REFERENCES projects (id)
    ON DELETE CASCADE
    ON UPDATE NO ACTION,

  CONSTRAINT fk_runs_lookup_id
    FOREIGN KEY (lookup_id)
    REFERENCES messages (id)
    ON DELETE SET NULL
    ON UPDATE NO ACTION
);

INSERT INTO runs
    (
        id,
        source_id,
        run_index,
        root_source_id,
        root_run_index,
        root_replay_ref,
        metadata,
        children,
        blocks,
        variables,
        output,
        error,
        project_id,
        branch_id,
        lookup_id,
        created_at,
        updated_at
    )
    SELECT
        id,
        source_id,
        run_index,
        root_source_id,
        root_run_index,
        root_replay_ref,
        metadata,
        children,
        blocks,
        variables,
        output,
        error,
        project_id,
        branch_id,
        lookup_id,
        created_at,
        updated_at
    FROM _runs_old;

DROP TABLE _runs_old;

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
                            .table(Runs::Table)
                            .name("fk_runs_user_id")
                            .to_owned(),
                    )
                    .await?;

                // Drop the column
                manager
                    .alter_table(
                        Table::alter()
                            .table(Runs::Table)
                            .drop_column(Runs::UserId)
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Runs {
    Table,
    UserId,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
