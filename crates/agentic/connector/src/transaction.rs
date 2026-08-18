//! Backend-agnostic multi-statement transaction handle.
//!
//! `DatabaseConnector` executes one statement at a time, which is correct for
//! analytics: a `SELECT` needs no atomicity, and the warehouse writers
//! (`ctx.warehouse.{insert,exec,upsert}`) are each a single statement. That
//! stops being true the moment an app writes *transactional* data — "insert
//! order + N line items + decrement inventory, or nothing" is three statements
//! that can half-apply.
//!
//! This trait is the seam for that. A [`SqlTransaction`] is a **pinned
//! connection** with an open `BEGIN`: every statement issued through it lands
//! on the same backend session, and the whole set commits or rolls back
//! together.
//!
//! Two properties are load-bearing and deliberately encoded in the signatures:
//!
//! 1. **Parameters are bound, not interpolated.** [`SqlTransaction::query`] and
//!    [`SqlTransaction::exec`] take `params` separately from `sql` and the
//!    backend binds them over the wire. `ctx.warehouse.exec(sql)` takes a bare
//!    string, which is survivable for internal ETL but is an injection hole by
//!    construction on a surface that accepts end-user input — and a transaction
//!    API exists precisely for surfaces that do.
//! 2. **Commit is a move.** `commit`/`rollback` take `Box<Self>`, so the handle
//!    is consumed and a committed transaction cannot be reused. Dropping a
//!    handle without either **must** roll back (see the impl notes on
//!    [`PgTransaction`]) — an abandoned transaction that silently commits would
//!    be the worst possible failure mode here.
//!
//! [`PgTransaction`]: crate::postgres_tx::PgTransaction

use async_trait::async_trait;

use crate::connector::ConnectorError;

/// One statement's bound parameters, as JSON.
///
/// JSON is the interchange because the only caller today is the custom-app
/// function isolate, which speaks JSON across the op boundary. Backends map
/// these onto their own parameter types; a value a backend cannot represent is
/// a hard error naming the position, never a silent coercion.
pub type TxParams = [serde_json::Value];

/// A pinned connection with an open transaction.
///
/// Obtained from [`DatabaseConnector::begin_transaction`]. See the module docs
/// for the two invariants callers may rely on.
///
/// [`DatabaseConnector::begin_transaction`]: crate::connector::DatabaseConnector::begin_transaction
#[async_trait]
pub trait SqlTransaction: Send {
    /// Run a row-returning statement and collect its rows as JSON objects
    /// keyed by column name.
    ///
    /// Rows are collected, not streamed: a transaction is held open for the
    /// duration, so an unbounded scan inside one is a lock-duration problem
    /// before it is a memory problem. Callers that need bulk reads should do
    /// them outside the transaction.
    async fn query(
        &mut self,
        sql: &str,
        params: &TxParams,
    ) -> Result<Vec<serde_json::Value>, ConnectorError>;

    /// Run a statement for its effect and return the number of rows affected.
    async fn exec(&mut self, sql: &str, params: &TxParams) -> Result<u64, ConnectorError>;

    /// Commit and release the connection. Consumes the handle.
    async fn commit(self: Box<Self>) -> Result<(), ConnectorError>;

    /// Roll back and release the connection. Consumes the handle.
    ///
    /// Rolling back an already-finished transaction is not an error — the
    /// caller's cleanup path should be safe to run unconditionally.
    async fn rollback(self: Box<Self>) -> Result<(), ConnectorError>;
}

/// The error a connector returns when it has no transaction support.
///
/// Shared so the message is identical across backends and greppable: an app
/// author who points `ctx.tx()` at DuckDB or Snowflake should get the same
/// sentence naming the backend, not a per-connector variant.
pub fn unsupported(backend: &str) -> ConnectorError {
    ConnectorError::Other(format!(
        "{backend} does not support multi-statement transactions — \
         ctx.tx() requires a Postgres-backed database (`type: postgres`). \
         Use ctx.warehouse.{{insert,exec,upsert}} for single-statement writes."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_names_the_backend_and_the_alternative() {
        let msg = unsupported("DuckDB").to_string();
        assert!(msg.contains("DuckDB"), "names the backend: {msg}");
        assert!(msg.contains("ctx.tx()"), "names the API that failed: {msg}");
        assert!(
            msg.contains("ctx.warehouse"),
            "points at the working alternative: {msg}"
        );
    }
}
