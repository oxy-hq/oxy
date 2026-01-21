//! ClickHouse migration system for observability data
//!
//! Since SeaORM doesn't support ClickHouse, we implement a custom migration
//! system that follows similar patterns to SeaORM migrations.

mod m20251228_000001_create_intent_tables;
mod m20260112_000001_add_agent_ref;
mod m20260115_000001_create_metric_usage;
mod migrator;

pub use migrator::{ClickHouseMigration, ClickHouseMigrator};

/// Get the ClickHouse migrator with all migrations
pub fn get_clickhouse_migrator() -> ClickHouseMigrator {
    use m20251228_000001_create_intent_tables::CreateIntentTables;
    use m20260112_000001_add_agent_ref::AddSourceFieldsToIntentClassifications;
    use m20260115_000001_create_metric_usage::CreateMetricUsage;

    ClickHouseMigrator::new()
        .add_migration(Box::new(CreateIntentTables))
        .add_migration(Box::new(AddSourceFieldsToIntentClassifications))
        .add_migration(Box::new(CreateMetricUsage))
}
