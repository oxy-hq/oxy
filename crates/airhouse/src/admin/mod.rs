//! Airhouse Admin API HTTP client.
//!
//! Wraps the `/admin/v1/{tenants,users}` endpoints exposed by an Airhouse
//! deployment. Used by the tenant + per-user provisioners.

mod client;
mod error;
mod types;

pub use client::AirhouseAdminClient;
pub use error::AirhouseError;
pub use types::{TenantRecord, UserRecord, UserRole};
