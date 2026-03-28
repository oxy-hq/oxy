use sea_orm::{DatabaseBackend, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rename_table_if_needed(manager, "agentic_run_suspension", "agentic_run_suspensions")
            .await?;
        rename_table_if_needed(manager, "agentic_run_event", "agentic_run_events").await?;
        rename_table_if_needed(manager, "agentic_run", "agentic_runs").await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rename_table_if_needed(manager, "agentic_run_suspensions", "agentic_run_suspension")
            .await?;
        rename_table_if_needed(manager, "agentic_run_events", "agentic_run_event").await?;
        rename_table_if_needed(manager, "agentic_runs", "agentic_run").await?;
        Ok(())
    }
}

async fn table_exists(manager: &SchemaManager<'_>, name: &str) -> Result<bool, DbErr> {
    let db = manager.get_connection();
    let result = db
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
            [name.into()],
        ))
        .await?;
    Ok(result
        .and_then(|row| row.try_get::<bool>("", "exists").ok())
        .unwrap_or(false))
}

async fn rename_table_if_needed(
    manager: &SchemaManager<'_>,
    from: &str,
    to: &str,
) -> Result<(), DbErr> {
    if table_exists(manager, to).await? || !table_exists(manager, from).await? {
        return Ok(());
    }
    manager
        .rename_table(
            Table::rename()
                .table(Alias::new(from), Alias::new(to))
                .to_owned(),
        )
        .await
}
