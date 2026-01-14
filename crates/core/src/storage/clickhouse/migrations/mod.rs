//! ClickHouse migration system for observability data
//!
//! Since SeaORM doesn't support ClickHouse, we implement a custom migration
//! system that follows similar patterns to SeaORM migrations.

mod m20251228_000001_create_intent_tables;
mod m20260112_000001_add_agent_ref;
mod migrator;

pub use migrator::{ClickHouseMigration, ClickHouseMigrator};

/// Get the ClickHouse migrator with all migrations
pub fn get_clickhouse_migrator() -> ClickHouseMigrator {
    use m20251228_000001_create_intent_tables::CreateIntentTables;
    use m20260112_000001_add_agent_ref::AddSourceFieldsToIntentClassifications;

    ClickHouseMigrator::new()
        .add_migration(Box::new(CreateIntentTables))
        .add_migration(Box::new(AddSourceFieldsToIntentClassifications))
}
