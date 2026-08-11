use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One periodic measurement of an app's silo size.
///
/// Exists for two questions the current-state rollup cannot answer:
///
/// * **GB-month** — the billing primitive is the *mean* of these samples over a
///   period, not the peak and not the end-of-period value. Billing peak would
///   punish an app that writes a 5 GB export and deletes it an hour later.
///   (Vercel Blob snapshots every 15 minutes and averages; same idea, cheaper
///   cadence.)
/// * **Growth rate** — differencing two samples gives the Δ/week that actually
///   predicts the next invoice, which a single current number never can.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "app_storage_usage_samples")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub app_id: Uuid,
    /// Second half of the composite key: a repeat measurement for the same
    /// instant overwrites rather than double-counting into the mean.
    #[sea_orm(primary_key, auto_increment = false)]
    pub measured_at: DateTimeWithTimeZone,
    pub bytes: i64,
    pub object_count: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::apps::Entity",
        from = "Column::AppId",
        to = "super::apps::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Apps,
}

impl Related<super::apps::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Apps.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
