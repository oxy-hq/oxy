use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agentic_run_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub run_id: String,
    /// Monotonic per-run counter — used as the SSE `id:` field for catch-up.
    pub seq: i64,
    pub event_type: String,
    pub payload: Json,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::agentic_run::Entity",
        from = "Column::RunId",
        to = "super::agentic_run::Column::Id",
        on_delete = "Cascade"
    )]
    Run,
}

impl Related<super::agentic_run::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Run.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
