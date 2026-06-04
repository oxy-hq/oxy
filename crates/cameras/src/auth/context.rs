//! Per-request edge context populated by the device-token middleware.
//!
//! Every authenticated `/control/*` handler can rely on these four
//! UUIDs being present: which edge box is calling, which site it
//! belongs to, which workspace owns the site, and which token row
//! authenticated the request (for last-used tracking + revocation
//! audit).

use sea_orm::prelude::Uuid;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct EdgeContext {
    pub edge_box_id: Uuid,
    pub site_id: Uuid,
    /// Loose cross-aggregate ref — the Workspace that owns this fleet.
    /// Used by the ingest path to pick the right Airhouse tenant.
    pub workspace_id: Uuid,
    /// Which `edge_box_tokens` row authenticated the request.
    pub token_id: Uuid,
    /// First 8 chars of the bearer — included so handlers can log
    /// "edge sim-abc12345 made request X" without revealing the secret.
    pub token_prefix: String,
}
