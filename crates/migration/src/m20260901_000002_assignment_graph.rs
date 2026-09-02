use sea_orm_migration::prelude::*;

/// The assignment graph — locations, tenant-defined roles, and work items.
///
/// This is the table five product surfaces are clients of: Tasks, Site visits,
/// Location launcher, Training and Compliance all reduce to "somebody owes
/// somebody a piece of work at a place by a time". Building it once is the
/// entire argument for it being a platform entity rather than five app tables
/// that each re-derive "assigned to me" and "supervised by me".
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            -- A physical place work happens at. The axis every operational
            -- record hangs off, and the thing a multi-unit operator thinks in.
            CREATE TABLE locations (
                id          UUID PRIMARY KEY,
                org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                name        TEXT NOT NULL,
                -- Lifecycle, not a boolean. An operator's roster is mostly NOT
                -- open stores: the tenant this is modelled on runs 13 open, 7
                -- launching and 5 pre-launch, and the launching ones are where
                -- the work actually is.
                status      TEXT NOT NULL DEFAULT 'open'
                            CHECK (status IN ('pre_launch','launching','open','archived','terminated')),
                -- Work is due at a LOCAL time. Storing the zone per location is
                -- what stops "due by close" meaning 23:00 UTC at a store in
                -- Oregon.
                timezone    TEXT NOT NULL DEFAULT 'UTC',
                external_id TEXT,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (org_id, name)
            );
            CREATE INDEX locations_by_org ON locations (org_id, status);

            -- Tenant-defined roles. `org_members.role` is a three-value enum;
            -- a real operator invents its own vocabulary — eight roles split
            -- across "works at a location" and "works at head office" — and
            -- expects it to mean something.
            --
            -- NOT an authorization principal. `oxy-authz` still decides what a
            -- person may do; this decides what they are CALLED and what work
            -- routes to them. Conflating the two is how a display label
            -- silently becomes a permission.
            CREATE TABLE org_roles (
                id         UUID PRIMARY KEY,
                org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                name       TEXT NOT NULL,
                scope      TEXT NOT NULL CHECK (scope IN ('location','franchisor')),
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (org_id, name)
            );

            -- Who holds a role, and where.
            --
            -- `location_id` is NULL for a franchisor-scope role: a Corporate
            -- user holds it across the org rather than at one store, and a
            -- sentinel location would make every location query have to know
            -- about it.
            CREATE TABLE org_role_members (
                id          UUID PRIMARY KEY,
                org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                role_id     UUID NOT NULL REFERENCES org_roles(id) ON DELETE CASCADE,
                user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                location_id UUID REFERENCES locations(id) ON DELETE CASCADE,
                created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (role_id, user_id, location_id)
            );
            -- Postgres treats NULLs as distinct, so the UNIQUE above does not
            -- bind the franchisor-scope grant it was meant to: `(role, user,
            -- NULL)` can be inserted without limit, and every duplicate adds
            -- another OR arm to the scope query that reads a user's work.
            CREATE UNIQUE INDEX org_role_members_org_wide_grant_is_unique
                ON org_role_members (role_id, user_id)
                WHERE location_id IS NULL;
            CREATE INDEX org_role_members_by_user ON org_role_members (user_id);
            CREATE INDEX org_role_members_by_location ON org_role_members (location_id);

            -- The graph itself.
            CREATE TABLE work_items (
                id           UUID PRIMARY KEY,
                org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                location_id  UUID REFERENCES locations(id) ON DELETE CASCADE,
                title        TEXT NOT NULL,
                body         TEXT,

                -- ASSIGNMENT: to a person, or to whoever holds a role here.
                -- Both are needed and they are not the same fact. "The closing
                -- checklist" belongs to whoever is on shift, which is a role;
                -- "re-test the sanitiser you logged wrong" belongs to a person.
                assignee_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
                assignee_role_id UUID REFERENCES org_roles(id) ON DELETE SET NULL,
                supervisor_id    UUID REFERENCES users(id) ON DELETE SET NULL,

                due_at       TIMESTAMPTZ,
                status       TEXT NOT NULL DEFAULT 'open'
                             CHECK (status IN ('open','in_progress','done','cancelled')),
                priority     SMALLINT NOT NULL DEFAULT 0,

                -- PROVENANCE, polymorphic on purpose. Work arrives from a
                -- failed form answer, a site-visit finding, a launcher
                -- template, a training path, a document expiry — five kinds
                -- today and more later. Five nullable foreign keys would add a
                -- column per source forever; a (kind, id) pair costs the
                -- referential integrity, which is the right trade for a field
                -- that exists to answer "why does this task exist" rather than
                -- to be joined on.
                source_kind  TEXT,
                source_id    TEXT,

                created_by   UUID REFERENCES users(id) ON DELETE SET NULL,
                created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
                completed_at TIMESTAMPTZ,
                completed_by UUID REFERENCES users(id) ON DELETE SET NULL,

                -- NOT a CHECK that an assignee exists, though an earlier draft
                -- had one. Both assignment columns are `ON DELETE SET NULL`, so
                -- deleting the user or the role that an item is solely assigned
                -- to would null the last non-null column and violate the check —
                -- which does not reject the assignment, it rejects the DELETE.
                -- Removing a departed employee, or retiring a role, would fail
                -- with a constraint error naming a table nobody was touching.
                --
                -- Assignment lapsing is a real state, not a corrupt one: when
                -- somebody leaves, their open work becomes nobody's job, and
                -- that is precisely the thing a manager has to see and reassign.
                -- So the invariant is enforced where it belongs — at creation,
                -- in the handler, with a 400 that says which field is missing —
                -- and the lapsed state is made cheap to find instead of
                -- impossible to reach.
                --
                -- A completed item records who and when, or neither.
                CONSTRAINT work_items_completion_is_whole
                    CHECK ((status = 'done') = (completed_at IS NOT NULL))
            );

            -- The two views the product is built out of. Partial on `status`
            -- because both screens ask for open work, and the closed tail of a
            -- year-old store dwarfs it.
            CREATE INDEX work_items_assigned
                ON work_items (assignee_user_id, due_at) WHERE status <> 'done';
            CREATE INDEX work_items_supervised
                ON work_items (supervisor_id, due_at) WHERE status <> 'done';
            CREATE INDEX work_items_by_role
                ON work_items (assignee_role_id, location_id, due_at) WHERE status <> 'done';
            CREATE INDEX work_items_by_location
                ON work_items (location_id, status, due_at);
            -- Work whose assignment lapsed — the person left, or the role was
            -- retired. The state the dropped CHECK used to make unreachable, and
            -- the one a manager most needs surfaced: an unassigned item is the
            -- commonest way a checklist quietly stops running.
            CREATE INDEX work_items_unassigned
                ON work_items (org_id, due_at)
                WHERE status <> 'done'
                  AND assignee_user_id IS NULL
                  AND assignee_role_id IS NULL;
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
            DROP TABLE IF EXISTS work_items CASCADE;
            DROP TABLE IF EXISTS org_role_members CASCADE;
            DROP TABLE IF EXISTS org_roles CASCADE;
            DROP TABLE IF EXISTS locations CASCADE;
        "#,
            )
            .await?;
        Ok(())
    }
}
