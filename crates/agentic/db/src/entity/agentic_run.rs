use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agentic_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub agent_id: String,
    pub question: String,
    /// `running` | `suspended` | `done` | `failed`
    pub status: String,
    pub answer: Option<String>,
    pub error_message: Option<String>,
    /// FK → threads(id); set when the run is initiated from a thread.
    pub thread_id: Option<Uuid>,
    /// Serialized `QueryRequestItem` from the completed run's spec.
    /// Used to seed `AnalyticsIntent.spec_hint` on follow-up questions.
    pub spec_hint: Option<serde_json::Value>,
    /// Thinking mode used for this run (`"auto"` or `"extended_thinking"`).
    pub thinking_mode: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::agentic_run_event::Entity")]
    RunEvents,
    #[sea_orm(has_one = "super::agentic_run_suspension::Entity")]
    Suspension,
}

impl Related<super::agentic_run_event::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RunEvents.def()
    }
}

impl Related<super::agentic_run_suspension::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Suspension.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
