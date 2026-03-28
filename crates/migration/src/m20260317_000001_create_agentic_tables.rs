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
