use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // test_runs — no FK on project_id (matches runs table pattern for local-dev zero-UUID)
        manager
            .create_table(
                Table::create()
                    .table(TestRuns::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TestRuns::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(TestRuns::SourceId).string().not_null())
                    .col(ColumnDef::new(TestRuns::RunIndex).integer().not_null())
                    .col(ColumnDef::new(TestRuns::ProjectId).uuid().not_null())
                    .col(ColumnDef::new(TestRuns::Name).string().null())
                    .col(ColumnDef::new(TestRuns::ProjectRunId).uuid().null())
                    .col(
                        ColumnDef::new(TestRuns::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TestRuns::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_test_runs_source_run_index")
                    .table(TestRuns::Table)
                    .col(TestRuns::SourceId)
                    .col(TestRuns::RunIndex)
                    .col(TestRuns::ProjectId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_test_runs_project_id")
                    .table(TestRuns::Table)
                    .col(TestRuns::ProjectId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_test_runs_project_run_id")
                    .table(TestRuns::Table)
                    .col(TestRuns::ProjectRunId)
                    .to_owned(),
            )
            .await?;

        // test_run_cases
        manager
            .create_table(
                Table::create()
                    .table(TestRunCases::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TestRunCases::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(TestRunCases::TestRunId).uuid().not_null())
                    .col(ColumnDef::new(TestRunCases::CaseIndex).integer().not_null())
                    .col(
                        ColumnDef::new(TestRunCases::Prompt)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(TestRunCases::Expected)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(ColumnDef::new(TestRunCases::ActualOutput).text().null())
                    .col(
                        ColumnDef::new(TestRunCases::Score)
                            .double()
                            .not_null()
                            .default(0.0),
                    )
                    .col(
                        ColumnDef::new(TestRunCases::Verdict)
                            .string()
                            .not_null()
                            .default("fail"),
                    )
                    .col(
                        ColumnDef::new(TestRunCases::PassingRuns)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(TestRunCases::TotalRuns)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(TestRunCases::AvgDurationMs).double().null())
                    .col(ColumnDef::new(TestRunCases::InputTokens).integer().null())
                    .col(ColumnDef::new(TestRunCases::OutputTokens).integer().null())
                    .col(ColumnDef::new(TestRunCases::JudgeReasoning).json().null())
                    .col(ColumnDef::new(TestRunCases::Errors).json().null())
                    .col(
                        ColumnDef::new(TestRunCases::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_test_run_cases_test_run_id")
                            .from(TestRunCases::Table, TestRunCases::TestRunId)
                            .to(TestRuns::Table, TestRuns::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_test_run_cases_run_id")
                    .table(TestRunCases::Table)
                    .col(TestRunCases::TestRunId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_test_run_cases_unique")
                    .table(TestRunCases::Table)
                    .col(TestRunCases::TestRunId)
                    .col(TestRunCases::CaseIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // test_project_runs
        manager
            .create_table(
                Table::create()
                    .table(TestProjectRuns::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TestProjectRuns::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(TestProjectRuns::ProjectId).uuid().not_null())
                    .col(ColumnDef::new(TestProjectRuns::Name).string().null())
                    .col(
                        ColumnDef::new(TestProjectRuns::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TestProjectRuns::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_test_project_runs_project_id")
                    .table(TestProjectRuns::Table)
                    .col(TestProjectRuns::ProjectId)
                    .to_owned(),
            )
            .await?;

        // test_case_human_verdicts (final schema with run_index + created_at)
        manager
            .create_table(
                Table::create()
                    .table(TestCaseHumanVerdicts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TestCaseHumanVerdicts::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(TestCaseHumanVerdicts::ProjectId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TestCaseHumanVerdicts::SourceId)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TestCaseHumanVerdicts::CaseIndex)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TestCaseHumanVerdicts::Verdict)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TestCaseHumanVerdicts::RunIndex)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(TestCaseHumanVerdicts::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(TestCaseHumanVerdicts::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_test_case_human_verdicts_unique")
                    .table(TestCaseHumanVerdicts::Table)
                    .col(TestCaseHumanVerdicts::ProjectId)
                    .col(TestCaseHumanVerdicts::SourceId)
                    .col(TestCaseHumanVerdicts::RunIndex)
                    .col(TestCaseHumanVerdicts::CaseIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // test_run_sequences — atomic counter replacing advisory-lock + MAX(run_index)
        manager
            .create_table(
                Table::create()
                    .table(TestRunSequences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TestRunSequences::ProjectId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TestRunSequences::SourceId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TestRunSequences::LastValue)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .primary_key(
                        Index::create()
                            .col(TestRunSequences::ProjectId)
                            .col(TestRunSequences::SourceId),
                    )
                    .to_owned(),
            )
            .await?;

        // Seed from existing test_runs so the counter starts above any run_index already in the table.
        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO test_run_sequences (project_id, source_id, last_value) \
                 SELECT project_id, source_id, MAX(run_index) \
                 FROM test_runs \
                 WHERE project_id IS NOT NULL \
                   AND source_id IS NOT NULL \
                   AND run_index IS NOT NULL \
                 GROUP BY project_id, source_id \
                 ON CONFLICT (project_id, source_id) \
                 DO UPDATE SET last_value = EXCLUDED.last_value",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(TestRunSequences::Table).to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_test_case_human_verdicts_unique")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(TestCaseHumanVerdicts::Table).to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_test_project_runs_project_id")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(TestProjectRuns::Table).to_owned())
            .await?;

        manager
            .drop_index(Index::drop().name("idx_test_run_cases_unique").to_owned())
            .await?;
        manager
            .drop_index(Index::drop().name("idx_test_run_cases_run_id").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(TestRunCases::Table).to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_test_runs_project_run_id")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(Index::drop().name("idx_test_runs_project_id").to_owned())
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_test_runs_source_run_index")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(TestRuns::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum TestRuns {
    Table,
    Id,
    SourceId,
    RunIndex,
    ProjectId,
    Name,
    ProjectRunId,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum TestRunCases {
    Table,
    Id,
    TestRunId,
    CaseIndex,
    Prompt,
    Expected,
    ActualOutput,
    Score,
    Verdict,
    PassingRuns,
    TotalRuns,
    AvgDurationMs,
    InputTokens,
    OutputTokens,
    JudgeReasoning,
    Errors,
    CreatedAt,
}

#[derive(Iden)]
enum TestProjectRuns {
    Table,
    Id,
    ProjectId,
    Name,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum TestCaseHumanVerdicts {
    Table,
    Id,
    ProjectId,
    SourceId,
    CaseIndex,
    Verdict,
    RunIndex,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum TestRunSequences {
    Table,
    ProjectId,
    SourceId,
    LastValue,
}
