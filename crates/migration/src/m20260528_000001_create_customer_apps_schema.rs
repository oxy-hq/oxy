//! Consolidated customer-apps schema migration.
//!
//! Squashes the in-flight history of 14 incremental migrations on the
//! `new-auth` branch (`m20260512_…` through `m20260527_…`) into one
//! file that produces the final state directly. None of the squashed
//! migrations were ever applied to a production database — they only
//! existed on this feature branch — so a clean consolidation is safe.
//!
//! What this creates:
//!
//! - **`apps`** — customer-app registry row. Per-org bundle, links to
//!   project + branch + source storage (v0 / local / s3). Slug is
//!   unique within an org. Carries the bundle-level optional manifest
//!   override, the scaffold-PR URL, the last-sync timestamp, and the
//!   draft/published toggle (published_at).
//!
//! - **`app_admins`** — global "Oxy staff" role. Email-keyed so the
//!   row can pre-exist a user (matches magic-link onboarding).
//!   Members can access the customer-apps admin surface + any
//!   registered app regardless of org membership.
//!
//! - **`workspace_oxy_access`** — per-workspace opt-in flag. When a
//!   row exists, Oxy admins (from `app_admins`) can build customer
//!   apps on that workspace's data. Combined check: admin AND
//!   workspace opted in.
//!
//! - **`customer_app_procedure_runs`** — persistent state for
//!   procedure runs kicked off from bundles via `useProcedureRun`.
//!   Separate from `agentic_runs` because procedures don't go through
//!   the analytics/builder FSM (different progress + output schema).
//!   Lifecycle: handler writes a `running` row, spawned task updates
//!   on completion, periodic sweep evicts terminal rows after 24h.
//!   FK to workspaces with cascade so workspace deletion cleans up.
//!
//! All `create_table` + `create_index` calls use `if_not_exists()` so
//! the migration is idempotent against dev databases that already
//! applied any of the historical 14 (their `seaql_migrations` rows
//! become orphan history but cause no failure — SeaORM only acts on
//! pending names).

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
    Slug,
    Name,
    OrgId,
    ProjectId,
    Branch,
    SourceRepo,
    Status,
    SourceType,
    SourceConfig,
    LastSyncedAt,
    ManifestOverride,
    BootstrapPrUrl,
    PublishedAt,
    RepoPath,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AppAdmins {
    Table,
    Id,
    Email,
    GrantedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum WorkspaceOxyAccess {
    Table,
    Id,
    WorkspaceId,
    GrantedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum CustomerAppProcedureRuns {
    Table,
    Id,
    WorkspaceId,
    ProcedureId,
    Status,
    Params,
    ProgressStep,
    ProgressPercent,
    ResultSummary,
    ResultOutputs,
    ErrorMessage,
    ErrorCode,
    CancelRequestedAt,
    StartedAt,
    CompletedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Workspaces {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── apps ──────────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(Apps::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Apps::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Apps::Slug).text().not_null())
                    .col(ColumnDef::new(Apps::Name).text().not_null())
                    .col(ColumnDef::new(Apps::OrgId).uuid().not_null())
                    .col(ColumnDef::new(Apps::ProjectId).uuid().not_null())
                    .col(ColumnDef::new(Apps::Branch).text().not_null())
                    .col(ColumnDef::new(Apps::SourceRepo).text().not_null())
                    .col(ColumnDef::new(Apps::Status).text().not_null())
                    .col(
                        ColumnDef::new(Apps::SourceType)
                            .text()
                            .not_null()
                            .default("s3"),
                    )
                    .col(ColumnDef::new(Apps::SourceConfig).json_binary().not_null())
                    .col(ColumnDef::new(Apps::LastSyncedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Apps::ManifestOverride).json_binary())
                    .col(ColumnDef::new(Apps::BootstrapPrUrl).text())
                    .col(ColumnDef::new(Apps::PublishedAt).timestamp_with_time_zone())
                    // Stable bundle identifier — the path under the
                    // customer-apps git repo (`<repo-org>/<repo-slug>`)
                    // that this row's bundle lives at. Decouples the
                    // S3 key from the admin row's slug so the same
                    // bundle has the same S3 path across dev/prod
                    // even if operators renamed the admin row.
                    // Nullable for back-compat; sync code falls back
                    // to `<org_slug>/<app_slug>` when absent.
                    .col(ColumnDef::new(Apps::RepoPath).text())
                    .col(
                        ColumnDef::new(Apps::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Apps::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_apps_org_id")
                            .from(Apps::Table, Apps::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_apps_org_id")
                    .table(Apps::Table)
                    .col(Apps::OrgId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_apps_org_slug")
                    .table(Apps::Table)
                    .col(Apps::OrgId)
                    .col(Apps::Slug)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── app_admins ────────────────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(AppAdmins::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AppAdmins::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AppAdmins::Email).text().not_null())
                    .col(ColumnDef::new(AppAdmins::GrantedBy).uuid())
                    .col(
                        ColumnDef::new(AppAdmins::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_admins_granted_by")
                            .from(AppAdmins::Table, AppAdmins::GrantedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_app_admins_email")
                    .table(AppAdmins::Table)
                    .col(AppAdmins::Email)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── workspace_oxy_access ──────────────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceOxyAccess::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkspaceOxyAccess::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceOxyAccess::WorkspaceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkspaceOxyAccess::GrantedBy).uuid())
                    .col(
                        ColumnDef::new(WorkspaceOxyAccess::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_oxy_access_workspace_id")
                            .from(WorkspaceOxyAccess::Table, WorkspaceOxyAccess::WorkspaceId)
                            .to(Workspaces::Table, Workspaces::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_oxy_access_granted_by")
                            .from(WorkspaceOxyAccess::Table, WorkspaceOxyAccess::GrantedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_workspace_oxy_access_workspace")
                    .table(WorkspaceOxyAccess::Table)
                    .col(WorkspaceOxyAccess::WorkspaceId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // ── customer_app_procedure_runs ───────────────────────────────────
        manager
            .create_table(
                Table::create()
                    .table(CustomerAppProcedureRuns::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CustomerAppProcedureRuns::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CustomerAppProcedureRuns::WorkspaceId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CustomerAppProcedureRuns::ProcedureId)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CustomerAppProcedureRuns::Status)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(CustomerAppProcedureRuns::Params).json_binary())
                    .col(ColumnDef::new(CustomerAppProcedureRuns::ProgressStep).text())
                    .col(ColumnDef::new(CustomerAppProcedureRuns::ProgressPercent).small_integer())
                    .col(ColumnDef::new(CustomerAppProcedureRuns::ResultSummary).text())
                    .col(ColumnDef::new(CustomerAppProcedureRuns::ResultOutputs).json_binary())
                    .col(ColumnDef::new(CustomerAppProcedureRuns::ErrorMessage).text())
                    .col(ColumnDef::new(CustomerAppProcedureRuns::ErrorCode).text())
                    .col(
                        ColumnDef::new(CustomerAppProcedureRuns::CancelRequestedAt)
                            .timestamp_with_time_zone(),
                    )
                    .col(
                        ColumnDef::new(CustomerAppProcedureRuns::StartedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(CustomerAppProcedureRuns::CompletedAt)
                            .timestamp_with_time_zone(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_cust_app_proc_runs_workspace")
                            .from(
                                CustomerAppProcedureRuns::Table,
                                CustomerAppProcedureRuns::WorkspaceId,
                            )
                            .to(Workspaces::Table, Workspaces::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_cust_app_proc_runs_workspace")
                    .table(CustomerAppProcedureRuns::Table)
                    .col(CustomerAppProcedureRuns::WorkspaceId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_cust_app_proc_runs_completed")
                    .table(CustomerAppProcedureRuns::Table)
                    .col(CustomerAppProcedureRuns::CompletedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(CustomerAppProcedureRuns::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkspaceOxyAccess::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(AppAdmins::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Apps::Table).if_exists().to_owned())
            .await?;
        Ok(())
    }
}
