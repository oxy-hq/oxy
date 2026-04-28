use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            "slack_channel_bindings",
            "slack_user_identities",
            "slack_conversation_contexts",
        ] {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TABLE IF EXISTS {table} CASCADE"))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // No-op: this migration drops three legacy tables that were never
        // populated in production. A proper rollback would require re-
        // creating the tables with their original schemas; since the data is
        // permanently gone, a rollback is meaningless. If you need to revert
        // this migration in development, drop your database and re-run
        // migrations without this one.
        Ok(())
    }
}
