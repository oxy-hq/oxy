//! A custom-app draft build's **semantic pin** — the compiled revision its
//! staging requests read (`internal-docs/customer-apps-staging.md` D4,
//! mechanics in `internal-docs/compile-boundary.md` § "Staging revisions").
//!
//! The pin logic lives below `oxy-app`, and every name stays reachable here:
//!
//! * `oxy_custom_apps::staging_pin` — read a build's pin
//!   ([`pinned_revision_for`]), validate one at publish
//!   ([`validate_pin_for_publish`], [`check_pinnable`]), resolve a request's
//!   pin from its app and environment, and the promote notice ([`pin_drift`]);
//! * `oxy_app_core::staging_pin_scope` — run a future under a pin
//!   ([`with_staging_pin`]) and read it back;
//! * [`request`] — here, because it reads an axum `HeaderMap`: decide whether
//!   a browser data-plane request (`/api/projects/{id}/…`) is a staging
//!   request for an app, and return that app's pin.
//!
//! **The live channel ignores pins.** Nothing here is consulted for a request
//! that is not a staging request (the app's staging host, for a viewer who may
//! open it), so a promoted build always reads
//! `workspaces.current_revision_id` and promote stays a pointer move.

pub mod request;

pub use oxy_app_core::staging_pin_scope::{cache_partition, current_staging_pin, with_staging_pin};
pub use oxy_custom_apps::staging_pin::{
    PinRefusal, SemanticPinNotice, check_pinnable, pin_drift, pinned_revision_for,
    validate_pin_for_publish,
};
pub use request::{APP_HEADER, staging_pin_for_data_request};
