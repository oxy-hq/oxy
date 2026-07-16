use sea_orm_migration::prelude::*;

/// `app_builds` gains a recorded validation status, so promotion-to-live can be
/// gated on it — the "validator can't be bypassed" invariant. Before this,
/// nothing held promotion, so the gate's enforcement half did not exist.
///
/// `validation_status`:
///   `passed`  — validation recorded as successful. Gate 1 (fast byte-level
///               checks) runs synchronously at publish and 422s on failure, so
///               every build that reaches storage today is `passed`.
///   `pending` — awaiting a deeper deploy-time render probe (gate 2 — tracked
///               follow-up, not yet built).
///   `failed`  — a probe recorded a failure; promotion is refused.
/// `validation_detail` carries the human-readable reason when `failed`.
///
/// Additive + safe: existing builds are already serving, so they default to
/// `passed` and keep working; new builds are stamped explicitly at publish.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE app_builds \
                   ADD COLUMN IF NOT EXISTS validation_status VARCHAR NOT NULL DEFAULT 'passed', \
                   ADD COLUMN IF NOT EXISTS validation_detail TEXT",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE app_builds \
                   DROP COLUMN IF EXISTS validation_status, \
                   DROP COLUMN IF EXISTS validation_detail",
            )
            .await?;
        Ok(())
    }
}
