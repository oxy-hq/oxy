use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Internal deserialization type that includes the pg_url field returned by Airhouse.
/// Never expose this type or the pg_url value in API responses.
#[derive(Deserialize)]
pub(crate) struct TenantRecordRaw {
    pub id: String,
    pub pg_url: String,
    pub bucket: String,
    pub prefix: Option<String>,
    pub role: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// A provisioned Airhouse tenant.
///
/// The `pg_url` field is intentionally private. Access it only in internal worker
/// code that needs to connect to the DuckLake catalog directly. Never expose it in
/// API responses or user-facing surfaces — end users connect via the wire-protocol
/// port with their own credentials.
#[derive(Clone)]
pub struct TenantRecord {
    pub id: String,
    pub bucket: String,
    pub prefix: Option<String>,
    pub role: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pg_url: String,
}

impl TenantRecord {
    /// Returns the internal DuckLake connection URL.
    ///
    /// **Internal use only.** Never expose this in API responses or user-facing surfaces.
    /// End users connect via the wire-protocol port (`host`, `port`, `dbname`, `user`, `password`).
    pub fn pg_url(&self) -> &str {
        &self.pg_url
    }
}

impl fmt::Debug for TenantRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TenantRecord")
            .field("id", &self.id)
            .field("bucket", &self.bucket)
            .field("prefix", &self.prefix)
            .field("role", &self.role)
            .field("status", &self.status)
            .field("created_at", &self.created_at)
            .field("pg_url", &"[redacted]")
            .finish()
    }
}

impl From<TenantRecordRaw> for TenantRecord {
    fn from(raw: TenantRecordRaw) -> Self {
        Self {
            id: raw.id,
            bucket: raw.bucket,
            prefix: raw.prefix,
            role: raw.role,
            status: raw.status,
            created_at: raw.created_at,
            pg_url: raw.pg_url,
        }
    }
}

/// Role granted to an Airhouse tenant user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Reader,
    Writer,
    Admin,
}

/// A user within an Airhouse tenant.
#[derive(Debug, Clone, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub tenant_id: String,
    pub username: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}
