use sea_orm_migration::prelude::*;

/// `app_publish_tokens.created_by` becomes nullable.
///
/// A trusted-publishing OIDC exchange mints an **app-scoped machine credential**
/// with no human behind it (design §6, Option A). Such a token carries its
/// authority in its `app_id` (re-checked against the client's consent at publish
/// time), not in a minting user — so `created_by` must be allowed to be NULL.
///
/// Staff-minted tokens keep a `created_by`; nothing about the existing flow
/// changes. Additive: existing rows are untouched.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE app_publish_tokens ALTER COLUMN created_by DROP NOT NULL",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Only re-assert NOT NULL if no NULLs exist (machine tokens would block it).
        manager
            .get_connection()
            .execute_unprepared(
                "DELETE FROM app_publish_tokens WHERE created_by IS NULL; \
                 ALTER TABLE app_publish_tokens ALTER COLUMN created_by SET NOT NULL",
            )
            .await?;
        Ok(())
    }
}
