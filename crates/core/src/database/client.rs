use std::time::Duration;

use oxy_shared::errors::OxyError;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, SqlxPostgresConnector};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use tokio::sync::OnceCell;

use super::auth_mode::{DatabaseAuthMode, IamConfig, SslMode};
use super::iam;

static DB_POOL: OnceCell<DatabaseConnection> = OnceCell::const_new();

// Connection pool sizing — kept identical across auth modes so prod/dev
// behave the same under load.
const MAX_CONNECTIONS: u32 = 80;
const MIN_CONNECTIONS: u32 = 20;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_LIFETIME: Duration = Duration::from_secs(1800);

// Refresh IAM tokens every 10 min, leaving a 5-min headroom before the
// 15-min RDS token TTL expires. New physical connections always use a
// token at most ~10 min old.
const IAM_TOKEN_REFRESH_INTERVAL: Duration = Duration::from_secs(600);

// On refresh failure, retry aggressively until success. Without this, a
// single transient AWS credential-provider blip at t=10m would wait the
// full 10-min cadence before retrying at t=20m — but the baked token
// expires at t=15m, so new connections would start failing for 5 minutes
// before the next attempt. 60s retry closes that window.
const IAM_TOKEN_REFRESH_RETRY: Duration = Duration::from_secs(60);

pub async fn establish_connection() -> Result<DatabaseConnection, OxyError> {
    DB_POOL
        .get_or_try_init(|| async {
            match DatabaseAuthMode::from_env()? {
                DatabaseAuthMode::Password => connect_password().await,
                DatabaseAuthMode::Iam => connect_iam().await,
            }
        })
        .await
        .cloned()
}

async fn connect_password() -> Result<DatabaseConnection, OxyError> {
    let url = std::env::var("OXY_DATABASE_URL").map_err(|_| {
        OxyError::Database(
            "OXY_DATABASE_URL environment variable is required. \
             Use 'oxy start' to automatically start PostgreSQL with Docker, \
             or set OXY_DATABASE_URL to your PostgreSQL connection string."
                .to_string(),
        )
    })?;

    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        tracing::error!(
            "OXY_DATABASE_URL must be a PostgreSQL connection string (starting with \
             'postgres://' or 'postgresql://'). Got: {}",
            url
        );
        return Err(OxyError::Database(
            "OXY_DATABASE_URL must be a PostgreSQL connection string (starting with \
             'postgres://' or 'postgresql://')"
                .to_string(),
        ));
    }

    tracing::debug!("Connecting to PostgreSQL from OXY_DATABASE_URL");

    let mut opt = ConnectOptions::new(url);
    opt.max_connections(MAX_CONNECTIONS)
        .min_connections(MIN_CONNECTIONS)
        .connect_timeout(CONNECT_TIMEOUT)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .idle_timeout(IDLE_TIMEOUT)
        .max_lifetime(MAX_LIFETIME)
        .sqlx_logging(false);

    connect_sea_orm_with_retry(opt).await
}

async fn connect_iam() -> Result<DatabaseConnection, OxyError> {
    let config = IamConfig::from_env()?;
    tracing::info!(
        host = %config.host,
        port = config.port,
        database = %config.database,
        user = %config.user,
        region = %config.region,
        "Connecting to PostgreSQL via RDS IAM auth"
    );

    let initial_token = iam::generate_auth_token(&config).await?;
    let connect_options = build_pg_connect_options(&config, &initial_token);
    let pool = connect_sqlx_with_retry(connect_options).await?;
    let db = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

    // Spawn the token-refresh loop. It holds a clone of the underlying
    // sqlx::PgPool (Arc-backed) and swaps fresh options onto it every
    // IAM_TOKEN_REFRESH_INTERVAL. Existing connections are unaffected by
    // set_connect_options; only new physical connections pick up the
    // refreshed token.
    let pool_clone = db.get_postgres_connection_pool().clone();
    tokio::spawn(refresh_iam_token_loop(pool_clone, config));

    Ok(db)
}

fn build_pg_connect_options(config: &IamConfig, token: &str) -> PgConnectOptions {
    let ssl_mode = match config.ssl_mode {
        SslMode::Require => PgSslMode::Require,
        SslMode::VerifyFull => PgSslMode::VerifyFull,
    };
    PgConnectOptions::new()
        .host(&config.host)
        .port(config.port)
        .username(&config.user)
        .database(&config.database)
        .password(token)
        .ssl_mode(ssl_mode)
}

async fn refresh_iam_token_loop(pool: sqlx::PgPool, config: IamConfig) {
    let mut next_delay = IAM_TOKEN_REFRESH_INTERVAL;
    loop {
        tokio::time::sleep(next_delay).await;
        match iam::generate_auth_token(&config).await {
            Ok(token) => {
                pool.set_connect_options(build_pg_connect_options(&config, &token));
                tracing::info!("Refreshed RDS IAM auth token");
                next_delay = IAM_TOKEN_REFRESH_INTERVAL;
            }
            Err(e) => {
                // Existing pooled connections keep working; only *new*
                // physical connections will start failing once the currently
                // baked token ages past 15 min. Alert on this log line.
                tracing::error!(
                    error = %e,
                    retry_seconds = IAM_TOKEN_REFRESH_RETRY.as_secs(),
                    "Failed to refresh RDS IAM auth token; will retry shortly"
                );
                next_delay = IAM_TOKEN_REFRESH_RETRY;
            }
        }
    }
}

// ---- retry helpers ---------------------------------------------------------

// Postgres can accept TCP but reject the startup packet with "Connection reset
// by peer" for a short window after the container reports ready (Docker
// port-publisher + Postgres backend init race). Retry a handful of times with
// short backoff to absorb this.
const CONNECT_MAX_ATTEMPTS: u32 = 8;
const CONNECT_INITIAL_BACKOFF_MS: u64 = 250;
const CONNECT_MAX_BACKOFF_MS: u64 = 2000;

async fn connect_sea_orm_with_retry(opt: ConnectOptions) -> Result<DatabaseConnection, OxyError> {
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match Database::connect(opt.clone()).await {
            Ok(db) => return Ok(db),
            Err(e) if attempt < CONNECT_MAX_ATTEMPTS && is_transient_sea_orm_error(&e) => {
                sleep_backoff(attempt, &e.to_string()).await;
            }
            Err(e) => {
                tracing::error!(
                    "Failed to connect to PostgreSQL database after {} attempt(s): {}",
                    attempt,
                    e
                );
                return Err(OxyError::Database(e.to_string()));
            }
        }
    }
}

async fn connect_sqlx_with_retry(opt: PgConnectOptions) -> Result<sqlx::PgPool, OxyError> {
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        let pool_result = PgPoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .min_connections(MIN_CONNECTIONS)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .idle_timeout(Some(IDLE_TIMEOUT))
            .max_lifetime(Some(MAX_LIFETIME))
            .connect_with(opt.clone())
            .await;
        match pool_result {
            Ok(pool) => return Ok(pool),
            Err(e) if attempt < CONNECT_MAX_ATTEMPTS && is_transient_sqlx_error(&e) => {
                sleep_backoff(attempt, &e.to_string()).await;
            }
            Err(e) => {
                tracing::error!(
                    "Failed to establish IAM-authenticated PostgreSQL pool after {} attempt(s): {}",
                    attempt,
                    e
                );
                return Err(OxyError::Database(e.to_string()));
            }
        }
    }
}

async fn sleep_backoff(attempt: u32, msg: &str) {
    let backoff_ms = std::cmp::min(
        CONNECT_INITIAL_BACKOFF_MS.saturating_mul(2u64.saturating_pow(attempt - 1)),
        CONNECT_MAX_BACKOFF_MS,
    );
    tracing::warn!(
        "Transient PostgreSQL connect error (attempt {}/{}): {}. Retrying in {}ms",
        attempt,
        CONNECT_MAX_ATTEMPTS,
        msg,
        backoff_ms
    );
    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
}

// Classify errors that deserve a retry at startup. Prefer structural matching
// on `sea_orm::DbErr` / `sqlx::Error` variants — the string-based fallback is
// inherently version-sensitive (sea_orm, sqlx, or the OS may change error
// formatting) and is only there to catch the long tail.
fn is_transient_sea_orm_error(err: &sea_orm::DbErr) -> bool {
    use sea_orm::{DbErr, RuntimeErr};

    if let DbErr::Conn(RuntimeErr::SqlxError(sqlx_err)) = err
        && is_transient_sqlx_error(sqlx_err)
    {
        return true;
    }
    if matches!(err, DbErr::ConnectionAcquire(_)) {
        return true;
    }

    // Fallback: non-sqlx `Internal` errors and anything the structural path
    // missed. Substrings target stable English wording.
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("connection reset by peer")
        || msg.contains("connection refused")
        || msg.contains("connection closed")
        || msg.contains("broken pipe")
        || msg.contains("unexpected eof")
        || msg.contains("no connection could be made")
        // OS error codes are a last-resort fallback for non-English locales
        // where the substrings above don't match. Unix-specific; on Windows
        // the WSA codes (10054/10061) are rendered with the English substrings
        // above by the Rust standard library.
        || msg.contains("os error 54")    // macOS ECONNRESET
        || msg.contains("os error 104")   // Linux ECONNRESET
        || msg.contains("os error 111") // Linux ECONNREFUSED
}

fn is_transient_sqlx_error(err: &sqlx::Error) -> bool {
    use std::io::ErrorKind;
    match err {
        sqlx::Error::Io(io_err) => matches!(
            io_err.kind(),
            ErrorKind::ConnectionReset
                | ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionAborted
                | ErrorKind::BrokenPipe
                | ErrorKind::UnexpectedEof
                | ErrorKind::TimedOut
                | ErrorKind::NotConnected
        ),
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::WorkerCrashed => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn iam_config() -> IamConfig {
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
    fn build_pg_connect_options_sets_all_fields() {
        let cfg = iam_config();
        let opts = build_pg_connect_options(&cfg, "fake-iam-token");
        assert_eq!(opts.get_host(), "db.example.com");
        assert_eq!(opts.get_port(), 5432);
        assert_eq!(opts.get_username(), "oxy_app");
        assert_eq!(opts.get_database(), Some("oxydb"));
        assert!(matches!(opts.get_ssl_mode(), PgSslMode::Require));
    }

    #[test]
    fn build_pg_connect_options_propagates_verify_full() {
        let mut cfg = iam_config();
        cfg.ssl_mode = SslMode::VerifyFull;
        let opts = build_pg_connect_options(&cfg, "fake-iam-token");
        assert!(matches!(opts.get_ssl_mode(), PgSslMode::VerifyFull));
    }

    #[test]
    fn transient_sqlx_error_classifies_io_kinds() {
        for kind in [
            io::ErrorKind::ConnectionReset,
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::BrokenPipe,
            io::ErrorKind::UnexpectedEof,
            io::ErrorKind::TimedOut,
            io::ErrorKind::NotConnected,
        ] {
            let err = sqlx::Error::Io(io::Error::new(kind, "x"));
            assert!(
                is_transient_sqlx_error(&err),
                "expected {kind:?} to be transient"
            );
        }
    }

    #[test]
    fn transient_sqlx_error_rejects_permanent_io_kinds() {
        let err = sqlx::Error::Io(io::Error::new(io::ErrorKind::InvalidData, "x"));
        assert!(!is_transient_sqlx_error(&err));
    }

    #[test]
    fn transient_sqlx_error_classifies_pool_states() {
        assert!(is_transient_sqlx_error(&sqlx::Error::PoolTimedOut));
        assert!(is_transient_sqlx_error(&sqlx::Error::PoolClosed));
        assert!(is_transient_sqlx_error(&sqlx::Error::WorkerCrashed));
    }

    #[test]
    fn transient_sea_orm_error_classifies_structural_sqlx_wrap() {
        use sea_orm::{DbErr, RuntimeErr};
        let inner = sqlx::Error::Io(io::Error::new(io::ErrorKind::ConnectionReset, "x"));
        let err = DbErr::Conn(RuntimeErr::SqlxError(inner));
        assert!(is_transient_sea_orm_error(&err));
    }

    #[test]
    fn transient_sea_orm_error_falls_back_to_string_match() {
        use sea_orm::DbErr;
        let err = DbErr::Custom("Connection reset by peer".to_string());
        assert!(is_transient_sea_orm_error(&err));

        let err = DbErr::Custom("Syntax error at position 1".to_string());
        assert!(!is_transient_sea_orm_error(&err));
    }

    #[test]
    fn backoff_caps_at_ceiling() {
        // Replicates the backoff computation used in `sleep_backoff` so any
        // future change to exponent/ceiling is flagged by this test.
        fn compute(attempt: u32) -> u64 {
            std::cmp::min(
                CONNECT_INITIAL_BACKOFF_MS.saturating_mul(2u64.saturating_pow(attempt - 1)),
                CONNECT_MAX_BACKOFF_MS,
            )
        }
        assert_eq!(compute(1), 250);
        assert_eq!(compute(2), 500);
        assert_eq!(compute(3), 1000);
        assert_eq!(compute(4), 2000);
        assert_eq!(compute(5), 2000);
        assert_eq!(compute(10), 2000);
        // Far-out attempts must not overflow.
        assert_eq!(compute(100), 2000);
    }
}
