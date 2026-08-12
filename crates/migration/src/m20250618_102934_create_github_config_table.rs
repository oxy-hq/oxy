use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

/// The `settings` create-table statement.
///
/// Extracted so the test below asserts on the DDL this migration actually
/// emits, rather than on a copy that can drift away from it.
fn settings_table() -> TableCreateStatement {
    Table::create()
        .table(Settings::Table)
        .if_not_exists()
        // `pk_auto` expands to `integer().auto_increment().primary_key()`,
        // which renders an identity column on SeaORM 2.0 without the
        // non-default `postgres-use-serial-pk` feature. This table is
        // `serial` in every deployed database, so say so directly.
        .col(
            ColumnDef::new(Settings::Id)
                .custom(Alias::new("serial"))
                .not_null()
                .primary_key(),
        )
        .col(text(Settings::GithubToken)) // Encrypted GitHub token
        .col(big_integer_null(Settings::SelectedRepoId)) // GitHub repository ID
        .col(boolean(Settings::Onboarded).default(false))
        .col(text_null(Settings::Revision)) // Current revision/commit hash of the synced repo
        .col(string_len(Settings::SyncStatus, 20)) // Sync status enum: idle, syncing, synced, error
        .col(
            timestamp_with_time_zone(Settings::CreatedAt)
                .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
        )
        .col(
            timestamp_with_time_zone(Settings::UpdatedAt)
                .extra("DEFAULT CURRENT_TIMESTAMP".to_string()),
        )
        .to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create settings table for GitHub integration
        // This table should have 0 or 1 row - only one GitHub repository is supported for the whole app
        manager.create_table(settings_table()).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Settings::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Settings {
    Table,
    Id,
    GithubToken,
    SelectedRepoId,
    Revision,
    SyncStatus,
    CreatedAt,
    UpdatedAt,
    Onboarded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::sea_query::PostgresQueryBuilder;

    /// `settings.id` must render `serial`, not an identity column.
    ///
    /// This site was the gap in the previous guard: it spelled the PK as
    /// `pk_auto(Settings::Id)`, which expands to
    /// `integer().auto_increment().primary_key()`, so it was covered by the
    /// `postgres-use-serial-pk` feature without any test naming it. Dropping
    /// the feature would have flipped this table to an identity column with
    /// every test still green.
    #[test]
    fn settings_id_renders_serial() {
        let ddl = settings_table().to_string(PostgresQueryBuilder);

        assert!(
            ddl.contains("serial"),
            "settings.id must render `serial` — has it regressed to `pk_auto` / \
             `.auto_increment()`? Rendered: {ddl}"
        );
        assert!(
            !ddl.to_uppercase().contains("IDENTITY"),
            "identity columns diverge from every deployed database. Rendered: {ddl}"
        );
    }
}
