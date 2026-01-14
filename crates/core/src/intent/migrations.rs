//! ClickHouse migration system for intent classification

mod m20251228_000001_create_intent_tables;
mod m20260112_000001_add_agent_ref;
pub mod migrator;

pub use migrator::{ClickHouseMigration, ClickHouseMigrator};

/// Get the ClickHouse migrator with all migrations
pub fn get_clickhouse_migrator() -> ClickHouseMigrator {
    use m20251228_000001_create_intent_tables::CreateIntentTables;
    use m20260112_000001_add_agent_ref::AddSourceFieldsToIntentClassifications;

    ClickHouseMigrator::new()
        .add_migration(Box::new(CreateIntentTables))
        .add_migration(Box::new(AddSourceFieldsToIntentClassifications))
}
