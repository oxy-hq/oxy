//! Creates the two Oxy Functions tables (design doc §11.12, §11.14):
//!
//! - `app_functions` — one row per (build, function) for a function shipped in
//!   a customer-app bundle's `functions/` dir. Versions with its `app_builds`
//!   row (cascade-deleted) and points at the bundled JS artifact key.
//! - `app_function_invocations` — one row per invocation (route / schedule /
//!   airway), backing cancellation, rate limiting, the admin audit trail, and
//!   idempotent replay of side-effectful route calls.
//!
//! Squashed: this single migration supersedes the original
//! `create_app_functions` + `create_app_function_invocations` +
//! `app_function_invocations_idempotency` migrations (all unmerged).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum AppFunctions {
    Table,
    Id,
    AppId,
    BuildId,
    Name,
    ManifestJson,
    ArtifactKey,
    CreatedAt,
}

#[derive(DeriveIden)]
enum AppFunctionInvocations {
    Table,
    Id,
    AppId,
    BuildId,
    FunctionName,
    Mode,
    UserId,
    Status,
    DurationMs,
    Error,
    CancelRequestedAt,
    CreatedAt,
    IdempotencyKey,
    ResultBody,
    RequestHash,
}

#[derive(DeriveIden)]
enum AppBuilds {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── app_functions ──────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(AppFunctions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AppFunctions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AppFunctions::AppId).uuid().not_null())
                    .col(ColumnDef::new(AppFunctions::BuildId).uuid().not_null())
                    .col(ColumnDef::new(AppFunctions::Name).text().not_null())
                    .col(ColumnDef::new(AppFunctions::ManifestJson).json_binary())
                    .col(ColumnDef::new(AppFunctions::ArtifactKey).text().not_null())
                    .col(
                        ColumnDef::new(AppFunctions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_functions_build_id")
                            .from(AppFunctions::Table, AppFunctions::BuildId)
                            .to(AppBuilds::Table, AppBuilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_app_functions_build_id")
                    .table(AppFunctions::Table)
                    .col(AppFunctions::BuildId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_app_functions_build_name")
                    .table(AppFunctions::Table)
                    .col(AppFunctions::BuildId)
                    .col(AppFunctions::Name)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── app_function_invocations ───────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(AppFunctionInvocations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AppFunctionInvocations::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AppFunctionInvocations::AppId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AppFunctionInvocations::BuildId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AppFunctionInvocations::FunctionName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AppFunctionInvocations::Mode)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AppFunctionInvocations::UserId).uuid())
                    .col(
                        ColumnDef::new(AppFunctionInvocations::Status)
                            .text()
                            .not_null()
                            .default("running"),
                    )
                    .col(ColumnDef::new(AppFunctionInvocations::DurationMs).big_integer())
                    .col(ColumnDef::new(AppFunctionInvocations::Error).text())
                    .col(
                        ColumnDef::new(AppFunctionInvocations::CancelRequestedAt)
                            .timestamp_with_time_zone(),
                    )
                    .col(
                        ColumnDef::new(AppFunctionInvocations::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(AppFunctionInvocations::IdempotencyKey).text())
                    .col(ColumnDef::new(AppFunctionInvocations::ResultBody).text())
                    .col(ColumnDef::new(AppFunctionInvocations::RequestHash).big_integer())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_function_invocations_build_id")
                            .from(
                                AppFunctionInvocations::Table,
                                AppFunctionInvocations::BuildId,
                            )
                            .to(AppBuilds::Table, AppBuilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_app_function_invocations_app_fn")
                    .table(AppFunctionInvocations::Table)
                    .col(AppFunctionInvocations::AppId)
                    .col(AppFunctionInvocations::FunctionName)
                    .to_owned(),
            )
            .await?;
        // Exactly-once for a retried side-effectful invocation, scoped to
        // (app, function, user) — build-independent so it survives a redeploy.
        // Keyless rows are all NULL and thus distinct (no conflict).
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_app_function_invocations_idempotency")
                    .table(AppFunctionInvocations::Table)
                    .col(AppFunctionInvocations::AppId)
                    .col(AppFunctionInvocations::FunctionName)
                    .col(AppFunctionInvocations::UserId)
                    .col(AppFunctionInvocations::IdempotencyKey)
                    .unique()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AppFunctionInvocations::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(AppFunctions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
