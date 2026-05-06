//! Env-driven Airhouse runtime config + factory helpers.
//!
//! Loads the four `AIRHOUSE_*` env vars into a [`AirhouseConfig`] tri-state
//! (Enabled / Disabled / Misconfigured) and provides factory functions for
//! constructing a [`crate::TenantProvisioner`] / [`crate::UserProvisioner`]
//! when the integration is enabled.

use std::sync::OnceLock;
use uuid::Uuid;

use crate::admin::AirhouseAdminClient;
use crate::provisioner::TenantProvisioner;
use crate::user_provisioner::UserProvisioner;

// ── Env var names ─────────────────────────────────────────────────────────────

pub const AIRHOUSE_BASE_URL_VAR: &str = "AIRHOUSE_BASE_URL";
pub const AIRHOUSE_ADMIN_TOKEN_VAR: &str = "AIRHOUSE_ADMIN_TOKEN";
pub const AIRHOUSE_WIRE_HOST_VAR: &str = "AIRHOUSE_WIRE_HOST";
pub const AIRHOUSE_WIRE_PORT_VAR: &str = "AIRHOUSE_WIRE_PORT";

// ── Local-mode constants ──────────────────────────────────────────────────────

/// Well-known nil-UUID organization id used in local mode. Mirrors the
/// frontend `LOCAL_ORG_ID` constant. The local-mode startup seeder creates a
/// row at this id so `airhouse_tenants` / `airhouse_users` FKs are satisfied
/// and the provisioner's membership check passes for the local guest user.
pub const LOCAL_ORG_ID: Uuid = Uuid::nil();

const DEFAULT_WIRE_PORT: u16 = 5445;

/// Names of all required Airhouse env vars. Used in error messages.
pub const REQUIRED_VARS: &[&str] = &[
    AIRHOUSE_BASE_URL_VAR,
    AIRHOUSE_ADMIN_TOKEN_VAR,
    AIRHOUSE_WIRE_HOST_VAR,
    AIRHOUSE_WIRE_PORT_VAR,
];

// ── Runtime config ────────────────────────────────────────────────────────────

/// Required fields when the Airhouse integration is active.
///
/// S3 bucket + prefix are no longer configured here — Airhouse owns storage
/// internally (see `[storage]` in `airhouse.toml`). The bucket and prefix
/// values returned by the Admin API are persisted on the local
/// `airhouse_tenants` row from the response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AirhouseRuntimeConfig {
    pub base_url: String,
    pub admin_token: String,
    pub wire_host: String,
    pub wire_port: u16,
}

/// Three-state config for the Airhouse integration.
///
/// - `Enabled` — all required vars present and non-empty.
/// - `Disabled` — none of the required vars are set; integration is off.
/// - `Misconfigured` — at least one required var is set but the full set is
///   incomplete. Callers should surface this as a startup error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AirhouseConfig {
    Enabled(AirhouseRuntimeConfig),
    Disabled,
    Misconfigured,
}

static CACHED_CONFIG: OnceLock<AirhouseConfig> = OnceLock::new();

impl AirhouseConfig {
    /// Load from environment. Re-reads env vars every call; safe for tests.
    pub fn from_env() -> Self {
        let base_url = std::env::var(AIRHOUSE_BASE_URL_VAR).ok();
        let admin_token = std::env::var(AIRHOUSE_ADMIN_TOKEN_VAR).ok();
        let wire_host = std::env::var(AIRHOUSE_WIRE_HOST_VAR).ok();
        let wire_port_raw = std::env::var(AIRHOUSE_WIRE_PORT_VAR).ok();

        let any_set = [
            base_url.as_deref(),
            admin_token.as_deref(),
            wire_host.as_deref(),
            wire_port_raw.as_deref(),
        ]
        .iter()
        .any(|v| v.is_some_and(|s| !s.is_empty()));

        if !any_set {
            return Self::Disabled;
        }

        let (Some(base_url), Some(admin_token), Some(wire_host)) = (
            base_url.filter(|s| !s.is_empty()),
            admin_token.filter(|s| !s.is_empty()),
            wire_host.filter(|s| !s.is_empty()),
        ) else {
            return Self::Misconfigured;
        };

        let wire_port = wire_port_raw
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<u16>().unwrap_or_else(|_| {
                    tracing::warn!(
                        "{AIRHOUSE_WIRE_PORT_VAR} value {s:?} is not a valid port number; \
                         falling back to default {DEFAULT_WIRE_PORT}"
                    );
                    DEFAULT_WIRE_PORT
                })
            })
            .unwrap_or(DEFAULT_WIRE_PORT);

        let base_url = base_url.trim_end_matches('/').to_string();

        Self::Enabled(AirhouseRuntimeConfig {
            base_url,
            admin_token,
            wire_host,
            wire_port,
        })
    }

    /// Cached form of `from_env` — reads env vars once on the first call.
    pub fn cached() -> &'static AirhouseConfig {
        CACHED_CONFIG.get_or_init(Self::from_env)
    }

    pub fn as_runtime(&self) -> Option<&AirhouseRuntimeConfig> {
        match self {
            Self::Enabled(c) => Some(c),
            _ => None,
        }
    }

    pub fn into_runtime(self) -> Option<AirhouseRuntimeConfig> {
        match self {
            Self::Enabled(c) => Some(c),
            _ => None,
        }
    }
}

// ── Wire endpoint ─────────────────────────────────────────────────────────────

/// User-facing wire-protocol coordinates exposed by the deployment.
#[derive(Debug, Clone)]
pub struct WireEndpoint {
    pub host: String,
    pub port: u16,
}

/// Resolve the user-facing wire-protocol connection coordinates from config.
pub fn wire_endpoint() -> Option<WireEndpoint> {
    let cfg = AirhouseConfig::cached().as_runtime()?;
    Some(WireEndpoint {
        host: cfg.wire_host.clone(),
        port: cfg.wire_port,
    })
}

// ── Factory functions ─────────────────────────────────────────────────────────

/// Build a `TenantProvisioner` for the given DB connection if Airhouse is enabled.
/// Returns `None` when the integration is disabled or misconfigured — call sites
/// should treat that as "skip silently".
pub fn provisioner_for(db: sea_orm::DatabaseConnection) -> Option<TenantProvisioner> {
    let cfg = AirhouseConfig::cached().as_runtime()?.clone();
    let client = AirhouseAdminClient::new(cfg.base_url.clone(), cfg.admin_token.clone());
    Some(TenantProvisioner::new(db, client))
}

/// Build a `UserProvisioner` for the given DB connection if Airhouse is enabled.
pub fn user_provisioner_for(db: sea_orm::DatabaseConnection) -> Option<UserProvisioner> {
    let cfg = AirhouseConfig::cached().as_runtime()?;
    let client = AirhouseAdminClient::new(cfg.base_url.clone(), cfg.admin_token.clone());
    Some(UserProvisioner::new(db, client))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_clean_env<F: FnOnce()>(f: F) {
        let _g = ENV_LOCK.lock().unwrap();
        for k in [
            AIRHOUSE_BASE_URL_VAR,
            AIRHOUSE_ADMIN_TOKEN_VAR,
            AIRHOUSE_WIRE_HOST_VAR,
            AIRHOUSE_WIRE_PORT_VAR,
        ] {
            unsafe { std::env::remove_var(k) };
        }
        f();
    }

    #[test]
    fn disabled_when_nothing_set() {
        with_clean_env(|| {
            assert_eq!(AirhouseConfig::from_env(), AirhouseConfig::Disabled);
        });
    }

    #[test]
    fn misconfigured_when_partial() {
        with_clean_env(|| {
            unsafe { std::env::set_var(AIRHOUSE_BASE_URL_VAR, "http://airhouse:8080") };
            // admin_token + wire_host missing
            assert_eq!(AirhouseConfig::from_env(), AirhouseConfig::Misconfigured);
        });
    }

    #[test]
    fn enabled_with_defaults_when_all_required_present() {
        with_clean_env(|| {
            unsafe {
                std::env::set_var(AIRHOUSE_BASE_URL_VAR, "http://airhouse:8080/");
                std::env::set_var(AIRHOUSE_ADMIN_TOKEN_VAR, "secret");
                std::env::set_var(AIRHOUSE_WIRE_HOST_VAR, "airhouse");
            }
            let cfg = AirhouseConfig::from_env()
                .into_runtime()
                .expect("should be Enabled");
            assert_eq!(cfg.base_url, "http://airhouse:8080"); // trailing slash trimmed
            assert_eq!(cfg.wire_port, DEFAULT_WIRE_PORT);
        });
    }

    #[test]
    fn enabled_with_explicit_port() {
        with_clean_env(|| {
            unsafe {
                std::env::set_var(AIRHOUSE_BASE_URL_VAR, "http://airhouse:8080");
                std::env::set_var(AIRHOUSE_ADMIN_TOKEN_VAR, "secret");
                std::env::set_var(AIRHOUSE_WIRE_HOST_VAR, "airhouse");
                std::env::set_var(AIRHOUSE_WIRE_PORT_VAR, "9000");
            }
            let cfg = AirhouseConfig::from_env()
                .into_runtime()
                .expect("should be Enabled");
            assert_eq!(cfg.wire_port, 9000);
        });
    }
}
