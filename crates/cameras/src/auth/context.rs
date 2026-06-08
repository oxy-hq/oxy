//! Per-request edge context populated by the device-token middleware.
//!
//! Every authenticated `/control/*` handler can rely on these four
//! UUIDs being present: which edge box is calling, which site it
//! belongs to, which workspace owns the site, and which device_claims
//! row authorised the request (for per-device audit correlation).

use sea_orm::prelude::Uuid;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct EdgeContext {
    pub edge_box_id: Uuid,
    pub site_id: Uuid,
    /// Loose cross-aggregate ref — the Workspace that owns this fleet.
    /// Used by the ingest path to pick the right Airhouse tenant.
    pub workspace_id: Uuid,
    /// Which `device_claims` row authorised the request. Same value
    /// for every `/control/*` call from this device until the claim
    /// is revoked or rotated; useful for correlating per-device
    /// activity in audit / debug logs.
    pub claim_id: Uuid,
}
