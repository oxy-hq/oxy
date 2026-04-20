use oxy_shared::errors::OxyError;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::time::Duration;
use tokio::sync::OnceCell;

static DB_POOL: OnceCell<DatabaseConnection> = OnceCell::const_new();

pub async fn establish_connection() -> Result<DatabaseConnection, OxyError> {
    DB_POOL
        .get_or_try_init(|| async {
            // OXY_DATABASE_URL is required - PostgreSQL only
            let url = std::env::var("OXY_DATABASE_URL").map_err(|_| {
                OxyError::Database(
                    "OXY_DATABASE_URL environment variable is required. \
                    Use 'oxy start' to automatically start PostgreSQL with Docker, \
                    or set OXY_DATABASE_URL to your PostgreSQL connection string."
                        .to_string(),
                )
            })?;

            // Validate that the URL is a PostgreSQL connection string
            if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
                tracing::error!(
                    "OXY_DATABASE_URL must be a PostgreSQL connection string (starting with 'postgres://' or 'postgresql://'). Got: {}",
                    url
                );
                return Err(OxyError::Database(
                    "OXY_DATABASE_URL must be a PostgreSQL connection string (starting with 'postgres://' or 'postgresql://')".to_string()
                ));
            }

            tracing::debug!("Connecting to PostgreSQL from OXY_DATABASE_URL");

            // Configure connection pool for resilience against intermittent connection issues
            let mut opt = ConnectOptions::new(url);
            opt.max_connections(80)
                .min_connections(20)
                .connect_timeout(Duration::from_secs(10))
                .acquire_timeout(Duration::from_secs(30))
                // Close idle connections after 5 minutes to avoid stale connections
                .idle_timeout(Duration::from_secs(300))
                // Max lifetime of 30 minutes to force connection refresh
                .max_lifetime(Duration::from_secs(1800))
                .sqlx_logging(false);

            connect_with_retry(opt).await
        })
        .await
        .cloned()
}

// Postgres can accept TCP but reject the startup packet with "Connection reset by peer"
// for a short window after the container reports ready (Docker port-publisher + Postgres
// backend init race). Retry a handful of times with short backoff to absorb this.
const CONNECT_MAX_ATTEMPTS: u32 = 8;
const CONNECT_INITIAL_BACKOFF_MS: u64 = 250;
const CONNECT_MAX_BACKOFF_MS: u64 = 2000;

async fn connect_with_retry(opt: ConnectOptions) -> Result<DatabaseConnection, OxyError> {
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match Database::connect(opt.clone()).await {
            Ok(db) => return Ok(db),
            Err(e) if attempt < CONNECT_MAX_ATTEMPTS && is_transient_connect_error(&e) => {
                let backoff_ms = std::cmp::min(
                    CONNECT_INITIAL_BACKOFF_MS.saturating_mul(2u64.saturating_pow(attempt - 1)),
                    CONNECT_MAX_BACKOFF_MS,
                );
                tracing::warn!(
                    "Transient PostgreSQL connect error (attempt {}/{}): {}. Retrying in {}ms",
                    attempt,
                    CONNECT_MAX_ATTEMPTS,
                    e,
                    backoff_ms
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
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

// Classify errors that deserve a retry at startup. Prefer structural matching
// on `sea_orm::DbErr` / `sqlx::Error` variants — the string-based fallback is
// inherently version-sensitive (sea_orm, sqlx, or the OS may change error
// formatting) and is only there to catch the long tail.
fn is_transient_connect_error(err: &sea_orm::DbErr) -> bool {
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
