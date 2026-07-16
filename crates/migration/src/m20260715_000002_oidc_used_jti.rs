use sea_orm_migration::prelude::*;

/// `oidc_used_jti` — the single-use ledger for trusted-publishing OIDC tokens.
///
/// A GitHub Actions OIDC token is valid for minutes; without single-use, a stolen
/// token mints unlimited credentials in that window. Each accepted token's `jti`
/// is inserted here; a duplicate insert (PK conflict) means replay → reject.
///
/// Keyed in the DB, not in memory, because Oxy runs multi-replica: an in-process
/// set would let the same token be replayed against a second replica.
/// `expires_at` lets a sweeper prune spent rows past the token's lifetime.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OidcUsedJti::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OidcUsedJti::Jti)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(OidcUsedJti::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        // Prune path: "delete every spent jti past its expiry".
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_oidc_used_jti_expires")
                    .table(OidcUsedJti::Table)
                    .col(OidcUsedJti::ExpiresAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(OidcUsedJti::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum OidcUsedJti {
    Table,
    Jti,
    ExpiresAt,
}
