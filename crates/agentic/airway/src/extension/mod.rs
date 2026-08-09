//! Airway extension entities + migrator.
//!
//! Four aggregate roots managed by a single `AirwayMigrator`
//! (tracking table `seaql_migrations_airway`):
//!
//! - [`pipeline_state`] — `airway_pipeline_state` per pipeline_name
//! - [`load_audit`] — `airway_load_audit` per load_id
//! - [`run_extension`] — `airway_run_extensions` per `agentic_runs.id`
//! - [`pipeline_lease`] — `airway_pipeline_leases` per
//!   `(workspace_id, pipeline_name)`; the single-flight guard that keeps two
//!   runs of one pipeline from racing the cursor row and the end-of-load fold
//!
//! Cross-aggregate refs (e.g. `airway_run_extensions.load_id` ->
//! `airway_load_audit.load_id`) are loose UUIDs — no DB FK to other
//! aggregates.

pub mod load_audit;
pub mod migration;
pub mod pipeline_lease;
pub mod pipeline_state;
pub mod run_extension;

pub use migration::AirwayMigrator;
