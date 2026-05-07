use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Drop `airhouse_users` (Phase 6 of the SA migration) and sweep the
/// per-user password secrets out of `org_secrets`.
///
/// Phase 5 of the SA migration removed the only writers to `airhouse_users`:
/// `provision` no longer creates a per-user row, `rotate-password` is gone,
/// and `credentials` mints fresh ephemerals via the broker. By the time
/// this migration runs the table is read-only legacy state — nothing in
/// the codebase queries it anymore.
///
/// The `org_secrets` sweep cleans up `airhouse_user_password:*` rows that
/// `airhouse_users.password_secret_id` used to point at. The runbook
/// already noted these as not-cascaded-on-tenant-delete; this migration
/// is the catch-all.
///
/// Down migration is intentionally **destructive of forward state** —
/// recreating the table with no data won't unbreak anything because the
/// rows are gone. Treat as up-only in any non-throwaway environment.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS airhouse_users;

            -- Best-effort sweep. `org_secrets` lives in the central oxy DB;
            -- if a deployment ran this migration without the central
            -- migrator first the table won't exist and the DELETE is a
            -- no-op. The IF EXISTS check guards that case.
            DO $$
            BEGIN
                IF EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_name = 'org_secrets'
                ) THEN
                    DELETE FROM org_secrets
                    WHERE name LIKE 'airhouse_user_password:%';
                END IF;
            END
            $$;
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
            -- WARNING: recreated table is empty. The original rows are
            -- unrecoverable; restore from backup if needed.
            CREATE TABLE IF NOT EXISTS airhouse_users (
                id UUID PRIMARY KEY,
                tenant_row_id UUID NOT NULL
                    REFERENCES airhouse_tenants(id) ON DELETE CASCADE,
                workspace_id UUID NOT NULL
                    REFERENCES workspaces(id) ON DELETE CASCADE,
                oxy_user_id UUID NOT NULL
                    REFERENCES users(id) ON DELETE CASCADE,
                username VARCHAR(63) NOT NULL,
                role VARCHAR(16) NOT NULL,
                password_secret_id UUID,
                password_revealed_at TIMESTAMPTZ,
                status VARCHAR(32) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                CONSTRAINT uniq_airhouse_users_workspace_user
                    UNIQUE (workspace_id, oxy_user_id),
                CONSTRAINT uniq_airhouse_users_tenant_username
                    UNIQUE (tenant_row_id, username)
            );
            CREATE INDEX IF NOT EXISTS idx_airhouse_users_oxy_user
                ON airhouse_users(oxy_user_id);
            CREATE INDEX IF NOT EXISTS idx_airhouse_users_workspace
                ON airhouse_users(workspace_id);
        "#,
            )
            .await?;
        Ok(())
    }
}
