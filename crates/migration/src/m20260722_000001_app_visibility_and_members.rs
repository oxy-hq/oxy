use sea_orm_migration::prelude::*;

/// Per-app visibility + membership — "privileged users of custom apps".
///
/// Two separable concerns land together:
///
/// 1. **Visibility** (`apps.visibility`): `org` (default — every member of the
///    owning org can open the app, today's behavior) vs `members` (only listed
///    `app_members`, plus the org owner and Oxy staff as break-glass). Existing
///    rows default to `org`, so this migration changes no one's access.
/// 2. **Roles** (`app_members.role`): `admin` vs `member`. App-admin is what
///    gates an app's own privileged surface (e.g. the warehouse app's
///    `?view=admin`) WITHOUT requiring org-admin — the whole point, since org
///    Admin also carries billing and member management.
///
/// The authority rules live in `oxy-authz` (`Ring::AppAccess` / `Ring::AppAdmin`);
/// these tables are only the facts its loader reads.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            -- Default 'org' preserves the current "any org member sees it" rule
            -- for every existing app; restricting one is an explicit opt-in.
            ALTER TABLE apps
                ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'org';
            ALTER TABLE apps
                DROP CONSTRAINT IF EXISTS apps_visibility_check;
            ALTER TABLE apps
                ADD CONSTRAINT apps_visibility_check
                CHECK (visibility IN ('org', 'members'));

            CREATE TABLE IF NOT EXISTS app_members (
                id UUID PRIMARY KEY,
                app_id UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                role TEXT NOT NULL DEFAULT 'member',
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                created_by UUID REFERENCES users(id) ON DELETE SET NULL,
                CONSTRAINT app_members_role_check CHECK (role IN ('admin', 'member')),
                CONSTRAINT app_members_app_user_unique UNIQUE (app_id, user_id)
            );
            -- The loader's hot read is "every app this user belongs to".
            CREATE INDEX IF NOT EXISTS idx_app_members_user ON app_members(user_id);
            CREATE INDEX IF NOT EXISTS idx_app_members_app ON app_members(app_id);
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS app_members CASCADE;
            ALTER TABLE apps DROP CONSTRAINT IF EXISTS apps_visibility_check;
            ALTER TABLE apps DROP COLUMN IF EXISTS visibility;
        "#,
            )
            .await?;
        Ok(())
    }
}
