use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Promotes the first-registered admin user to Owner.
///
/// Before this migration the only roles were `member` and `admin`.
/// Owner is the new top-level role that can grant/revoke admin.
/// Existing installs have at least one bootstrap admin; we promote
/// the earliest one (by `created_at`) to keep full access intact.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "UPDATE users SET role = 'owner'
             WHERE id = (
                 SELECT id FROM users
                 WHERE role = 'admin'
                 ORDER BY created_at ASC
                 LIMIT 1
             )",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // Revert only the single user that `up` promoted: the earliest-created owner.
        // Demoting ALL owners would incorrectly downgrade any owner that was manually
        // promoted after this migration ran.
        db.execute_unprepared(
            "UPDATE users SET role = 'admin'
             WHERE id = (
                 SELECT id FROM users
                 WHERE role = 'owner'
                 ORDER BY created_at ASC
                 LIMIT 1
             )",
        )
        .await?;
        Ok(())
    }
}
