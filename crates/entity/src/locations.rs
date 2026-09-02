//! A physical place work happens at.
//!
//! The axis every operational record hangs off, and the unit a multi-unit
//! operator actually thinks in. Promoted out of app-local tables because five
//! product surfaces need to filter on it and each would otherwise keep its own.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "locations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub org_id: Uuid,
    pub name: String,
    /// `pre_launch` | `launching` | `open` | `archived` | `terminated`.
    ///
    /// Lifecycle rather than a boolean: an operator's roster is mostly NOT open
    /// stores, and the launching ones are where the work is.
    pub status: String,
    /// Work is due at a LOCAL time. Storing the zone per location is what stops
    /// "due by close" meaning 23:00 UTC at a store two time zones away.
    pub timezone: String,
    /// The tenant's own identifier for this store, when they have one. Carried
    /// so an import can be re-run without duplicating rows.
    pub external_id: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    #[sea_orm(has_many)]
    pub work_items: HasMany<super::work_items::Entity>,
}

impl Model {
    /// Is this a store that can be assigned day-to-day work?
    ///
    /// `launching` counts: a store being built has an opening playbook running
    /// against it, and that is exactly when the work is densest.
    pub fn accepts_work(&self) -> bool {
        matches!(self.status.as_str(), "pre_launch" | "launching" | "open")
    }
}

impl ActiveModelBehavior for ActiveModel {}
