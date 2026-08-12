use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// The `agentic_runs` create-table statement.
///
/// **Twin: `agentic_runs_table()` in `crates/agentic/runtime`.** Both
/// crates create this table with `if_not_exists`, so a column added here and
/// not there (or vice versa) does not fail — whichever migrator runs first
/// wins and the other no-ops, and the divergence shows up as a missing column
/// at runtime. Edit both. They deliberately render *different* identifiers:
/// singular here (bare `DeriveIden`), renamed to plural by `m20260317_000002`.
///
/// Extracted so the tests below assert the DDL this migration actually emits
/// rather than a copy that can drift away from it.
fn agentic_runs_table() -> TableCreateStatement {
    Table::create()
        .table(AgenticRun::Table)
        .if_not_exists()
        .col(
            ColumnDef::new(AgenticRun::Id)
                .string()
                .not_null()
                .primary_key(),
        )
        .col(ColumnDef::new(AgenticRun::AgentId).string().not_null())
        .col(ColumnDef::new(AgenticRun::Question).text().not_null())
        .col(
            ColumnDef::new(AgenticRun::Status)
                .string()
                .not_null()
                .default("running"),
        )
        .col(ColumnDef::new(AgenticRun::Answer).text().null())
        .col(ColumnDef::new(AgenticRun::ErrorMessage).text().null())
        .col(
            ColumnDef::new(AgenticRun::CreatedAt)
                .timestamp_with_time_zone()
                .not_null(),
        )
        .col(
            ColumnDef::new(AgenticRun::UpdatedAt)
                .timestamp_with_time_zone()
                .not_null(),
        )
        .to_owned()
}

/// The `agentic_run_suspensions` create-table statement.
///
/// **Twin: `agentic_run_suspensions_table()` in `crates/agentic/runtime`** —
/// same `if_not_exists` hazard and same deliberate identifier difference as
/// `agentic_runs_table()` above. Edit both.
fn agentic_run_suspensions_table() -> TableCreateStatement {
    Table::create()
        .table(AgenticRunSuspension::Table)
        .if_not_exists()
        .col(
            ColumnDef::new(AgenticRunSuspension::RunId)
                .string()
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(AgenticRunSuspension::Prompt)
                .text()
                .not_null(),
        )
        .col(
            ColumnDef::new(AgenticRunSuspension::Suggestions)
                .json_binary()
                .not_null(),
        )
        .col(
            ColumnDef::new(AgenticRunSuspension::ResumeData)
                .json_binary()
                .not_null(),
        )
        .col(
            ColumnDef::new(AgenticRunSuspension::CreatedAt)
                .timestamp_with_time_zone()
                .not_null(),
        )
        .foreign_key(
            ForeignKey::create()
                .from(AgenticRunSuspension::Table, AgenticRunSuspension::RunId)
                .to(AgenticRun::Table, AgenticRun::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

/// The unique index on `agentic_run_events (run_id, seq)`.
///
/// **Twin: `agentic_run_events_index()` in `crates/agentic/runtime`**, and a
/// sharper hazard than the tables: the index *name* is identical on both sides,
/// so `if_not_exists` no-ops by name. Change the columns or drop `.unique()` on
/// one side and the other silently does nothing — no error, just an index that
/// isn't the one you wrote.
///
/// NOT dead, despite central now running first everywhere. Several test
/// binaries each migrate a *fresh* shared database concurrently (nextest gives
/// no inter-binary ordering and the helper holds no lock), so two central runs
/// race here and the loser gets 42P07. Removing this was tried and reverted:
/// it fails ~174 tests across the three agentic packages, all on this one
/// index — which is a reason to pin it, not to leave it unguarded.
fn agentic_run_events_index() -> IndexCreateStatement {
    Index::create()
        .if_not_exists()
        .name("idx_agentic_run_events_run_id_seq")
        .table(AgenticRunEvent::Table)
        .col(AgenticRunEvent::RunId)
        .col(AgenticRunEvent::Seq)
        .unique()
        .to_owned()
}

/// The `agentic_run_events` create-table statement.
///
/// **Twin: `agentic_run_events_table()` in `crates/agentic/runtime`** — same
/// `if_not_exists` hazard and same deliberate identifier difference as
/// `agentic_runs_table()` above. Edit both.
///
/// Extracted so the test below can assert on the DDL this migration actually
/// emits. A test that rebuilds an equivalent `ColumnDef` proves only that the
/// copy in the test is right — `up()` could regress to `.auto_increment()` and
/// stay green, which is the exact drift the explicit `bigserial` prevents.
fn agentic_run_events_table() -> TableCreateStatement {
    Table::create()
        .table(AgenticRunEvent::Table)
        .if_not_exists()
        .col(
            // `bigserial` spelled out rather than
            // `.big_integer().auto_increment()`: under SeaORM 2.0 that pair
            // renders `GENERATED BY DEFAULT AS IDENTITY` unless the non-default
            // `postgres-use-serial-pk` feature is on, so the column's type used
            // to depend on a cargo feature two files away. Deployed databases
            // have `bigserial`; saying so here makes a fresh migration produce
            // it either way.
            ColumnDef::new(AgenticRunEvent::Id)
                .custom(Alias::new("bigserial"))
                .not_null()
                .primary_key(),
        )
        .col(ColumnDef::new(AgenticRunEvent::RunId).string().not_null())
        .col(
            ColumnDef::new(AgenticRunEvent::Seq)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(AgenticRunEvent::EventType)
                .string()
                .not_null(),
        )
        .col(
            ColumnDef::new(AgenticRunEvent::Payload)
                .json_binary()
                .not_null(),
        )
        .col(
            ColumnDef::new(AgenticRunEvent::CreatedAt)
                .timestamp_with_time_zone()
                .not_null(),
        )
        .foreign_key(
            ForeignKey::create()
                .from(AgenticRunEvent::Table, AgenticRunEvent::RunId)
                .to(AgenticRun::Table, AgenticRun::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── agentic_runs ──────────────────────────────────────────────────────
        manager.create_table(agentic_runs_table()).await?;

        // ── agentic_run_events ────────────────────────────────────────────────
        manager.create_table(agentic_run_events_table()).await?;

        manager.create_index(agentic_run_events_index()).await?;

        // ── agentic_run_suspensions ───────────────────────────────────────────
        manager
            .create_table(agentic_run_suspensions_table())
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AgenticRunSuspension::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(AgenticRunEvent::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(AgenticRun::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum AgenticRun {
    Table,
    Id,
    AgentId,
    Question,
    Status,
    Answer,
    ErrorMessage,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AgenticRunEvent {
    Table,
    Id,
    RunId,
    Seq,
    EventType,
    Payload,
    CreatedAt,
}

#[derive(DeriveIden)]
enum AgenticRunSuspension {
    Table,
    RunId,
    Prompt,
    Suggestions,
    ResumeData,
    CreatedAt,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::sea_query::PostgresQueryBuilder;

    /// This table's PK must render `bigserial`, without depending on the
    /// `postgres-use-serial-pk` cargo feature.
    ///
    /// Background: SeaORM 2.0 changed what `.auto_increment()` emits for Postgres
    /// — `GENERATED BY DEFAULT AS IDENTITY` rather than `serial` — and the feature
    /// existed to hold the old rendering. Deployed databases were migrated under
    /// the old behaviour, so a fresh run under the new one comes up with a
    /// different schema; the kind of drift nobody notices until a restore.
    ///
    /// Depending on the feature made a column's type a property of the dependency
    /// graph instead of the migration. Three sites now name `serial`/`bigserial`
    /// directly, which is why the feature could be dropped — but that trades away
    /// the old guard's "one test covers every call site" property, so **each site
    /// is asserted where it lives**:
    ///
    /// | Table | Assertion |
    /// | ----- | --------- |
    /// | `agentic_run_event` (here) | this test |
    /// | `settings` | `settings_id_renders_serial`, `m20250618_102934_create_github_config_table.rs` |
    /// | `agentic_run_events` | `agentic_run_events_id_renders_bigserial`, `crates/agentic/runtime/src/migration.rs` |
    ///
    /// This renders `up()`'s own statement — not a copy of it — so it fails if
    /// the call site regresses to `.auto_increment()` / `pk_auto`. That matters
    /// more now than before: with the feature gone, the wrong spelling still
    /// compiles and merely emits the wrong DDL, so nothing else would object.
    #[test]
    fn migration_ddl_is_not_feature_dependent() {
        let ddl = agentic_run_events_table().to_string(PostgresQueryBuilder);

        // Full-statement equality, not `contains`: this statement and
        // agentic-runtime's are near-copies of the same table, and pinning the
        // whole rendering makes a column added to one and not the other show up
        // as a failure here rather than as a missing column at runtime (both are
        // `IF NOT EXISTS`, so whichever migrator runs first wins and the second
        // silently no-ops — in practice this one, since central runs first).
        //
        // NOTE: the two are deliberately NOT asserted equal to each other —
        // they render different identifiers. The singular `agentic_run_event` /
        // `agentic_run` below are **transient**: this migrator derives them from
        // its bare `DeriveIden` enum, and `m20260317_000002` renames all three to
        // the plural forms immediately after. A fresh database therefore ends up
        // with the plural tables the entity layer reads — the singular spelling
        // never outlives these two consecutive migrations.
        assert_eq!(
            ddl,
            r#"CREATE TABLE IF NOT EXISTS "agentic_run_event" ( "id" bigserial NOT NULL PRIMARY KEY, "run_id" varchar NOT NULL, "seq" bigint NOT NULL, "event_type" varchar NOT NULL, "payload" jsonb NOT NULL, "created_at" timestamp with time zone NOT NULL, FOREIGN KEY ("run_id") REFERENCES "agentic_run" ("id") ON DELETE CASCADE )"#,
            "agentic_run_event DDL changed. Causes, in rough order of likelihood: \
             a column edit here (update this literal AND check whether \
             agentic-runtime's twin needs the same edit); or a sea-query \
             rendering change upstream (whitespace/quoting), in which case only \
             this literal needs refreshing."
        );
        assert!(
            !ddl.to_uppercase().contains("IDENTITY"),
            "identity columns diverge from every deployed database. Rendered: {ddl}"
        );

        // `.auto_increment()` is the spelling this replaced: assert it really is
        // feature-dependent now, so the assertions above are guarding something
        // rather than passing for free.
        let legacy = Table::create()
            .table(AgenticRunEvent::Table)
            .col(
                ColumnDef::new(AgenticRunEvent::Id)
                    .big_integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .to_string(PostgresQueryBuilder);
        assert!(
            legacy.to_uppercase().contains("IDENTITY"),
            "`.auto_increment()` no longer renders IDENTITY — either \
             `postgres-use-serial-pk` is back (directly or pulled in transitively \
             by a dependency), or an upstream sea-query default changed. Either \
             way the explicit `bigserial` spellings have stopped being \
             load-bearing and this guard proves less than it claims. \
             Rendered: {legacy}"
        );
    }
    /// The other two tables this migrator shares with `crates/agentic/runtime`.
    ///
    /// Same hazard as `agentic_run_events`: both crates create them with
    /// `IF NOT EXISTS`, so a column added on one side only does not fail — the
    /// second create no-ops and the column is simply missing at runtime.
    ///
    /// These pin the **create** shape only. `agentic-runtime` later adds
    /// `parent_run_id` / `task_status` / `task_metadata` to `agentic_runs` via
    /// `column_exists`-guarded ALTERs; that is deliberate, not divergence, and
    /// does not belong in these literals.
    #[test]
    fn sibling_twin_tables_render_their_pinned_ddl() {
        assert_eq!(
            agentic_runs_table().to_string(PostgresQueryBuilder),
            r#"CREATE TABLE IF NOT EXISTS "agentic_run" ( "id" varchar NOT NULL PRIMARY KEY, "agent_id" varchar NOT NULL, "question" text NOT NULL, "status" varchar NOT NULL DEFAULT 'running', "answer" text NULL, "error_message" text NULL, "created_at" timestamp with time zone NOT NULL, "updated_at" timestamp with time zone NOT NULL )"#,
            "agentic_run DDL changed. Causes: a column edit here (update this \
             literal AND check agentic-runtime's twin); or an upstream sea-query \
             rendering change, in which case only this literal needs refreshing."
        );

        assert_eq!(
            agentic_run_suspensions_table().to_string(PostgresQueryBuilder),
            r#"CREATE TABLE IF NOT EXISTS "agentic_run_suspension" ( "run_id" varchar NOT NULL PRIMARY KEY, "prompt" text NOT NULL, "suggestions" jsonb NOT NULL, "resume_data" jsonb NOT NULL, "created_at" timestamp with time zone NOT NULL, FOREIGN KEY ("run_id") REFERENCES "agentic_run" ("id") ON DELETE CASCADE )"#,
            "agentic_run_suspension DDL changed. Causes: a column edit here \
             (update this literal AND check agentic-runtime's twin); or an \
             upstream sea-query rendering change."
        );
    }
    /// The shared unique index, pinned like the tables — and it is the sharper
    /// case: both migrators create `idx_agentic_run_events_run_id_seq` under
    /// the *same name*, so `IF NOT EXISTS` no-ops by name. Change the columns
    /// or drop `.unique()` on one side and the other silently does nothing.
    #[test]
    fn the_shared_index_renders_its_pinned_ddl() {
        assert_eq!(
            agentic_run_events_index().to_string(PostgresQueryBuilder),
            r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_agentic_run_events_run_id_seq" ON "agentic_run_event" ("run_id", "seq")"#,
            "the shared index changed. Causes: an edit here (update this literal \
             AND check crates/agentic/runtime's twin, which creates the same index name); or an \
             upstream sea-query rendering change."
        );
    }
}
