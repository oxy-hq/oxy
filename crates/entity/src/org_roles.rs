//! A tenant-defined role.
//!
//! `org_members.role` is a three-value enum. A real operator invents its own
//! vocabulary — eight roles split across "works at a location" and "works at
//! head office" — and expects it to mean something.
//!
//! **This is not an authorization principal.** `oxy-authz` still decides what a
//! person may do; this decides what they are CALLED and what work routes to
//! them. Conflating the two is how a display label silently becomes a
//! permission, which is a class of bug that does not announce itself.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "org_roles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub org_id: Uuid,
    pub name: String,
    /// `location` — held at one store. `franchisor` — held across the org.
    pub scope: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl Model {
    /// A franchisor-scope role is held across the org, so its membership rows
    /// carry no location. Callers use this rather than testing the string, so
    /// adding a third scope is one match arm rather than a grep.
    pub fn is_location_scoped(&self) -> bool {
        self.scope == "location"
    }
}

impl ActiveModelBehavior for ActiveModel {}
