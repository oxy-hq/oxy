pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20241111_110133_add_agent_to_conversation;
mod m20241112_035850_add_message;
mod m20250307_090813_add_threads;
mod m20250318_230139_add_thread_references;
mod m20250501_215840_add_tasks;
mod m20250519_011103_add_workflow_to_threads;
mod m20250523_123859_add_users_table;
mod m20250523_123900_add_user_id_to_threads;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20241111_110133_add_agent_to_conversation::Migration),
            Box::new(m20241112_035850_add_message::Migration),
            Box::new(m20250307_090813_add_threads::Migration),
            Box::new(m20250318_230139_add_thread_references::Migration),
            Box::new(m20250501_215840_add_tasks::Migration),
            Box::new(m20250519_011103_add_workflow_to_threads::Migration),
            Box::new(m20250523_123859_add_users_table::Migration),
            Box::new(m20250523_123900_add_user_id_to_threads::Migration),
        ]
    }
}
