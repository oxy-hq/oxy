//! Customer-owned domains for a custom app, over HTTP: the staff console's
//! routes ([`admin`]) and the sign-in handoff plus the TLS edge's question
//! ([`handoff`]). The rules live in `oxy_custom_apps::custom_domains`; how a
//! request on such a domain reaches its app is `oxy_app_core::custom_domain_host`.
//! Operator guide: `internal-docs/customer-apps.md` § Custom domains.

pub mod admin;
pub mod handoff;
