use sea_orm_migration::prelude::*;

/// **Platform standing becomes a grant** — `(role × scope)` instead of a boolean.
///
/// `app_admins` used to answer one question: is this email Oxy staff? Every row meant
/// the same thing, so `PrincipalFacts::is_global_admin` was a `bool` and the nine tenant
/// rings that honour a staff override could not tell an app publisher from someone
/// entitled to delete the org. `Ring::OwnerOnly` — "delete, ownership transfer,
/// owner-promotion" — honoured that flag, which is the hole this closes.
///
/// Two additive columns and one child table. No existing row changes meaning:
///
/// 1. **`app_admins.role`** — the preset the grant was issued as, defaulting to
///    `global_admin`. Every pre-existing row backfills to exactly what it already
///    meant, so this migration is behaviour-neutral on its own. `PlatformRole::caps()`
///    in `oxy-authz` expands the name; the expansion is **not** stored, which is what
///    keeps policy in code (an engine reading policy-as-data is an explicit non-goal —
///    see `crates/authz/src/lib.rs`).
/// 2. **`app_admins.scope_all`** — `true` (the default, and what every existing row
///    gets) means the grant reaches every org, present and future. `false` means it
///    reaches exactly the orgs in the child table.
/// 3. **`app_admin_scope_orgs`** — the org set for a bounded grant.
///
/// **`scope_all` is a column and not "the child table is empty"** on purpose. Deriving
/// unbounded from an absent row makes deleting the last scope row silently promote a
/// bounded grant to a global one — the failure mode points the wrong way. With an
/// explicit flag, `scope_all = false` with no rows reaches nothing, which is the safe
/// direction.
///
/// **The table keeps its name.** It now holds App Operators as well as Global Admins,
/// so `app_admins` reads narrow — but the name is a storage contract with live rows,
/// entity mappings and raw SQL behind it, and this repo already keeps such names when
/// the concept is renamed (`agentic_workflow_state` survives the Workflow → Automation
/// rename for the same reason). The mapping is documented in
/// `internal-docs/roles-and-authorization.md`.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            ALTER TABLE app_admins
                ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'global_admin',
                ADD COLUMN IF NOT EXISTS scope_all BOOLEAN NOT NULL DEFAULT true,
                -- A grant is now UPDATED in place (role and scope are replaceable), so
                -- `created_at` alone can no longer answer "when did this become what it
                -- is". Backfilled to `created_at` so an untouched row reads as
                -- "unchanged since it was granted" rather than "changed just now".
                ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
            UPDATE app_admins SET updated_at = created_at WHERE updated_at > created_at;

            -- Reject a role id this build cannot expand. `PlatformRole::from_str`
            -- already drops an unknown id at load time (denying, never guessing), so
            -- this constraint is the write-side half of the same rule: the two must
            -- agree or a typo becomes a silently powerless grant.
            ALTER TABLE app_admins
                DROP CONSTRAINT IF EXISTS app_admins_role_known;
            ALTER TABLE app_admins
                ADD CONSTRAINT app_admins_role_known
                CHECK (role IN ('global_admin', 'app_operator'));

            CREATE TABLE IF NOT EXISTS app_admin_scope_orgs (
                id UUID PRIMARY KEY,
                app_admin_id UUID NOT NULL REFERENCES app_admins(id) ON DELETE CASCADE,
                org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                created_by UUID REFERENCES users(id) ON DELETE SET NULL,
                CONSTRAINT app_admin_scope_orgs_unique UNIQUE (app_admin_id, org_id)
            );
            -- The loader's only read is "every org in this grant's scope"; the org
            -- direction is indexed for the admin UI's "who reaches this tenant?".
            CREATE INDEX IF NOT EXISTS idx_app_admin_scope_orgs_admin
                ON app_admin_scope_orgs(app_admin_id);
            CREATE INDEX IF NOT EXISTS idx_app_admin_scope_orgs_org
                ON app_admin_scope_orgs(org_id);
            "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Dropping `role`/`scope_all` collapses every grant back to "staff", which is
        // exactly what the pre-migration code assumed — a down-migration widens an App
        // Operator to a Global Admin. That is inherent to reverting this change, not an
        // oversight: run it only alongside a binary that predates the capability split.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS app_admin_scope_orgs;
            ALTER TABLE app_admins DROP CONSTRAINT IF EXISTS app_admins_role_known;
            ALTER TABLE app_admins
                DROP COLUMN IF EXISTS role,
                DROP COLUMN IF EXISTS scope_all,
                DROP COLUMN IF EXISTS updated_at;
            "#,
            )
            .await?;
        Ok(())
    }
}
