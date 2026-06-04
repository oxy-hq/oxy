//! Business logic for the camera fleet aggregate.
//!
//! Each submodule covers one operational surface. Route handlers in
//! `crates/cameras/src/routes/` are thin parsing + auth wrappers around
//! these functions.
//!
//! ## Modules
//!
//! - [`registration`] — operator pre-registers an edge box for a site;
//!   service issues a fresh device token and persists `(edge_box,
//!   edge_box_token)`. The plaintext token is returned **once** to be
//!   handed to the installer.
//! - [`config`] — edge-side: read assigned cameras + zones. Operator-
//!   side: PATCH zones / lines for a camera.
//! - [`onboarding`] — UniFi-specific. Preview and import a customer's
//!   fleet from `api.ui.com` into our `sites` / `edge_boxes` / `cameras`
//!   tables. Uses `oxy_unifi::UnifiClient`.
//! - [`ingest`] — write per-frame events + health rows to Airhouse via
//!   the SA broker (`SystemPurpose::EdgeIngest`). **Stub** until step 10
//!   lands the per-tenant Airhouse DDL.
//! - [`compliance`] — VLM compliance report ingest. Same shape as
//!   [`ingest`]; **stub** until step 10.

pub mod agreement;
pub mod alerts;
pub mod arbitrations;
pub mod audit;
pub mod budget;
pub mod camera_health;
pub mod clips;
pub mod compliance;
pub mod config;
pub mod cost;
pub mod dashboard;
pub mod fleet;
pub mod ingest;
pub mod listing;
pub mod log_retention;
pub mod logs;
pub mod onboarding;
pub mod packs;
pub mod preview;
pub mod pricing;
pub mod provisioning;
pub mod registration;
pub mod rollouts;
pub mod sites;
pub mod stale;
pub mod unifi_credentials;
pub mod updates;
pub mod webrtc;

use thiserror::Error;

/// Errors returned by service-layer functions. Route handlers translate
/// these into HTTP status codes.
#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("not found")]
    NotFound,

    /// A unique-constraint violation or business-rule conflict (e.g.
    /// camera with the same `(site_id, name)` already exists).
    #[error("conflict: {0}")]
    Conflict(String),

    /// Caller-supplied input failed validation.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The resource exists but belongs to a different workspace than the
    /// caller has access to. Translated to 403 Forbidden by the route
    /// layer (NOT 404 — 404 would leak the existence of cross-workspace
    /// resources to a probing caller, but a deliberate write-attempt
    /// gets the more honest "you have no business touching this" signal).
    #[error("forbidden: {0}")]
    Forbidden(&'static str),

    /// Caller's request is valid but the target system isn't reachable
    /// or hasn't completed setup. Translated to 503 Service Unavailable —
    /// matches the Airhouse-disabled mapping. Used by the preview proxy
    /// when an edge box has no `tailscale_ip` or a camera isn't yet
    /// bound to a worker.
    #[error("unavailable: {0}")]
    Unavailable(&'static str),

    /// Upstream HTTP call to an edge box failed (network, timeout, 5xx).
    /// Route layer maps to 502 Bad Gateway — caller's request was
    /// well-formed but the downstream service errored.
    #[error("upstream error: {0}")]
    Upstream(String),

    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("UniFi API error: {0}")]
    Unifi(#[from] oxy_unifi::UnifiError),

    /// Failure on the Airhouse path — mint, connect, DDL, INSERT, or
    /// SELECT. The route layer surfaces these as 502 Bad Gateway: the
    /// caller's input was fine but our downstream errored.
    #[error("airhouse error: {0}")]
    Airhouse(#[from] crate::airhouse::AirhouseError),

    /// Reserved for future paths that depend on infrastructure not yet
    /// in place.
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),

    /// Catch-all for server-side failures that aren't a database or
    /// upstream issue — encryption layer failures, malformed sealed
    /// blobs, etc. Translated to 500 Internal Server Error.
    #[error("internal error: {0}")]
    Internal(String),
}

pub type ServiceResult<T> = Result<T, ServiceError>;
