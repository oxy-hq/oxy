//! What each integration calls a location.
//!
//! The app keys on its own location; Toast keys on a restaurant GUID; a
//! camera console on a site id; payroll on a cost centre. This is the one
//! table that says they are the same place — the seam the semantic model
//! binds to and the reason a sales figure can land on "Clovis" rather than on
//! `a1b2c3…`. Promoted from a custom app's own schema on 2026-09-07; see
//! `internal-docs/operating-graph.md`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "location_external_ids")]
pub struct Model {
    #[sea_orm(indexed)]
    pub org_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub location_id: Uuid,
    /// A lowercase token the tenant's integrations agree on: `toast`,
    /// `momos`, `unifi`, `payroll`.
    #[sea_orm(primary_key, auto_increment = false)]
    pub system: String,
    pub external_id: String,
    pub set_by: Option<Uuid>,
    pub set_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}

/// The shape a `system` token must have. Lowercase, so `Toast` and `toast`
/// cannot become two systems; short, so it reads as a key and not a sentence.
pub fn is_valid_system(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::is_valid_system;

    #[test]
    fn a_system_is_a_lowercase_token() {
        for ok in ["toast", "momos", "unifi", "payroll_v2", "pos-2"] {
            assert!(is_valid_system(ok), "{ok}");
        }
        for bad in ["", "Toast", "toast pos", "toast/pos", &"x".repeat(33)] {
            assert!(!is_valid_system(bad), "{bad:?}");
        }
    }
}
