use sea_orm_migration::prelude::*;

/// The OLTP control plane's two tables, in their final shape.
///
/// **Squashed from five.** This feature has never been deployed, so the four
/// `ALTER TABLE`s that followed the original `CREATE` were history nobody
/// needs: every column below arrived on this branch, and replaying the
/// intermediate shapes only makes the schema harder to read. The comments the
/// alters carried are kept — they explain why a column is nullable, which is
/// the part that stays useful.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE oltp_tenants (
                id UUID PRIMARY KEY,
                org_id UUID NOT NULL UNIQUE REFERENCES organizations(id) ON DELETE CASCADE,
                provider VARCHAR(32) NOT NULL,
                project_id VARCHAR(255) NOT NULL,
                branch_id VARCHAR(255) NOT NULL,
                project_name VARCHAR(63) NOT NULL UNIQUE,
                region VARCHAR(64) NOT NULL,
                pg_version SMALLINT NOT NULL,
                host TEXT NOT NULL,
                database_name VARCHAR(63) NOT NULL,
                owner_role VARCHAR(63) NOT NULL,
                owner_password_ciphertext BYTEA,
                status VARCHAR(32) NOT NULL,

                -- Which version of Oxy's own in-tenant objects (oxy_meta
                -- ledger, analyst role, baseline grants) this database carries.
                -- Tracked here, in the control plane, so "which tenants are
                -- behind?" is one query rather than N connections.
                platform_schema_version INTEGER NOT NULL DEFAULT 0,

                -- Sealed login password for the tenant's analyst role.
                --
                -- On the tenant rather than in `oltp_roles` because the analyst
                -- is not a writer: no schema, no writer_kind, exactly one per
                -- database. Modelling it as a writer would mean inventing a
                -- third WriterKind that owns nothing.
                --
                -- NULL means the login has not been minted yet; the provisioner
                -- mints it lazily on first resolve.
                analyst_password_ciphertext BYTEA,

                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            CREATE INDEX idx_oltp_tenants_org ON oltp_tenants(org_id);
            CREATE INDEX idx_oltp_tenants_platform_version
                ON oltp_tenants(platform_schema_version);

            CREATE TABLE oltp_roles (
                id UUID PRIMARY KEY,
                tenant_row_id UUID NOT NULL
                    REFERENCES oltp_tenants(id) ON DELETE CASCADE,
                writer_kind VARCHAR(16) NOT NULL,
                writer_name VARCHAR(63) NOT NULL,
                schema_name VARCHAR(63) NOT NULL,
                role_name VARCHAR(63) NOT NULL,
                grant_level VARCHAR(8) NOT NULL,
                password_ciphertext BYTEA NOT NULL,

                -- Which workspace owns this schema namespace.
                --
                -- An OLTP database is per ORG, but schema definitions compile
                -- per WORKSPACE, and an org may hold several. Two workspaces
                -- both declaring `app_bookings` would interleave DDL into one
                -- schema, each overwriting the other's idea of the tables.
                -- Claiming the namespace makes the second one fail its compile.
                --
                -- NULL means unclaimed; `ensure_writer` adopts such a row
                -- rather than failing.
                claimed_by_workspace_id UUID,

                -- Whether the read-only analyst may read this writer's schema.
                --
                -- This lived ONLY as a GRANT inside the tenant, so nothing in
                -- Oxy knew what an operator had chosen, and every caller
                -- re-derived it from the kind's DEFAULT. That produced two
                -- silent failures: a pipeline opted OUT had its grants
                -- reinstated by the next migration, and an app opted IN stopped
                -- covering tables added later.
                --
                -- NULL means "never chosen" — the reader falls back to the
                -- kind's default (`raw_*` visible, `app_*` not).
                analytics_visible BOOLEAN,

                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                rotated_at TIMESTAMPTZ,

                -- One role per (tenant, role name). The provisioner reconciles
                -- against this rather than creating a duplicate after a partial
                -- provision.
                CONSTRAINT uq_oltp_roles_tenant_role UNIQUE (tenant_row_id, role_name),
                -- A writer owns exactly one schema, so the same schema cannot be
                -- claimed twice within a tenant.
                CONSTRAINT uq_oltp_roles_tenant_schema UNIQUE (tenant_row_id, schema_name)
            );
            CREATE INDEX idx_oltp_roles_tenant ON oltp_roles(tenant_row_id);
            CREATE INDEX idx_oltp_roles_claim ON oltp_roles(claimed_by_workspace_id);
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
            DROP TABLE IF EXISTS oltp_roles CASCADE;
            DROP TABLE IF EXISTS oltp_tenants CASCADE;
        "#,
            )
            .await?;
        Ok(())
    }
}
