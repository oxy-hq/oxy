use sea_orm_migration::prelude::*;

/// Adds `cursor_lag_floor_secs` to the operational tier
/// (`airway_deployment_config`) — airway 0.1.24's eighth `GlobalConfig`
/// setting, contributed upstream by oxy as #111.
///
/// # What the setting is
///
/// A **floor** under every resource's declared `cursor_lag`, in whole seconds.
/// The effective lag becomes `max(contract.cursor_lag(), floor)`, resolved by
/// `airway::config::global::resolve_cursor_lookback` — the operator's guard
/// against a source whose index lags further back than its contract claims.
///
/// A floor, never a ceiling, and airway is explicit that the direction is the
/// safety argument: widening a window can only re-read rows a pull would have
/// resumed past, while capping one a vendor genuinely needs reintroduces the
/// skip the declaration exists to prevent. There is deliberately no
/// `max_rewind` column here for the same reason upstream defers the key.
///
/// # NULL means *no floor* — and so does nothing else
///
/// Same rule as every other column on this table, but this one has a second
/// spelling that had to be closed off. `max(lag, 0)` is `lag` for every
/// resource in the tree, so a stored `0` would be a written setting that raises
/// nothing — accepted, validated, stored and ignored, which is the failure this
/// whole surface exists to avoid. Upstream `GlobalConfig::validate` **rejects**
/// a zero floor rather than reading it as absence, and the admin write path
/// runs every save through that same `validate` (via
/// `DeploymentValues::to_global`), so a `0` comes back as a `400` naming the
/// key and never reaches this column.
///
/// # The `>= 0` CHECK is the type-domain guard, not the zero rule
///
/// Matching the columns this table already carries: `cursor_lag_floor_secs` is
/// a `Duration` on the airway side and Postgres has no unsigned integer, so a
/// negative would be a value the Rust type cannot hold. **The rule that zero is
/// refused is not restated here**, deliberately — it lives in airway's
/// `validate` and is enforced on the write path, so there is exactly one place
/// that decides what a meaningful value is. A `CHECK (> 0)` would be a second
/// copy of an upstream rule, free to drift the day upstream changes its mind,
/// and it would report a constraint violation where the API reports a sentence
/// naming the key.
///
/// # Additive, and reversible without data loss for anyone who never set it
///
/// `ADD COLUMN` on a table that has at most one row. `down` drops the column,
/// which discards a configured floor — acceptable for an operational setting
/// whose absence is a well-defined state (no floor), and symmetric with how the
/// table's creating migration drops the whole thing.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            ALTER TABLE airway_deployment_config
                ADD COLUMN IF NOT EXISTS cursor_lag_floor_secs BIGINT;
        "#,
            )
            .await?;
        // Separate statement, and `DO $$` rather than a bare `ADD CONSTRAINT`:
        // `ADD COLUMN IF NOT EXISTS` is re-runnable but `ADD CONSTRAINT` is
        // not, so a re-applied migration would fail on the constraint alone.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
            DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_constraint
                    WHERE conname = 'airway_deployment_config_cursor_lag_floor_secs_check'
                ) THEN
                    ALTER TABLE airway_deployment_config
                        ADD CONSTRAINT airway_deployment_config_cursor_lag_floor_secs_check
                        CHECK (cursor_lag_floor_secs IS NULL OR cursor_lag_floor_secs >= 0);
                END IF;
            END $$;
        "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE airway_deployment_config DROP COLUMN IF EXISTS cursor_lag_floor_secs",
            )
            .await?;
        Ok(())
    }
}
