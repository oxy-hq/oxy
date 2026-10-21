//! Who a device-code request names to approve it (`oxyc … --approver <email>`).
//!
//! The email is resolved once, at the start, to an active `users` row, and the
//! id is stored on the request. The caller answers the CLI the same whether it
//! resolved or not — the asker learns nothing about who has an account — and
//! tells the person only when it did. The email itself is never stored.

use entity::users::{self, UserStatus};
use oxy_shared::errors::OxyError;
use sea_orm::sea_query::{Expr, ExprTrait, Func};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};
use uuid::Uuid;

pub use super::device_ask::clean_approver;

/// The active user whose email is `email` (any case, already
/// [`clean_approver`]'d), or `None`. Also `None` when two accounts differ only
/// in the case of their address, rather than guessing which was meant. The
/// start binds a `None` to `UNRESOLVED_APPROVER`, so the request is nobody's
/// to decide and lapses; it never falls back to "anyone".
pub async fn resolve<C: ConnectionTrait>(db: &C, email: &str) -> Result<Option<Uuid>, OxyError> {
    let found: Vec<Uuid> = users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .filter(Expr::expr(Func::lower(Expr::col(users::Column::Email))).eq(email.to_lowercase()))
        .filter(users::Column::Status.eq(UserStatus::Active))
        .limit(2)
        .into_tuple::<Uuid>()
        .all(db)
        .await
        .map_err(|e| OxyError::DBError(format!("resolve cli device approver: {e}")))?;
    Ok(match found.as_slice() {
        [only] => Some(*only),
        _ => None,
    })
}
