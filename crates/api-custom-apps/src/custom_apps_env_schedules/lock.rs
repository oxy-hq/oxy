//! One reconcile of an environment's schedule rows at a time.
//!
//! A publish to an environment and a flip of its switch each list the rows,
//! then create what is missing: side by side, both would see none and both
//! create one, and the duplicate would fire twice. So each holds this lock
//! while it reads and writes.
//!
//! A Postgres transaction-scoped advisory lock on a pooled connection of its
//! own, as a sandbox's teardown holds (`custom_apps_sandboxes::lock`), with a
//! key of its own: `schedules:<env>` cannot be a sandbox's name, so it never
//! waits on a teardown or a migration. Ending the transaction releases it, and
//! so does a dropped connection. A reconcile is a few queries, so a waiter
//! that has not got the lock in ten seconds ([`SET_LOCK_TIMEOUT`]) gives up
//! rather than hold a publish.

use sea_orm::{DatabaseConnection, DbErr};
use uuid::Uuid;

use crate::custom_apps_sandboxes::lock::lock_key;

/// How long a reconcile waits for another one of the same environment.
const SET_LOCK_TIMEOUT: &str = "SET LOCAL lock_timeout = '10s'";

/// The key of `environment`'s schedule rows of `app_id`.
pub(super) fn reconcile_key(app_id: Uuid, environment: &str) -> i64 {
    lock_key(app_id, &format!("schedules:{environment}"))
}

/// The lock, held until [`release`](Self::release) or drop.
pub(super) struct ReconcileLock {
    txn: sqlx::Transaction<'static, sqlx::Postgres>,
}

impl ReconcileLock {
    /// Wait for the lock — at most ten seconds.
    pub(super) async fn acquire(
        db: &DatabaseConnection,
        app_id: Uuid,
        environment: &str,
    ) -> Result<Self, DbErr> {
        let failed = |step: &str, e: sqlx::Error| DbErr::Custom(format!("{step}: {e}"));
        let mut txn = db
            .get_postgres_connection_pool()
            .begin()
            .await
            .map_err(|e| failed("open the schedules lock", e))?;
        sqlx::query(SET_LOCK_TIMEOUT)
            .execute(&mut *txn)
            .await
            .map_err(|e| failed("bound the schedules lock", e))?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(reconcile_key(app_id, environment))
            .execute(&mut *txn)
            .await
            .map_err(|e| failed("take the schedules lock", e))?;
        Ok(Self { txn })
    }

    /// Let the lock go. A failed rollback is moot: the connection closing
    /// ends the transaction too.
    pub(super) async fn release(self) {
        let _ = self.txn.rollback().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Never a sandbox's own key, so a reconcile never waits on its teardown.
    #[test]
    fn the_schedules_key_is_not_the_sandboxs() {
        let app = Uuid::from_u128(7);
        assert_ne!(reconcile_key(app, "dev-a1"), lock_key(app, "dev-a1"));
        assert_ne!(reconcile_key(app, "dev-a1"), reconcile_key(app, "staging"));
        assert_eq!(reconcile_key(app, "dev-a1"), reconcile_key(app, "dev-a1"));
    }
}
