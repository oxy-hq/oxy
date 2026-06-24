use sea_orm_migration::prelude::*;

/// Opt-in org subdomain routing. One row per org that has been granted a
/// bare subdomain. The subdomain label IS the org slug
/// (`organizations.slug`) — `<slug>.oxygen-hq.com` — so there's no separate
/// label column. Presence of an `enabled` row is the opt-in flag;
/// `default_workspace_id` is the project the subdomain root scopes to.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum OrgSubdomains {
    Table,
    Id,
    OrgId,
    DefaultWorkspaceId,
    Enabled,
    CreatedBy,
    CreatedAt,
    UpdatedAt,
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
        manager
            .create_table(
                Table::create()
                    .table(OrgSubdomains::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OrgSubdomains::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OrgSubdomains::OrgId).uuid().not_null())
                    .col(ColumnDef::new(OrgSubdomains::DefaultWorkspaceId).uuid())
                    .col(
                        ColumnDef::new(OrgSubdomains::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(OrgSubdomains::CreatedBy).uuid())
                    .col(
                        ColumnDef::new(OrgSubdomains::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(OrgSubdomains::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    // Org delete removes its subdomain mapping.
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_org_subdomains_org_id")
                            .from(OrgSubdomains::Table, OrgSubdomains::OrgId)
                            .to(Organizations::Table, Organizations::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    // Default project may be deleted independently — keep the
                    // subdomain reservation, just null the pointer (dispatch
                    // falls back to the app-host picker).
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_org_subdomains_default_workspace_id")
                            .from(OrgSubdomains::Table, OrgSubdomains::DefaultWorkspaceId)
                            .to(Workspaces::Table, Workspaces::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_org_subdomains_created_by")
                            .from(OrgSubdomains::Table, OrgSubdomains::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // One subdomain mapping per org (the org slug is the routing key).
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("uq_org_subdomains_org_id")
                    .table(OrgSubdomains::Table)
                    .col(OrgSubdomains::OrgId)
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
                    .table(OrgSubdomains::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
