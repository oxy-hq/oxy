use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // ── world_model_configs (.world-model.yml) ────────────────────────────
        // Singleton per revision — one `.world-model.yml` per workspace, so
        // the PK is just `revision_id`. The full payload (top-level
        // `entities`) lives in a single JSONB column; runtime round-trips
        // it back into the strict-typed WorldModelConfig.
        //
        // Split into its own migration (rather than appended to
        // `m20260606_000002_create_compile_boundary`) so that databases which
        // already applied the compile-boundary migration still receive this
        // table — SeaORM skips `up()` for an already-recorded migration name.
        manager
            .create_table(
                Table::create()
                    .table(WorldModelConfigs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorldModelConfigs::RevisionId)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorldModelConfigs::Definition)
                            .json_binary()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_world_model_configs_revision")
                            .from(WorldModelConfigs::Table, WorldModelConfigs::RevisionId)
                            .to(Revisions::Table, Revisions::RevisionId)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(WorldModelConfigs::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Revisions {
    Table,
    RevisionId,
}

#[derive(DeriveIden)]
enum WorldModelConfigs {
    Table,
    RevisionId,
    Definition,
}
