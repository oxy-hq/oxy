use sea_orm_migration::prelude::*;

/// Adds `app_role` + `org_role` to `custom_app_view_event` — the viewer's
/// standing **at the moment they opened the app**.
///
/// The Activity tab could already answer "who opened this app" (the table has
/// carried `user_email` since it was created) but not "in what capacity". The
/// obvious alternative — joining `app_members` / `org_members` at read time —
/// is wrong rather than merely slower: roles change, so a person who was an
/// admin when they ran an export and is a plain member today would render as a
/// member, and the record would quietly rewrite its own history. A usage log
/// has to snapshot.
///
/// Note this is the *opposite* trade-off from the neighbouring `user_email`,
/// whose doc comment calls staleness acceptable because it is re-derivable from
/// `user_id`. A past role is not re-derivable from anything. Same
/// denormalization, opposite reason.
///
/// Nullable + additive: existing rows stay `NULL`, which reads as "not
/// recorded" and is deliberately distinct from a role of `member`. `NULL` also
/// covers a live row whose role lookup failed — view recording is best-effort
/// and must never fail a page load, so an unresolvable role is an absent label,
/// never a guessed one.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE custom_app_view_event \
                   ADD COLUMN IF NOT EXISTS app_role VARCHAR(16), \
                   ADD COLUMN IF NOT EXISTS org_role VARCHAR(16)",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE custom_app_view_event \
                   DROP COLUMN IF EXISTS app_role, \
                   DROP COLUMN IF EXISTS org_role",
            )
            .await?;
        Ok(())
    }
}
