//! Airhouse integration for Oxy.
//!
//! Airhouse is a managed analytics warehouse that speaks the PostgreSQL wire
//! protocol but executes SQL in the DuckDB dialect. This crate owns everything
//! Oxy needs to talk to Airhouse:
//!
//! - **`connector`** feature — `AirhouseConnector` (pgwire transport, DuckDB
//!   dialect). Implements `agentic_connector::DatabaseConnector`. Pulls in
//!   only the trait crate + `tokio-postgres` so it is safe to enable from
//!   `agentic-pipeline` without breaking that crate's no-platform-deps rule.
//! - **`credentials`** feature — `airhouse_managed` credential resolver.
//!   Carved out as the minimum set `oxy` core needs to dispatch managed
//!   connections in its legacy `Connector::from_db` path. Pulls in only
//!   `oxy-platform`, `oxy-shared`, and `entity`.
//! - **`admin`** feature — implies `credentials`. Adds the admin HTTP client
//!   (`AirhouseAdminClient`), tenant + per-user provisioners, env-driven
//!   config loader, and the local-mode seeder. Adds `oxy-auth` (used by the
//!   seeder for the `Identity` type).
//! - **`rest`** feature — Axum handlers for `/airhouse/me/{connection,
//!   credentials, provision, rotate-password}`. Requires `admin`.

#[cfg(feature = "connector")]
pub mod connector;

#[cfg(feature = "credentials")]
pub mod credentials;

#[cfg(feature = "credentials")]
pub mod entity;

#[cfg(feature = "credentials")]
pub mod migration;

#[cfg(feature = "admin")]
pub mod admin;

#[cfg(feature = "admin")]
pub mod config;

#[cfg(feature = "admin")]
pub mod local_seed;

#[cfg(feature = "admin")]
pub mod provisioner;

#[cfg(feature = "admin")]
pub mod user_provisioner;

#[cfg(feature = "rest")]
pub mod api;

// ── Re-exports ────────────────────────────────────────────────────────────────

#[cfg(feature = "connector")]
pub use connector::AirhouseConnector;

#[cfg(feature = "credentials")]
pub use credentials::{ManagedAirhouseCreds, resolve_managed_airhouse_credentials};

#[cfg(feature = "admin")]
pub use admin::{AirhouseAdminClient, AirhouseError, TenantRecord, UserRecord, UserRole};

#[cfg(feature = "admin")]
pub use config::{
    AIRHOUSE_ADMIN_TOKEN_VAR, AIRHOUSE_BASE_URL_VAR, AIRHOUSE_WIRE_HOST_VAR,
    AIRHOUSE_WIRE_PORT_VAR, AirhouseConfig, AirhouseRuntimeConfig, LOCAL_ORG_ID, REQUIRED_VARS,
    WireEndpoint, provisioner_for, user_provisioner_for, wire_endpoint,
};

#[cfg(feature = "admin")]
pub use local_seed::ensure_local_org_seeded;

#[cfg(feature = "admin")]
pub use provisioner::{ProvisionerError, TenantProvisioner};

#[cfg(feature = "admin")]
pub use user_provisioner::{ProvisionedUser, UserProvisioner, UserProvisionerError};
