use sea_orm_migration::prelude::*;

/// Org-level **teams**, and app access granted to a team.
///
/// `m20260722_000001_app_visibility_and_members` shipped the enforcement engine
/// (`apps.visibility` + `app_members` + `Ring::AppAccess`/`Ring::AppAdmin`) but no
/// control surface — nothing in the product could ever write those rows. This
/// migration adds the missing half, and does it by the unit an org admin actually
/// thinks in: a named team, not a per-app list of people.
///
/// Three tables, all additive — no existing row changes meaning:
///
/// 1. **`org_teams`** — a named audience inside one org ("Finance", "Store
///    Managers"). Deliberately org-scoped, not workspace-scoped: an org with
///    several workspaces should name "Finance" once, and the officer doing the
///    granting thinks in org terms.
/// 2. **`org_team_members`** — who is in a team. Membership is validated at write
///    time to be an org member; there is no external-guest path (see
///    `Ring::AppAccess`, which requires org membership on the restricted arm).
/// 3. **`app_team_grants`** — the twin of `app_members`, keyed by team instead of
///    user, carrying the same `role`. This is what makes "grantee = user | team"
///    one concept with two kinds rather than two parallel systems.
///
/// The authority rules live in `oxy-authz` (`Ring::AppGrant` for who may edit these,
/// `Ring::AppAccess`/`Ring::AppAdmin` for what they buy). These tables are only
/// facts — `oxy_server_authz::loader` unions team-reached grants into the SAME
/// `PrincipalFacts` vectors the direct `app_members` rows already populate, which is
/// why no ring had to learn about teams.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            CREATE TABLE IF NOT EXISTS org_teams (
                id UUID PRIMARY KEY,
                org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                description TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                created_by UUID REFERENCES users(id) ON DELETE SET NULL,
                CONSTRAINT org_teams_name_not_blank CHECK (btrim(name) <> '')
            );
            -- Case-insensitive uniqueness: "Finance" and "finance" are the same
            -- team to a human, and letting both exist makes the grant list a
            -- guessing game. Expression index because the constraint is on lower().
            CREATE UNIQUE INDEX IF NOT EXISTS idx_org_teams_org_name
                ON org_teams(org_id, lower(name));
            CREATE INDEX IF NOT EXISTS idx_org_teams_org ON org_teams(org_id);

            CREATE TABLE IF NOT EXISTS org_team_members (
                id UUID PRIMARY KEY,
                team_id UUID NOT NULL REFERENCES org_teams(id) ON DELETE CASCADE,
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                created_by UUID REFERENCES users(id) ON DELETE SET NULL,
                CONSTRAINT org_team_members_team_user_unique UNIQUE (team_id, user_id)
            );
            -- The loader's hot read is "every team this user is in", then "every
            -- app granted to those teams" — both directions are indexed.
            CREATE INDEX IF NOT EXISTS idx_org_team_members_user
                ON org_team_members(user_id);
            CREATE INDEX IF NOT EXISTS idx_org_team_members_team
                ON org_team_members(team_id);

            CREATE TABLE IF NOT EXISTS app_team_grants (
                id UUID PRIMARY KEY,
                app_id UUID NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
                team_id UUID NOT NULL REFERENCES org_teams(id) ON DELETE CASCADE,
                role TEXT NOT NULL DEFAULT 'member',
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                created_by UUID REFERENCES users(id) ON DELETE SET NULL,
                CONSTRAINT app_team_grants_role_check CHECK (role IN ('admin', 'member')),
                CONSTRAINT app_team_grants_app_team_unique UNIQUE (app_id, team_id)
            );
            CREATE INDEX IF NOT EXISTS idx_app_team_grants_team
                ON app_team_grants(team_id);
            CREATE INDEX IF NOT EXISTS idx_app_team_grants_app
                ON app_team_grants(app_id);
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop in FK order (grants → members → teams). CASCADE covers it either
        // way, but stating the order keeps the intent readable.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DROP TABLE IF EXISTS app_team_grants CASCADE;
            DROP TABLE IF EXISTS org_team_members CASCADE;
            DROP TABLE IF EXISTS org_teams CASCADE;
        "#,
            )
            .await?;
        Ok(())
    }
}
