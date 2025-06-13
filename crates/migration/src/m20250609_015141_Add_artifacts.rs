use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .create_table(
                Table::create()
                    .table(Artifacts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Artifacts::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Artifacts::Content).json().not_null())
                    .col(ColumnDef::new(Artifacts::Kind).string().not_null())
                    .col(ColumnDef::new(Artifacts::MessageId).uuid().not_null())
                    .col(ColumnDef::new(Artifacts::ThreadId).uuid().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-artifacts-threads-id")
                            .from(Artifacts::Table, Artifacts::ThreadId)
                            .to(Threads::Table, Threads::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-artifacts-messages-id")
                            .from(Artifacts::Table, Artifacts::MessageId)
                            .to(Messages::Table, Messages::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(Artifacts::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp().to_owned()),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .drop_table(Table::drop().table(Artifacts::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Artifacts {
    Table,
    Id,
    Content,
    Kind,
    ThreadId,
    MessageId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Threads {
    Table,
    Id,
}
#[derive(DeriveIden)]
enum Messages {
    Table,
    Id,
}
