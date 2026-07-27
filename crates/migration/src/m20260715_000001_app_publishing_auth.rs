use sea_orm_migration::prelude::*;

/// App publishing authorization (design:
/// `internal-docs/partner-platform.md`).
///
/// Three schema changes, all additive so the existing staff CI publish flow keeps
/// working:
///
/// 1. `partner_publish_consent` — the CLIENT's opt-in switch. Default OFF (no row =
///    denied). A partner with `manage_apps` + an assigned client still cannot
///    publish into that client until the client's own officer turns it on. This is
///    the DAP→GDAP correction: no unconsented third-party write into a tenant.
///
/// 2. `app_publishers` — trusted-publishing config, **APP-scoped** (one app, not a
///    whole org). A GitHub Actions OIDC token that matches a row's claims is
///    exchanged for a short-lived credential caveated to that app.
///
/// 3. `app_publish_tokens` gains `app_id` (NULL = legacy staff-wide token; set =
///    app-scoped fallback token) and `expires_at` (NULL = legacy non-expiring; set
///    = required for partner-minted tokens, enforced at the app layer).
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. Client consent — modelled on `workspace_oxy_lockdown`: opt-in, keyed on
        //    the client org, set only by that org's real officer.
        manager
            .create_table(
                Table::create()
                    .table(PartnerPublishConsent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PartnerPublishConsent::OrgId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    // Explicit ON. Absence of a row is the default OFF; a row with
                    // enabled=false is an explicit, audited revoke that we keep so
                    // the client's history shows the toggle both ways.
                    .col(
                        ColumnDef::new(PartnerPublishConsent::Enabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(PartnerPublishConsent::GrantedBy)
                            .uuid()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(PartnerPublishConsent::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_partner_publish_consent_org")
                            .from(PartnerPublishConsent::Table, PartnerPublishConsent::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // 2. Trusted-publishing config, app-scoped. UNIQUE on the full claim tuple:
        //    the same repo+workflow+environment maps to at most one publisher per
        //    app, and a wildcard is never storable (every column is NOT NULL).
        manager
            .create_table(
                Table::create()
                    .table(AppPublishers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AppPublishers::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AppPublishers::AppId).uuid().not_null())
                    .col(ColumnDef::new(AppPublishers::RepoOwner).text().not_null())
                    // GitHub numeric account id — the account-resurrection defence.
                    // A deleted-and-recreated owner with the same name has a new id.
                    .col(
                        ColumnDef::new(AppPublishers::RepoOwnerId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AppPublishers::RepoName).text().not_null())
                    // ".github/workflows/oxy-publish.yml"
                    .col(ColumnDef::new(AppPublishers::WorkflowRef).text().not_null())
                    // REQUIRED — a token without a matching `environment` claim
                    // fails. The environment is what lets the client attach
                    // required-reviewers to the publish job.
                    .col(ColumnDef::new(AppPublishers::Environment).text().not_null())
                    .col(ColumnDef::new(AppPublishers::CreatedBy).uuid().null())
                    .col(
                        ColumnDef::new(AppPublishers::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_app_publishers_app")
                            .from(AppPublishers::Table, AppPublishers::AppId)
                            .to(Apps::Table, Apps::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_app_publishers_claims")
                    .table(AppPublishers::Table)
                    .col(AppPublishers::AppId)
                    .col(AppPublishers::RepoOwnerId)
                    .col(AppPublishers::RepoName)
                    .col(AppPublishers::WorkflowRef)
                    .col(AppPublishers::Environment)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Lookup path for the OIDC exchange: "which publishers match this repo?"
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_app_publishers_repo")
                    .table(AppPublishers::Table)
                    .col(AppPublishers::RepoOwnerId)
                    .col(AppPublishers::RepoName)
                    .to_owned(),
            )
            .await?;

        // 3. Scope the fallback token. Additive: existing staff tokens have NULL in
        //    both, preserving today's behaviour.
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE app_publish_tokens ADD COLUMN IF NOT EXISTS app_id UUID",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE app_publish_tokens \
                 ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE app_publish_tokens DROP COLUMN IF EXISTS app_id")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE app_publish_tokens DROP COLUMN IF EXISTS expires_at")
            .await?;
        for t in ["app_publishers", "partner_publish_consent"] {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TABLE IF EXISTS {t} CASCADE"))
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum PartnerPublishConsent {
    Table,
    OrgId,
    Enabled,
    GrantedBy,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AppPublishers {
    Table,
    Id,
    AppId,
    RepoOwner,
    RepoOwnerId,
    RepoName,
    WorkflowRef,
    Environment,
    CreatedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Organizations {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Apps {
    Table,
    Id,
}
