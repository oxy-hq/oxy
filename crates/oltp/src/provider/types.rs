//! Wire types, shaped after the Neon REST API.
//!
//! Field names follow Neon's JSON so the real client can deserialise straight
//! into these. Where Neon returns more than Oxy needs, the surplus is dropped
//! rather than modelled.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    /// Provider-visible project name. Derived from the org, and unique —
    /// a collision is [`super::ProviderError::ProjectNameTaken`], never a
    /// silent adoption.
    pub name: String,
    /// Provider region id, e.g. `aws-us-east-2`.
    pub region_id: String,
    /// Major Postgres version.
    pub pg_version: u8,
}

/// A provisioned per-org Postgres.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub region_id: String,
    pub pg_version: u8,
    /// The default branch. Oxy pins to this one; per-branch databases (the
    /// mapping onto Oxy's git branches) are a later slice.
    pub branch: Branch,
    pub database: DatabaseInfo,
    /// Role that owns the database. Its password is disclosed only at project
    /// creation.
    pub owner_role: Role,
    /// Hostname clients connect to.
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub owner_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    pub name: String,
    /// Disclosed **once**, at create or reset. `None` on reads.
    ///
    /// Losing it means the role must be reset, not recovered — the provider
    /// stores only a hash. Seal it before this value goes out of scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl Role {
    /// Drop the password, for logging or for persisting a row that must not
    /// carry a plaintext secret.
    pub fn redacted(&self) -> Role {
        Role {
            name: self.name.clone(),
            password: None,
        }
    }
}

impl Project {
    /// Postgres DSN for a given role. Used to run schema/role DDL and, later,
    /// to hand a writer its connection.
    pub fn dsn_for(&self, role_name: &str, password: &str) -> String {
        format!(
            "postgres://{role_name}:{password}@{host}/{db}?sslmode=require",
            // Encoded: a provider-issued password may contain URI-special
            // characters, and an unescaped `@` silently becomes a host.
            password = crate::roles::encode_userinfo(password),
            host = self.host,
            db = self.database.name,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project {
            id: "proj-1".into(),
            name: "oxy-org-acme".into(),
            region_id: "aws-us-east-2".into(),
            pg_version: 17,
            branch: Branch {
                id: "br-1".into(),
                name: "main".into(),
            },
            database: DatabaseInfo {
                name: "neondb".into(),
                owner_name: "oxy_owner".into(),
            },
            owner_role: Role {
                name: "oxy_owner".into(),
                password: Some("hunter2".into()),
            },
            host: "ep-1.aws-us-east-2.neon.tech".into(),
        }
    }

    #[test]
    fn dsn_requires_tls() {
        let dsn = project().dsn_for("app_bookings_rw", "pw");
        assert!(dsn.ends_with("?sslmode=require"), "got {dsn}");
        assert!(
            dsn.starts_with("postgres://app_bookings_rw:pw@"),
            "got {dsn}"
        );
    }

    #[test]
    fn redacted_role_drops_the_password() {
        let r = project().owner_role.redacted();
        assert!(r.password.is_none());
        assert_eq!(r.name, "oxy_owner");
    }

    #[test]
    fn absent_password_is_omitted_from_json_not_serialised_as_null() {
        let json = serde_json::to_string(&project().owner_role.redacted()).unwrap();
        assert!(!json.contains("password"), "got {json}");
    }
}
