//! Is this `DbErr` worth retrying?
//!
//! **Ported, deliberately, from `crates/platform/src/db/client.rs`'s private
//! `is_transient_sea_orm_error` / `is_transient_sqlx_error`.** That is the
//! workspace's one existing transient-vs-permanent convention and this is a
//! byte-for-byte copy of its logic, so the two stay diffable. It is copied
//! rather than called because `oxy-platform` is a platform crate and
//! `agentic-pipeline`'s `src/` may not import one (see this crate's
//! `CLAUDE.md`) — not because a second convention was wanted.
//!
//! One consumer today: [`crate::airway_config`]'s admission resolver, which
//! retries a transient failure instead of failing the automation step that
//! asked for the policy. If a second *agentic* consumer appears, move this
//! module down to `agentic-runtime` (which already owns the queue's DB
//! primitives) rather than making a third copy.
//!
//! The classification is deliberately asymmetric. Calling a determinate error
//! transient costs a bounded wait and then the same failure; calling a
//! transient error determinate kills work that would have succeeded. When a
//! variant is genuinely ambiguous, prefer `true`.

/// `true` when `err` looks like a connection / pool / IO problem rather than a
/// statement the database will reject just as firmly next time.
///
/// Prefer structural matching on `sea_orm::DbErr` / `sqlx::Error` variants —
/// the string-based fallback is inherently version-sensitive (sea_orm, sqlx,
/// or the OS may change error formatting) and is only there to catch the long
/// tail, notably `DbErr::Query`/`DbErr::Exec` wrapping an IO error when a
/// pooled connection drops mid-statement.
pub(crate) fn is_transient_db_error(err: &sea_orm::DbErr) -> bool {
    use sea_orm::{DbErr, RuntimeErr};

    if let DbErr::Conn(RuntimeErr::SqlxError(sqlx_err)) = err
        && is_transient_sqlx_error(sqlx_err)
    {
        return true;
    }
    // `ConnAcquireErr::Timeout` is how the 30s `acquire_timeout` on the shared
    // pool surfaces; `ConnectionClosed` is a pool shutting down under us.
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
        || msg.contains("pool timed out")
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
    use super::is_transient_db_error;
    use sea_orm::{DbErr, RuntimeErr};

    #[test]
    fn pool_and_connection_failures_are_transient() {
        assert!(is_transient_db_error(&DbErr::ConnectionAcquire(
            sea_orm::ConnAcquireErr::Timeout
        )));
        assert!(is_transient_db_error(&DbErr::ConnectionAcquire(
            sea_orm::ConnAcquireErr::ConnectionClosed
        )));
        assert!(is_transient_db_error(&DbErr::Conn(RuntimeErr::SqlxError(
            sqlx::Error::PoolTimedOut.into()
        ))));
        assert!(is_transient_db_error(&DbErr::Conn(RuntimeErr::SqlxError(
            sqlx::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionReset)).into()
        ))));
    }

    /// The case the structural path misses: a pooled connection dropped
    /// mid-statement arrives as `Query`/`Exec`, not `Conn`. The string
    /// fallback is what makes those retryable.
    #[test]
    fn a_connection_dropped_mid_statement_is_transient() {
        assert!(is_transient_db_error(&DbErr::Query(RuntimeErr::SqlxError(
            sqlx::Error::Io(std::io::Error::from(std::io::ErrorKind::BrokenPipe)).into()
        ))));
        assert!(is_transient_db_error(&DbErr::Exec(RuntimeErr::Internal(
            "connection reset by peer".into()
        ))));
    }

    /// A statement the database will reject just as firmly next time must not
    /// be retried — retrying it only delays a failure that is already final.
    #[test]
    fn determinate_failures_are_not_transient() {
        assert!(!is_transient_db_error(&DbErr::RecordNotFound("x".into())));
        assert!(!is_transient_db_error(&DbErr::Type("bad column".into())));
        assert!(!is_transient_db_error(&DbErr::Json("bad json".into())));
        assert!(!is_transient_db_error(&DbErr::Custom(
            "relation \"airway_source_config\" does not exist".into()
        )));
        assert!(!is_transient_db_error(&DbErr::RecordNotInserted));
    }
}
