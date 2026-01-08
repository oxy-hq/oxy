use sea_orm_migration::{
    prelude::*,
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

  CONSTRAINT fk_runs_lookup_id
    FOREIGN KEY (lookup_id)
    REFERENCES messages (id)
    ON DELETE SET NULL
    ON UPDATE NO ACTION
);

INSERT INTO runs SELECT * FROM _runs_old;

DROP TABLE _runs_old;

-- Fix checkpoints table foreign key to point to runs table
ALTER TABLE checkpoints RENAME TO _checkpoints_old;

CREATE TABLE "checkpoints" (
  "id" uuid_text NOT NULL,
  "run_id" uuid_text NOT NULL,
  "replay_id" varchar NOT NULL,
  "checkpoint_hash" varchar NOT NULL,
  "output" json_text NULL,
  "events" json_text NULL,
  "child_run_info" json_text NULL,
  "loop_values" json_text NULL,
  "created_at" timestamp_with_timezone_text NOT NULL,
  "updated_at" timestamp_with_timezone_text NOT NULL,
  FOREIGN KEY ("run_id") REFERENCES "runs" ("id") ON DELETE CASCADE
);

INSERT INTO checkpoints SELECT * FROM _checkpoints_old;

DROP TABLE _checkpoints_old;

CREATE UNIQUE INDEX "idx_unique_run_replay_checkpoint" ON "checkpoints" ("run_id", "replay_id");

COMMIT;

PRAGMA foreign_keys=on;

"#,
                );
                manager.get_connection().execute(stmt).await?;
            }
            _ => {}
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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

INSERT INTO runs SELECT * FROM _runs_old;

DROP TABLE _runs_old;

COMMIT;

PRAGMA foreign_keys=on;
"#,
                );
                manager.get_connection().execute(stmt).await?;
            }
            _ => {}
        }
        Ok(())
    }
}
