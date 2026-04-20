use entity::prelude::Users;
use entity::users;
use oxy::database::{client::establish_connection, filters::UserQueryFilterExt};
use oxy_shared::errors::OxyError;
use sea_orm::{ActiveValue, DbErr, EntityTrait, PaginatorTrait, Set, prelude::*};
use uuid::Uuid;

use crate::types::{AuthenticatedUser, Identity};
use entity::users::UserStatus;

/// Email address for the built-in local guest user (no-auth local mode).
/// This user is always granted Owner role so local installs work out of the box.
pub const LOCAL_GUEST_EMAIL: &str = "<local-user@example.com>";

/// Returns `true` if `email` should be treated as admin in local (non-cloud) mode.
///
/// Logic:
/// 1. The built-in local guest is always admin.
/// 2. If `OXY_OWNER` is set, only that email is admin (plus the local guest).
/// 3. If `OXY_OWNER` is unset, everyone is admin (permissive default for single-user installs).
pub fn is_local_admin(owner_email: Option<&str>, email: &str) -> bool {
    email == LOCAL_GUEST_EMAIL || owner_email.is_none_or(|owner| owner == email)
}

/// Convenience wrapper that reads `OXY_OWNER` from the environment.
pub fn is_local_admin_from_env(email: &str) -> bool {
    let owner = std::env::var("OXY_OWNER").ok();
    is_local_admin(owner.as_deref(), email)
}

/// Returns true if this email should be promoted to Owner on login.
///
/// Covers:
/// - The built-in local guest (always admin in no-auth mode).
/// - The email set in `OXY_OWNER` — a single address that receives the Owner
///   role unconditionally. Setting `OXY_OWNER` and re-logging in is the
///   bootstrap path for assigning ownership without touching the database.
pub fn should_be_owner(email: &str) -> bool {
    if email == LOCAL_GUEST_EMAIL {
        return true;
    }
    match std::env::var("OXY_OWNER") {
        Ok(owner) => owner.trim() == email,
        Err(_) => false,
    }
}

pub struct UserService;

impl UserService {
    /// Look up an existing user by the email in the identity. Does not create.
    /// Used by non-mutating endpoints (e.g. `GET /user`) where an incidental user
    /// row should not be minted just because someone hit the endpoint.
    pub async fn find_user_by_identity(
        identity: &Identity,
    ) -> Result<Option<AuthenticatedUser>, OxyError> {
        let connection = establish_connection().await?;
        let user = Users::find()
            .filter_by_email(&identity.email)
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?;
        Ok(user.map(|u| u.into()))
    }

    pub async fn get_or_create_user(identity: &Identity) -> Result<AuthenticatedUser, OxyError> {
        let connection = establish_connection().await?;

        // First, try to find existing user
        if let Some(existing_user) = Users::find()
            .filter_by_email(&identity.email)
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?
        {
            return Ok(existing_user.into());
        }

        // User not found, create new user.

        let new_user = users::ActiveModel {
            id: Set(Uuid::new_v4()),
            email: Set(identity.email.clone()),
            name: Set(identity
                .name
                .clone()
                .unwrap_or_else(|| identity.email.clone())),
            picture: Set(identity.picture.clone()),
            email_verified: Set(true),
            magic_link_token: ActiveValue::not_set(),
            magic_link_token_expires_at: ActiveValue::not_set(),
            status: Set(UserStatus::Active),
            created_at: ActiveValue::not_set(), // Will use database default
            last_login_at: ActiveValue::not_set(), // Will use database default
        };

        match new_user.insert(&connection).await {
            Ok(user) => {
                tracing::info!("Created new user: {} ({})", user.email, user.id);
                Ok(user.into())
            }
            Err(e) if is_unique_violation(&e) => {
                // Race condition: another request created the user concurrently.
                // Fetch the existing user.
                Users::find()
                    .filter_by_email(&identity.email)
                    .one(&connection)
                    .await
                    .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?
                    .map(|u| u.into())
                    .ok_or_else(|| {
                        OxyError::DBError(format!(
                            "User '{}' not found after unique constraint violation",
                            identity.email
                        ))
                    })
            }
            Err(e) => Err(OxyError::DBError(format!("Failed to create user: {e}"))),
        }
    }

    pub async fn update_user_profile(
        user_id: Uuid,
        name: Option<String>,
        picture: Option<String>,
    ) -> Result<AuthenticatedUser, OxyError> {
        let connection = establish_connection().await?;

        let user = Users::find_by_id(user_id)
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?
            .ok_or_else(|| OxyError::DBError("User not found".to_string()))?;

        let mut user: users::ActiveModel = user.into();

        if let Some(name) = name {
            user.name = Set(name);
        }

        if let Some(picture) = picture {
            user.picture = Set(Some(picture));
        }

        let updated_user = user
            .update(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to update user: {e}")))?;

        Ok(updated_user.into())
    }

    pub async fn list_all_users() -> Result<Vec<AuthenticatedUser>, OxyError> {
        let connection = establish_connection().await?;

        let users = Users::find()
            .all(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query users: {e}")))?;

        Ok(users.into_iter().map(|user| user.into()).collect())
    }

    /// Soft delete user
    pub async fn delete_user(user_id: Uuid) -> Result<(), OxyError> {
        let connection = establish_connection().await?;

        let user = Users::find_by_id(user_id)
            .filter_active()
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?
            .ok_or_else(|| OxyError::DBError("User not found or already deleted".to_string()))?;

        let mut user: users::ActiveModel = user.into();
        user.status = Set(UserStatus::Deleted);

        user.update(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to delete user: {e}")))?;

        Ok(())
    }

    /// Update user status
    pub async fn update_user_status(user_id: Uuid, status: UserStatus) -> Result<(), OxyError> {
        let connection = establish_connection().await?;

        let user = Users::find_by_id(user_id)
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?
            .ok_or_else(|| OxyError::DBError("User not found".to_string()))?;

        let mut user: users::ActiveModel = user.into();
        user.status = Set(status);

        user.update(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to update user status: {e}")))?;

        Ok(())
    }
}

/// Check if a database error is a unique constraint violation.
/// Uses Sea-ORM's structured `SqlErr` rather than string matching so the check
/// is portable across DB engines.
fn is_unique_violation(err: &DbErr) -> bool {
    matches!(
        err.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_guest_is_always_admin() {
        assert!(is_local_admin(None, LOCAL_GUEST_EMAIL));
        assert!(is_local_admin(Some("other@example.com"), LOCAL_GUEST_EMAIL));
    }

    #[test]
    fn no_owner_makes_everyone_admin() {
        assert!(is_local_admin(None, "anyone@example.com"));
        assert!(is_local_admin(None, "random@test.org"));
    }

    #[test]
    fn owner_set_restricts_access() {
        assert!(is_local_admin(
            Some("alice@example.com"),
            "alice@example.com"
        ));
        assert!(!is_local_admin(
            Some("alice@example.com"),
            "bob@example.com"
        ));
    }

    #[test]
    fn should_be_owner_local_guest() {
        assert!(should_be_owner(LOCAL_GUEST_EMAIL));
    }

    #[test]
    fn should_be_owner_regular_user_without_env() {
        unsafe {
            std::env::remove_var("OXY_OWNER");
        }
        assert!(!should_be_owner("user@example.com"));
    }

    #[test]
    fn should_be_owner_respects_oxy_owner_env() {
        unsafe {
            std::env::set_var("OXY_OWNER", "admin@example.com");
        }
        assert!(should_be_owner("admin@example.com"));
        assert!(!should_be_owner("nobody@example.com"));
        unsafe {
            std::env::remove_var("OXY_OWNER");
        }
    }

    #[test]
    fn is_unique_violation_negative_on_non_sql_err() {
        // DbErr variants without a structured SqlErr payload must not be
        // treated as unique-constraint violations.
        let err = DbErr::Custom("some non-DB error".to_string());
        assert!(!is_unique_violation(&err));
    }
}
