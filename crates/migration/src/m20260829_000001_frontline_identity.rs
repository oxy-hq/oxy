use sea_orm_migration::prelude::*;

/// Frontline identity — enrolling a human with no email address.
///
/// Design record: `internal-docs/frontline-identity.md`. The short version:
/// one principal (a real `users` row), email demoted from *the* key to *a*
/// credential, and frontline standing deliberately kept out of `org_members`.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            -- 1. Email becomes optional.
            --
            -- The UNIQUE stays exactly as it is: Postgres permits many NULLs in
            -- a unique index, so "at most one user per address" survives without
            -- a partial index or a sentinel value. Every existing row keeps its
            -- address, so nothing about today's login paths changes.
            ALTER TABLE users ALTER COLUMN email DROP NOT NULL;

            -- 2. Credentials are their own table.
            --
            -- `org_id` is the scope, and it is the whole reason a 4-digit PIN is
            -- viable: a PIN is unique inside one org, never globally. Email
            -- credentials carry org_id IS NULL and stay globally unique.
            CREATE TABLE user_credentials (
                id            UUID PRIMARY KEY,
                user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                kind          TEXT NOT NULL,
                org_id        UUID REFERENCES organizations(id) ON DELETE CASCADE,
                identifier    TEXT NOT NULL,
                -- argon2 for 'pin'. NULL where possession is the proof (an
                -- emailed link, an SMS code) and there is no secret at rest.
                secret_hash   TEXT,
                -- Throttle state. On the credential, not the user: locking an
                -- account because somebody guessed at its PIN would let one
                -- kiosk lock a worker out of every other way in, and the PIN is
                -- the only credential that can be brute-forced (an emailed link
                -- and an SMS code prove possession instead).
                failed_attempts INTEGER NOT NULL DEFAULT 0,
                locked_until  TIMESTAMPTZ,
                created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
                last_used_at  TIMESTAMPTZ,
                CONSTRAINT user_credentials_kind_check
                    CHECK (kind IN ('email', 'phone', 'pin')),
                -- A PIN without a secret is not a credential; an org-scoped
                -- credential without an org is not scoped. Both are cheap to
                -- get wrong from application code and expensive to notice.
                CONSTRAINT user_credentials_pin_has_secret
                    CHECK (kind <> 'pin' OR (secret_hash IS NOT NULL AND org_id IS NOT NULL))
            );

            -- TWO partial indexes, not one constraint over (kind, org_id,
            -- identifier). Postgres treats NULLs as DISTINCT in a unique
            -- constraint, so that single form would accept two rows of
            -- ('email', NULL, 'a@b.com') and the global uniqueness this model
            -- leans on would be silently gone.
            CREATE UNIQUE INDEX user_credentials_global
                ON user_credentials (kind, identifier) WHERE org_id IS NULL;
            CREATE UNIQUE INDEX user_credentials_scoped
                ON user_credentials (kind, org_id, identifier) WHERE org_id IS NOT NULL;

            CREATE INDEX user_credentials_user_idx ON user_credentials (user_id);

            -- 3. Frontline standing — NOT an org_members row.
            --
            -- An org Member reaches Airhouse settings, and through
            -- EffectiveWorkspaceRole reaches Databases and Secrets. Enrolling
            -- hourly staff there would hand them the tenant's credential
            -- surface: privilege escalation by construction, not a policy that
            -- could be tightened afterwards.
            --
            -- No `role` column on purpose. Role vocabulary is the one part of
            -- this that genuinely needs the customer, and an empty enum now
            -- beats a wrong one shipped.
            CREATE TABLE org_frontline_members (
                org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                status      TEXT NOT NULL DEFAULT 'active',
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (org_id, user_id),
                CONSTRAINT org_frontline_members_status_check
                    CHECK (status IN ('active', 'suspended'))
            );

            CREATE INDEX org_frontline_members_user_idx
                ON org_frontline_members (user_id);
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Restoring NOT NULL would fail against any row this feature created,
        // so the down path deletes the email-less users it is responsible for
        // first. That is destructive, and it is the honest inverse: those rows
        // cannot exist in the schema being rolled back to.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS org_frontline_members CASCADE;
            DROP TABLE IF EXISTS user_credentials CASCADE;
            DELETE FROM users WHERE email IS NULL;
            ALTER TABLE users ALTER COLUMN email SET NOT NULL;
        "#,
            )
            .await?;
        Ok(())
    }
}
