//! Database authentication mode selection.
//!
//! The app supports two paths to Postgres:
//!
//! - `Password` — classic `OXY_DATABASE_URL` with a static credential. Used
//!   for local dev (`oxy start`), in-cluster CNPG, and anywhere credentials
//!   are managed out-of-band.
//! - `Iam`     — AWS RDS IAM auth. Short-lived SigV4 tokens are generated
//!   in-process and swapped onto the live pool via
//!   `sqlx::Pool::set_connect_options`. Removes the class of outages caused
//!   by master-password rotation.

use oxy_shared::errors::OxyError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseAuthMode {
    Password,
    Iam,
}

impl DatabaseAuthMode {
    pub fn from_env() -> Result<Self, OxyError> {
        match std::env::var("OXY_DATABASE_AUTH_MODE").ok().as_deref() {
            None | Some("") | Some("password") => Ok(Self::Password),
            Some("iam") => Ok(Self::Iam),
            Some(other) => Err(OxyError::Database(format!(
                "OXY_DATABASE_AUTH_MODE must be 'password' or 'iam' (got: {other:?})"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    Require,
    VerifyFull,
}

impl SslMode {
    fn parse(s: &str) -> Result<Self, OxyError> {
        match s {
            "require" => Ok(Self::Require),
            "verify-full" => Ok(Self::VerifyFull),
            other => Err(OxyError::Database(format!(
                "OXY_DATABASE_SSL_MODE must be 'require' or 'verify-full' (got: {other:?})"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IamConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub region: String,
    pub ssl_mode: SslMode,
}

impl IamConfig {
    pub fn from_env() -> Result<Self, OxyError> {
        let host = require_var("OXY_DATABASE_HOST")?;
        let port = std::env::var("OXY_DATABASE_PORT")
            .ok()
            .as_deref()
            .unwrap_or("5432")
            .parse::<u16>()
            .map_err(|e| {
                OxyError::Database(format!("OXY_DATABASE_PORT must be a valid port: {e}"))
            })?;
        let database = require_var("OXY_DATABASE_NAME")?;
        let user = require_var("OXY_DATABASE_USER")?;
        let region = require_var("OXY_DATABASE_REGION")?;
        let ssl_mode_raw =
            std::env::var("OXY_DATABASE_SSL_MODE").unwrap_or_else(|_| "require".to_string());
        let ssl_mode = SslMode::parse(&ssl_mode_raw)?;
        Ok(Self {
            host,
            port,
            database,
            user,
            region,
            ssl_mode,
        })
    }
}

fn require_var(name: &'static str) -> Result<String, OxyError> {
    std::env::var(name).map_err(|_| {
        OxyError::Database(format!(
            "{name} is required when OXY_DATABASE_AUTH_MODE=iam"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // These env vars are process-global; keep tests serial so they don't
    // race each other.

    fn clear_iam_env() {
        for var in [
            "OXY_DATABASE_AUTH_MODE",
            "OXY_DATABASE_HOST",
            "OXY_DATABASE_PORT",
            "OXY_DATABASE_NAME",
            "OXY_DATABASE_USER",
            "OXY_DATABASE_REGION",
            "OXY_DATABASE_SSL_MODE",
        ] {
            unsafe { std::env::remove_var(var) };
        }
    }

    #[test]
    #[serial]
    fn auth_mode_defaults_to_password() {
        clear_iam_env();
        assert_eq!(
            DatabaseAuthMode::from_env().unwrap(),
            DatabaseAuthMode::Password
        );
    }

    #[test]
    #[serial]
    fn auth_mode_parses_iam() {
        clear_iam_env();
        unsafe { std::env::set_var("OXY_DATABASE_AUTH_MODE", "iam") };
        assert_eq!(DatabaseAuthMode::from_env().unwrap(), DatabaseAuthMode::Iam);
        clear_iam_env();
    }

    #[test]
    #[serial]
    fn auth_mode_rejects_unknown() {
        clear_iam_env();
        unsafe { std::env::set_var("OXY_DATABASE_AUTH_MODE", "kerberos") };
        assert!(DatabaseAuthMode::from_env().is_err());
        clear_iam_env();
    }

    #[test]
    #[serial]
    fn iam_config_requires_host_name_user_region() {
        clear_iam_env();
        assert!(IamConfig::from_env().is_err());
        clear_iam_env();
    }

    #[test]
    #[serial]
    fn iam_config_reads_all_fields() {
        clear_iam_env();
        unsafe {
            std::env::set_var("OXY_DATABASE_HOST", "db.example.com");
            std::env::set_var("OXY_DATABASE_PORT", "5432");
            std::env::set_var("OXY_DATABASE_NAME", "oxydb");
            std::env::set_var("OXY_DATABASE_USER", "oxy_app");
            std::env::set_var("OXY_DATABASE_REGION", "us-west-2");
            std::env::set_var("OXY_DATABASE_SSL_MODE", "verify-full");
        }
        let cfg = IamConfig::from_env().unwrap();
        assert_eq!(cfg.host, "db.example.com");
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.database, "oxydb");
        assert_eq!(cfg.user, "oxy_app");
        assert_eq!(cfg.region, "us-west-2");
        assert_eq!(cfg.ssl_mode, SslMode::VerifyFull);
        clear_iam_env();
    }

    #[test]
    #[serial]
    fn iam_config_defaults_port_and_sslmode() {
        clear_iam_env();
        unsafe {
            std::env::set_var("OXY_DATABASE_HOST", "db.example.com");
            std::env::set_var("OXY_DATABASE_NAME", "oxydb");
            std::env::set_var("OXY_DATABASE_USER", "oxy_app");
            std::env::set_var("OXY_DATABASE_REGION", "us-west-2");
        }
        let cfg = IamConfig::from_env().unwrap();
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.ssl_mode, SslMode::Require);
        clear_iam_env();
    }
}
