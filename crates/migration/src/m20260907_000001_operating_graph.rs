use sea_orm_migration::prelude::*;

/// The operating graph — the assignment graph grown into the platform's model
/// of the physical world. See `internal-docs/operating-graph.md`.
///
/// Four changes, one migration, all additive:
///
/// * `locations` gains a hierarchy (`parent_id`) and a tenant-named level
///   (`kind`). One self-reference; a level is a word, not a table.
/// * `location_external_ids` — what each integration calls this place. The
///   seam the semantic model binds to (`restaurant_id` → a location) and the
///   table Store Ops kept in its own schema until now.
/// * `org_role_members` gains `supervisor_id` — who a person reports to AT
///   THAT PLACE, which is what routes an escalation.
/// * `org_kiosk_devices` gains `location_id` — a tablet is a physical object
///   at a place.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            -- The hierarchy. `parent_id` is SET NULL on delete so retiring a
            -- region leaves its stores as roots rather than taking them with
            -- it. A cycle is refused by the writer, which walks up from the
            -- proposed parent; the database cannot express "acyclic".
            ALTER TABLE locations
                ADD COLUMN parent_id UUID REFERENCES locations(id) ON DELETE SET NULL,
                ADD COLUMN kind TEXT;
            CREATE INDEX locations_by_parent ON locations (org_id, parent_id);

            -- What each integration calls this place. `system` is a lowercase
            -- token the tenant's integrations agree on ('toast', 'momos',
            -- 'unifi', 'payroll'); `external_id` is whatever that system uses.
            -- One id per system per location, and one location per id per
            -- system within an org — the second UNIQUE is what makes a
            -- lookup by external id answer one place.
            CREATE TABLE location_external_ids (
                org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                location_id UUID NOT NULL REFERENCES locations(id) ON DELETE CASCADE,
                system      TEXT NOT NULL,
                external_id TEXT NOT NULL,
                set_by      UUID REFERENCES users(id) ON DELETE SET NULL,
                set_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (location_id, system),
                UNIQUE (org_id, system, external_id)
            );

            -- Who a person reports to at that place.
            ALTER TABLE org_role_members
                ADD COLUMN supervisor_id UUID REFERENCES users(id) ON DELETE SET NULL;

            -- A tablet sits somewhere.
            ALTER TABLE org_kiosk_devices
                ADD COLUMN location_id UUID REFERENCES locations(id) ON DELETE SET NULL;
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
            ALTER TABLE org_kiosk_devices DROP COLUMN location_id;
            ALTER TABLE org_role_members DROP COLUMN supervisor_id;
            DROP TABLE location_external_ids;
            DROP INDEX IF EXISTS locations_by_parent;
            ALTER TABLE locations DROP COLUMN kind, DROP COLUMN parent_id;
            "#,
            )
            .await?;
        Ok(())
    }
}
