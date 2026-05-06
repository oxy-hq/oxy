use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rebind airhouse tables from org_id to workspace_id.
        // The tables were created in m20260429_000001/2 and contain no
        // production data yet, so DROP + RECREATE is the cleanest path.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS airhouse_users;
            DROP TABLE IF EXISTS airhouse_tenants;

            CREATE TABLE airhouse_tenants (
                id UUID PRIMARY KEY,
                workspace_id UUID NOT NULL UNIQUE
                    REFERENCES workspaces(id) ON DELETE CASCADE,
                airhouse_tenant_id VARCHAR(63) NOT NULL UNIQUE,
                bucket TEXT NOT NULL,
                prefix TEXT,
                status VARCHAR(32) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            CREATE INDEX idx_airhouse_tenants_workspace ON airhouse_tenants(workspace_id);

            CREATE TABLE airhouse_users (
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
            CREATE INDEX idx_airhouse_users_oxy_user ON airhouse_users(oxy_user_id);
            CREATE INDEX idx_airhouse_users_workspace ON airhouse_users(workspace_id);
        "#,
            )
            .await?;
        Ok(())
    }

    /// **DESTRUCTIVE.** Drops both `airhouse_users` and `airhouse_tenants`
    /// without any data preservation. This was acceptable when the migration
    /// was authored (no production data existed yet) but will lose tenant
    /// metadata, secret references, and per-user provisioning state if run
    /// against a database that has actually been used. Treat this migration
    /// as effectively up-only in any non-throwaway environment — restore
    /// from backup rather than rolling back.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            -- WARNING: this drops all airhouse provisioning data. There is no
            -- corresponding up-migration backup; rolling forward again will
            -- start from an empty state. See the doc comment on `down`.
            DROP TABLE IF EXISTS airhouse_users;
            DROP TABLE IF EXISTS airhouse_tenants;
        "#,
            )
            .await?;
        Ok(())
    }
}
