use sea_orm_migration::prelude::*;

/// Per-source-kind airway admission config, with a sparse per-workspace
/// override. See `entity::airway_source_config` for the read-side shape and
/// `agentic_pipeline::airway_config::resolve_admission` (stage 2, follow-up
/// task) for how the global and workspace rows merge field by field.
///
/// A surrogate `id` primary key plus **two partial unique indexes**, not one
/// composite `UNIQUE (source_kind, workspace_id)`: Postgres treats `NULL`s as
/// distinct in a plain unique index, so the composite form would happily
/// admit two global rows for `toast` and make resolution non-deterministic —
/// the resolver would return whichever row the planner ordered first.
///
/// The surrogate key is **not** avoidable by promoting the natural key: a
/// `PRIMARY KEY` implies `NOT NULL` on every column, and `workspace_id IS NULL`
/// *is* the global row. `(source_kind, workspace_id)` therefore cannot be the
/// primary key without inventing a sentinel UUID for "global", which would put
/// a magic value in a foreign-keyed column and re-open exactly the
/// NULL-comparison trap the partial indexes close. Given a surrogate key is
/// forced, `SERIAL`/`i32` is a question of width only, on a table whose row
/// count is bounded by `source kinds × workspaces holding an override` — a few
/// hundred at the outer edge, six orders of magnitude below `i32`.
///
/// `workspace_id` FKs to `workspaces(id)` `ON DELETE CASCADE` (nullable,
/// still — `NULL` is the global row and has no referent). Cascade, not
/// restrict: a per-workspace override is meaningless once its workspace is
/// gone, so it should vanish with it rather than linger as a row nothing
/// would think to look for.
///
/// `updated_at` is maintained by a `BEFORE UPDATE` trigger rather than by the
/// writer. This is an audit surface — the admin UI reports when a policy last
/// changed — and "the writer remembered" is not a property a reader can check.
///
/// # Why `contract_policy` and `environment` carry no CHECK constraint
///
/// Both are free text, deliberately. The valid set is not defined here — it is
/// `airway::connector::{ContractPolicy, Environment}`, in the external engine,
/// and it moves: this workspace has bumped airway twenty-odd times. A CHECK
/// copies that set into SQL, where the copy is invisible from the crate that
/// owns it and goes stale on the next bump — and it goes stale in the worse
/// direction, rejecting a spelling the running code accepts, on an admin write
/// path, until someone ships a migration.
///
/// The usual counter-argument — "a bad value silently degrades to the default"
/// — does not hold here. `agentic_airway::AirwayAdmission::from_strings`
/// treats an unrecognised spelling as an **error**, never a fall-back
/// (`crates/agentic/airway/src/admission.rs`), precisely so a typo cannot turn
/// `require_declared` into `permissive` unnoticed. A value that arrives by raw
/// SQL therefore fails the run loudly, naming the column and the accepted set,
/// rather than quietly relaxing a policy. That is what a CHECK would have been
/// bought for, and it is already enforced where the vocabulary lives.
///
/// The `updated_at` trigger above is the same judgement in the other direction,
/// and the two are worth reading together: put in the schema what the schema
/// can own permanently (`now()` on update — defined by Postgres, cannot go
/// stale), and keep out of it what an upstream dependency defines and revises.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE IF NOT EXISTS airway_source_config (
                id SERIAL PRIMARY KEY,
                source_kind TEXT NOT NULL,
                workspace_id UUID REFERENCES workspaces(id) ON DELETE CASCADE,
                contract_policy TEXT,
                environment TEXT,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            -- Two PARTIAL unique indexes, not one composite. Postgres treats
            -- NULLs as distinct in a plain unique index, so `UNIQUE
            -- (source_kind, workspace_id)` would admit two global rows for the
            -- same kind and make resolution non-deterministic — the resolver
            -- would pick whichever the planner returned first.
            CREATE UNIQUE INDEX IF NOT EXISTS airway_source_config_global_uniq
                ON airway_source_config (source_kind) WHERE workspace_id IS NULL;
            CREATE UNIQUE INDEX IF NOT EXISTS airway_source_config_workspace_uniq
                ON airway_source_config (source_kind, workspace_id) WHERE workspace_id IS NOT NULL;

            -- `updated_at` advances on every UPDATE, including the UPDATE half
            -- of an `INSERT ... ON CONFLICT DO UPDATE`. In the database rather
            -- than in the writer because this is an audit surface: the admin UI
            -- reports when a policy last changed, and a writer that forgets to
            -- set the column makes that report lie in the one direction nobody
            -- checks (too old, never too new). A trigger also covers the writers
            -- an ORM hook cannot — psql, a migration, a future service.
            CREATE OR REPLACE FUNCTION airway_source_config_touch_updated_at()
            RETURNS TRIGGER AS $$
            BEGIN
                NEW.updated_at := now();
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;

            DROP TRIGGER IF EXISTS airway_source_config_set_updated_at ON airway_source_config;
            CREATE TRIGGER airway_source_config_set_updated_at
                BEFORE UPDATE ON airway_source_config
                FOR EACH ROW EXECUTE FUNCTION airway_source_config_touch_updated_at();
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            // `DROP TABLE CASCADE` takes the trigger but leaves the function
            // behind — it is a schema-level object, not a table-level one.
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS airway_source_config CASCADE;
            DROP FUNCTION IF EXISTS airway_source_config_touch_updated_at();
        "#,
            )
            .await?;
        Ok(())
    }
}
