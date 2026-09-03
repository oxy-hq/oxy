use sea_orm_migration::prelude::*;

/// Store the HTTP status a function returned, so a cached replay can report it.
///
/// # Why the column has to exist
///
/// A custom-app function's status never reached its caller: `run_with_runtime`
/// dropped `resp.status` and the SSE framing hardcoded `{"status": 200}`. So a
/// function returning 400, 403 or 409 arrived as an ordinary success, and every
/// client had to infer rejection from the body's shape.
///
/// Emitting the real status on the fresh-run path alone would have been worse
/// than leaving it: an idempotent retry replays from `result_body`, so the
/// first call would report 409 and the replay 200 for the same invocation. The
/// status has to be stored beside the body it belongs to, or the two disagree.
///
/// Nullable with no backfill. Rows written before this migration genuinely do
/// not know what they returned, and stamping them 200 would assert something
/// nobody recorded — the reader treats null as 200, which is what those rows
/// were already being reported as.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AppFunctionInvocations::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(AppFunctionInvocations::ResultStatus).small_integer(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AppFunctionInvocations::Table)
                    .drop_column(AppFunctionInvocations::ResultStatus)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum AppFunctionInvocations {
    Table,
    ResultStatus,
}
