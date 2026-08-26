//! One way to open a tenant Postgres connection.
//!
//! Every tenant-side connection — DDL, migrations, `ctx.tx`, the ERD reader —
//! goes through here so that TLS is not a per-call-site decision. It was, and
//! all four sites independently chose `NoTls`, which is invisible against the
//! local demo cluster and fatal against any managed provider: Neon, Supabase and
//! RDS with `rds.force_ssl` all refuse a plaintext session outright.
//!
//! **The DSN decides, not a flag.** `sslmode` is already part of every DSN this
//! crate builds ([`crate::provisioner::sslmode_for`]), so a single rustls
//! connector covers every case:
//!
//! | `sslmode` | Behaviour |
//! | --- | --- |
//! | `disable` | plaintext; what a local Docker Postgres uses |
//! | absent / `prefer` | try TLS, fall back to plaintext |
//! | `require` and stricter | TLS or the connection fails |
//!
//! So the same helper serves a container with no certificates and a managed
//! provider that mandates them, and adding a provider cannot reintroduce the
//! bug by forgetting a connector argument.

use std::sync::OnceLock;

use tokio_postgres::Client;
use tokio_postgres_rustls::MakeRustlsConnect;

/// Process-wide TLS config. Built once: assembling the root store per
/// connection would show up on `ctx.tx`, which opens one per invocation.
fn tls_connector() -> MakeRustlsConnect {
    static TLS: OnceLock<MakeRustlsConnect> = OnceLock::new();
    TLS.get_or_init(|| {
        // Idempotent, and `let _` because another crate in this process may
        // legitimately have installed it first.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        MakeRustlsConnect::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    })
    .clone()
}

/// Connect and spawn the connection driver.
///
/// `label` names the caller in the log line emitted when the driver ends, so a
/// dropped connection is attributable to `ctx.tx` or the migrator rather than
/// appearing as an anonymous disconnect.
pub async fn connect(dsn: &str, label: &'static str) -> Result<Client, tokio_postgres::Error> {
    let (client, connection) = tokio_postgres::connect(dsn, tls_connector()).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::debug!("{label} connection closed: {e}");
        }
    });
    Ok(client)
}

/// Render a Postgres failure the way a human needs it.
///
/// `tokio_postgres::Error`'s Display is just "db error" — the constraint name,
/// the column, the reason and any `RAISE ... USING HINT` all live in the
/// `DbError`. Lives here because every caller in this crate reaches Postgres
/// through this module, and "statement failed: db error" has now cost two
/// separate debugging sessions.
pub(crate) fn pg_detail(e: &tokio_postgres::Error) -> String {
    match e.as_db_error() {
        Some(db) => {
            // SQLSTATE first: it is the only stable, machine-matchable part of
            // a Postgres error, and callers that classify failures (see
            // `provisioner::is_unconfined`) must not have to match on prose.
            let mut msg = format!("[{}] {}", db.code().code(), db.message());
            if let Some(detail) = db.detail() {
                msg.push_str(&format!(" — {detail}"));
            }
            if let Some(hint) = db.hint() {
                msg.push_str(&format!(" (hint: {hint})"));
            }
            msg
        }
        None => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use tokio_postgres::config::SslMode;

    /// The whole design rests on `sslmode` surviving into the parsed config, so
    /// that one connector can serve both a plaintext container and a provider
    /// that mandates TLS. If this ever stopped holding, every tenant connection
    /// would silently pick the wrong mode.
    #[test]
    fn sslmode_in_the_dsn_is_what_decides() {
        let parse = |dsn: &str| {
            dsn.parse::<tokio_postgres::Config>()
                .unwrap()
                .get_ssl_mode()
        };

        assert_eq!(
            parse("postgres://u:p@localhost:55432/db?sslmode=disable"),
            SslMode::Disable,
            "the local demo cluster has no certificates"
        );
        assert_eq!(
            parse("postgres://u:p@ep-x.neon.tech/neondb?sslmode=require"),
            SslMode::Require,
            "a managed provider must not be reachable in plaintext"
        );
        assert_eq!(
            parse("postgres://u:p@localhost/db"),
            SslMode::Prefer,
            "an unannotated DSN tries TLS and falls back, so both work"
        );
    }
}
