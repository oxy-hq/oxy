//! Reset a pipeline's provisioned schema.
//!
//! Three small, independently-composable steps the `agentic-pipeline`
//! executor stitches together to wipe a pipeline back to a clean slate:
//! read the stored schema's table names, drop those tables at the
//! destination, then delete the `airway_pipeline_state` row (its
//! `PipelineState` cursors + `Schema` + version). A later run then
//! re-infers a fresh schema from scratch.
//!
//! Kept next to [`crate::state_store`] and [`crate::destination_factory`]
//! because all three lean on this crate's view of the airway engine
//! ([`StateStore`], [`airway::destination::Destination`]) plus the SeaORM
//! [`airway_pipeline_state`](crate::extension::pipeline_state) entity.

use std::sync::Arc;

use airway::state::StateStore;
use sea_orm::{DatabaseConnection, EntityTrait};

use crate::config::DestinationConfig;
use crate::destination_factory::CredentialProvider;
use crate::error::AirwayError;
use crate::extension::pipeline_state::Entity as PipelineStateEntity;
use crate::state_store::AirwayPgStateStore;

/// Table names in a pipeline's *stored* schema — the set a reset must drop.
///
/// Loads the pipeline's `airway_pipeline_state` row via
/// [`AirwayPgStateStore`] and returns its schema's table-name keys. A
/// pipeline that never provisioned (no row, so `StateSnapshot::default`'s
/// `schema: None`) yields an empty vec — nothing to drop.
pub async fn stored_schema_table_names(
    db: &DatabaseConnection,
    pipeline_name: &str,
) -> Result<Vec<String>, AirwayError> {
    // `AirwayPgStateStore::new` wants an owned `Arc<DatabaseConnection>`;
    // mirror the executor's `Arc::new(self.db.clone())` handoff.
    let store = AirwayPgStateStore::new(Arc::new(db.clone()), pipeline_name);
    let snapshot = store.load().await?;
    Ok(snapshot
        .schema
        .map(|s| s.tables.keys().cloned().collect())
        .unwrap_or_default())
}

/// Drop `tables` at the pipeline's destination.
///
/// Builds the concrete [`Destination`](airway::destination::Destination) from
/// `config` (threading the
/// optional airhouse credential provider so a managed destination re-mints
/// a fresh credential on connect) and issues a single `drop_tables`.
/// No-op-safe when `tables` is empty.
pub async fn drop_destination_tables(
    config: &DestinationConfig,
    provider: Option<Arc<dyn CredentialProvider>>,
    tables: &[String],
) -> Result<(), AirwayError> {
    if tables.is_empty() {
        return Ok(());
    }
    let dest = crate::destination_factory::build_destination(config, provider)?;
    dest.drop_tables(tables).await?;
    Ok(())
}

/// Delete the pipeline's `airway_pipeline_state` row, discarding its stored
/// `PipelineState` (incremental cursors), `Schema`, and version.
///
/// Idempotent — deleting an absent row is a no-op. The next run then starts
/// from a default snapshot and re-infers the schema.
pub async fn clear_pipeline_state(
    db: &DatabaseConnection,
    pipeline_name: &str,
) -> Result<(), AirwayError> {
    PipelineStateEntity::delete_by_id(pipeline_name.to_string())
        .exec(db)
        .await
        .map_err(|e| AirwayError::Other(format!("delete airway_pipeline_state: {e}")))?;
    Ok(())
}
