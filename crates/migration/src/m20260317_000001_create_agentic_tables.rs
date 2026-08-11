use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── agentic_runs ──────────────────────────────────────────────────────
        manager
            .create_table(
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
                    .to_owned(),
            )
            .await?;

        // ── agentic_run_events ────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(AgenticRunEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AgenticRunEvent::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
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
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    // NOT dead, despite central now running first everywhere.
                    // Several test binaries each migrate a *fresh* shared
                    // database concurrently (nextest gives no inter-binary
                    // ordering and the helper holds no lock), so two central
                    // runs race here and the loser gets 42P07. Removing this
                    // was tried and reverted: it fails ~174 tests across the
                    // three agentic packages, all on this one index.
                    .if_not_exists()
                    .name("idx_agentic_run_events_run_id_seq")
                    .table(AgenticRunEvent::Table)
                    .col(AgenticRunEvent::RunId)
                    .col(AgenticRunEvent::Seq)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── agentic_run_suspensions ───────────────────────────────────────────
        manager
            .create_table(
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
                    .to_owned(),
            )
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

    /// Guards the `postgres-use-serial-pk` feature on the workspace `sea-orm`
    /// dependency (see the comment on it in the root `Cargo.toml`).
    ///
    /// SeaORM 2.0 changed what `.auto_increment()` emits for Postgres: without
    /// that feature it renders `GENERATED BY DEFAULT AS IDENTITY` instead of
    /// `bigserial`. Deployed databases were migrated under the old behaviour,
    /// so a fresh one would silently come up with a different schema — the kind
    /// of drift nobody notices until a restore. If a future dependency bump
    /// drops the feature, this fails instead.
    ///
    /// Scope: this reconstructs a column rather than rendering the migration's
    /// own statement, so it guards the *feature*, not this table's shape. That
    /// covers every `.auto_increment()` in the workspace — the one below and
    /// the one in `agentic-runtime`'s migrator — because they all inherit the
    /// same `sea-orm = { workspace = true }`. A new call site needs no test of
    /// its own; removing the feature fails here for all of them at once.
    #[test]
    fn auto_increment_still_renders_serial_on_postgres() {
        let ddl = Table::create()
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
            ddl.contains("bigserial"),
            "`.auto_increment()` must still emit `bigserial`, not identity \
             columns — has the `postgres-use-serial-pk` feature been dropped \
             from the workspace `sea-orm` dependency? Rendered: {ddl}"
        );
    }
}
