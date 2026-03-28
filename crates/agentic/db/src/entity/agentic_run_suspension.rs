use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agentic_run_suspensions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub run_id: String,
    pub prompt: String,
    /// JSON array of suggestion strings.
    pub suggestions: Json,
    /// Serialized `SuspendedRunData` — fed back to `Orchestrator::resume`.
    pub resume_data: Json,
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
