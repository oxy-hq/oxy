use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "run_sequences")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub project_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub branch_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub source_id: String,
    pub last_value: i32,
}

impl ActiveModelBehavior for ActiveModel {}
