//! Shim: partner authorization now lives in the [`oxy_server_authz`] crate.
//!
//! Re-exported here so call sites keep using
//! `crate::server::api::middlewares::partner_authz::{PartnerCapability, PartnerScope,
//! resolve_scope, …}` unchanged.

pub use oxy_server_authz::partner_authz::*;
