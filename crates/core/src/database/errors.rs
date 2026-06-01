use sea_orm::DbErr;

/// Returns true if the error is a unique-constraint violation.
/// Backend-portable: sea_orm's SqlErr abstracts over Postgres/MySQL/SQLite.
pub fn is_unique_violation(err: &DbErr) -> bool {
    matches!(
        err.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}
