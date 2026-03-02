use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .table(Users::Table)
                    .name("idx_users_email_verification_token")
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::EmailVerificationToken)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        use sea_orm_migration::schema::string_null;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(string_null(Users::EmailVerificationToken))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(Users::Table)
                    .name("idx_users_email_verification_token")
                    .col(Users::EmailVerificationToken)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    EmailVerificationToken,
}
