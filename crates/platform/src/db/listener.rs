//! `ListenerConfigFactory` for the agentic task router's dedicated
//! LISTEN connection.
//!
//! Two implementations, selected by the same `OXY_DATABASE_AUTH_MODE`
//! that drives the connection pool in [`super::client`]:
//!
//! - **Password**: parses `OXY_DATABASE_URL` once at startup and the
//!   factory clones the resulting [`tokio_postgres::Config`] on each
//!   call. Same URL the pool uses; no surprises.
//! - **IAM**: each factory call awaits a fresh SigV4 token from
//!   [`super::iam::generate_auth_token`] and builds a Config with the
//!   token as the password. The reconnect loop in `PostgresTaskRouter`
//!   calls the factory exactly when it needs a fresh credential, which
//!   is the only time RDS IAM auth checks tokens.
//!
//! Why this lives in the platform crate, not the runtime: the platform
//! crate already owns [`super::auth_mode::IamConfig`] and the token
//! mint. Putting these factories here keeps `agentic-runtime` free of
//! AWS deps.
//!
//! ## What this file does NOT do
//!
//! - **No background token refresh.** Unlike the pool (which keeps
//!   long-lived physical connections that need fresh tokens
//!   continuously), the listener has one connection at a time, and
//!   Postgres doesn't re-auth mid-stream. The token only matters at
//!   *connect* time; the reconnect loop is the only refresh hook we
//!   need.
//! - **No retry inside the factory.** AWS credential outages bubble up
//!   to the router's reconnect-with-backoff path, which already retries
//!   200ms → 5s. Re-implementing that here would just busy-loop faster.

use std::sync::Arc;

use agentic_runtime::router::{ListenerConfigFactory, PostgresTaskRouter};
use oxy_shared::errors::OxyError;
use tokio_postgres::config::SslMode as PgSslMode;

use super::auth_mode::{DatabaseAuthMode, IamConfig, SslMode};
use super::iam::generate_auth_token;

/// Build a [`ListenerConfigFactory`] using the same `OXY_DATABASE_AUTH_MODE`
/// selection that [`super::establish_connection`] honours.
///
/// Returned closure is `Arc`-shared; cheap to clone into the router.
pub fn listener_factory_from_env() -> Result<ListenerConfigFactory, OxyError> {
    match DatabaseAuthMode::from_env()? {
        DatabaseAuthMode::Password => password_factory_from_env(),
        DatabaseAuthMode::Iam => Ok(iam_factory(IamConfig::from_env()?)),
    }
}

fn password_factory_from_env() -> Result<ListenerConfigFactory, OxyError> {
    let url = std::env::var("OXY_DATABASE_URL").map_err(|_| {
        OxyError::Database(
            "OXY_DATABASE_URL is required for the task router's listener \
             when OXY_DATABASE_AUTH_MODE is password (or unset)."
                .to_string(),
        )
    })?;
    PostgresTaskRouter::password_factory_from_url(&url)
        .map_err(|e| OxyError::Database(format!("listener factory: {e}")))
}

/// IAM-mode factory.
///
/// Each call awaits `generate_auth_token` — that does
/// `aws_config::load_defaults().await` internally, which can hit IMDS.
/// For the listener this fires only on reconnect (typically rare); the
/// router's reconnect backoff (200ms → 5s cap) bounds the rate during
/// failover events.
///
/// `SslMode::VerifyFull` and `SslMode::Require` both map to
/// `tokio_postgres::SslMode::Require`. The hostname / cert chain
/// verification difference normally signalled by libpq's `VerifyFull`
/// is enforced at the rustls layer inside
/// `agentic_runtime::router`'s `build_rustls_connector` — that
/// connector uses `with_root_certificates(...)` which performs full
/// chain + SAN/hostname verification unconditionally. We're always
/// strict; both modes get the same TLS behaviour.
fn iam_factory(config: IamConfig) -> ListenerConfigFactory {
    Arc::new(move || {
        let config = config.clone();
        Box::pin(async move {
            let token = generate_auth_token(&config)
                .await
                .map_err(|e| format!("IAM token mint: {e}"))?;
            Ok(build_iam_config(&config, &token))
        })
    })
}

fn build_iam_config(config: &IamConfig, token: &str) -> tokio_postgres::Config {
    let mut pg = tokio_postgres::Config::new();
    pg.host(&config.host)
        .port(config.port)
        .user(&config.user)
        .password(token)
        .dbname(&config.database)
        .ssl_mode(match config.ssl_mode {
            SslMode::Require | SslMode::VerifyFull => PgSslMode::Require,
        });
    pg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_iam_config() -> IamConfig {
        IamConfig {
            host: "db.example.com".to_string(),
            port: 5432,
            database: "oxydb".to_string(),
            user: "oxy_app".to_string(),
            region: "us-west-2".to_string(),
            ssl_mode: SslMode::Require,
        }
    }

    #[test]
    fn iam_config_propagates_all_fields() {
        let cfg = fake_iam_config();
        let pg = build_iam_config(&cfg, "fake-token");
        assert_eq!(pg.get_hosts().len(), 1, "expected exactly one host entry");
        assert_eq!(pg.get_ports(), &[5432]);
        assert_eq!(pg.get_user(), Some("oxy_app"));
        assert_eq!(pg.get_dbname(), Some("oxydb"));
        // get_ssl_mode is exposed by tokio-postgres.
        assert_eq!(pg.get_ssl_mode(), PgSslMode::Require);
    }

    #[test]
    fn verify_full_maps_to_require_at_tokio_postgres_layer() {
        // VerifyFull is enforced by rustls (always), not by the
        // tokio_postgres ssl_mode field. Both map to Require here;
        // strictness comes from `build_rustls_connector` in the router.
        let mut cfg = fake_iam_config();
        cfg.ssl_mode = SslMode::VerifyFull;
        let pg = build_iam_config(&cfg, "fake-token");
        assert_eq!(pg.get_ssl_mode(), PgSslMode::Require);
    }

    // The env-var dispatch tests are `#[serial]` because they read
    // process-global env vars and would race against each other and
    // against the auth_mode::tests suite. `serial_test` is already a
    // dev-dep for the same reason.
    use serial_test::serial;

    fn clear_env() {
        for var in [
            "OXY_DATABASE_AUTH_MODE",
            "OXY_DATABASE_URL",
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

    #[tokio::test]
    #[serial]
    async fn dispatch_picks_password_factory_by_default() {
        clear_env();
        unsafe {
            std::env::set_var(
                "OXY_DATABASE_URL",
                "postgresql://u:p@db.example.com:5432/oxy",
            );
        }
        // No `.expect(...)` — ListenerConfigFactory has no Debug impl.
        let factory = match listener_factory_from_env() {
            Ok(f) => f,
            Err(e) => panic!("expected password factory, got error: {e}"),
        };
        // The closure should be infallible for a parsed URL — calling
        // it twice mimics the reconnect loop and asserts the closure
        // is cheap to re-invoke without re-parsing on every call.
        let cfg_a = factory().await.expect("first call");
        let cfg_b = factory().await.expect("second call");
        assert_eq!(cfg_a.get_user(), Some("u"));
        assert_eq!(cfg_b.get_dbname(), Some("oxy"));
        clear_env();
    }

    #[tokio::test]
    #[serial]
    async fn dispatch_errors_when_password_url_missing() {
        clear_env();
        unsafe { std::env::set_var("OXY_DATABASE_AUTH_MODE", "password") };
        // ListenerConfigFactory is `Arc<dyn Fn(...) -> _>` which has
        // no Debug impl, so `expect_err` won't work — match on the
        // result manually.
        match listener_factory_from_env() {
            Ok(_) => panic!("expected missing-url error, got Ok"),
            Err(err) => {
                let msg = format!("{err}");
                assert!(
                    msg.contains("OXY_DATABASE_URL"),
                    "error should name the missing env var; got: {msg}"
                );
            }
        }
        clear_env();
    }

    #[tokio::test]
    #[serial]
    async fn dispatch_picks_iam_factory_when_mode_iam() {
        clear_env();
        unsafe {
            std::env::set_var("OXY_DATABASE_AUTH_MODE", "iam");
            std::env::set_var("OXY_DATABASE_HOST", "db.example.com");
            std::env::set_var("OXY_DATABASE_NAME", "oxydb");
            std::env::set_var("OXY_DATABASE_USER", "oxy_app");
            std::env::set_var("OXY_DATABASE_REGION", "us-west-2");
        }
        // We can construct the factory without AWS — IAM env-var
        // validation runs synchronously. Don't *call* the factory:
        // generate_auth_token would try to mint a real SigV4 token
        // and either hit IMDS or fail in opaque ways depending on
        // local AWS env. Construction alone validates dispatch.
        let _factory = match listener_factory_from_env() {
            Ok(f) => f,
            Err(e) => panic!("expected iam factory, got error: {e}"),
        };
        clear_env();
    }
}
