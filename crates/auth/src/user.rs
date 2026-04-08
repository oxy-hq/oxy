use entity::prelude::Users;
use entity::users;
use oxy::database::{client::establish_connection, filters::UserQueryFilterExt};
use oxy_shared::errors::OxyError;
use sea_orm::{ActiveValue, DbErr, EntityTrait, PaginatorTrait, Set, prelude::*};
use uuid::Uuid;

use crate::types::{AuthenticatedUser, Identity};
use entity::users::{UserRole, UserStatus};

/// Email address for the built-in local guest user (no-auth local mode).
/// This user is always granted Admin role so local installs work out of the box.
pub const LOCAL_GUEST_EMAIL: &str = "<local-user@example.com>";

/// Returns `true` if `email` should be treated as admin in local (non-cloud) mode.
///
/// Logic:
/// 1. The built-in local guest is always admin.
/// 2. If `admins` is non-empty, the email must be listed.
/// 3. If `admins` is empty, everyone is admin (permissive default for single-user installs).
pub fn is_admin_email(admins: &[String], email: &str) -> bool {
    email == LOCAL_GUEST_EMAIL || admins.is_empty() || admins.iter().any(|a| a == email)
}

/// Returns true if this email should unconditionally have Admin role.
///
/// Covers:
/// - The built-in local guest (always admin in no-auth mode).
/// - Any email listed in the `OXY_ADMINS` env var (comma-separated).
///   Setting `OXY_ADMINS` and re-logging in is the bootstrap path for
///   granting admin access when no admin exists yet.
pub fn should_be_admin(email: &str) -> bool {
    if email == LOCAL_GUEST_EMAIL {
        return true;
    }
    let oxy_admins = std::env::var("OXY_ADMINS").unwrap_or_default();
    oxy_admins
        .split(',')
        .map(|s| s.trim())
        .any(|a| !a.is_empty() && a == email)
}

pub struct UserService;

impl UserService {
    pub async fn get_or_create_user(identity: &Identity) -> Result<AuthenticatedUser, OxyError> {
        let connection = establish_connection().await?;

        // First, try to find existing user
        if let Some(existing_user) = Users::find()
            .filter_by_email(&identity.email)
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?
        {
            // Ensure users that should be admin (local guest or listed in OXY_ADMINS)
            // always have at least Admin role in the DB. This runs on every login so that
            // setting OXY_ADMINS and re-logging in is enough to bootstrap an admin.
            // Never demote an existing Owner.
            if existing_user.role == UserRole::Member && should_be_admin(&existing_user.email) {
                let mut active: users::ActiveModel = existing_user.into();
                active.role = Set(UserRole::Admin);
                let updated = active.update(&connection).await.map_err(|e| {
                    OxyError::DBError(format!("Failed to promote user to admin: {e}"))
                })?;
                return Ok(updated.into());
            }
            return Ok(existing_user.into());
        }

        // User not found, try to create.
        // The local guest is always Admin. The very first real user to register
        // becomes Owner so there is always a bootstrap owner without needing
        // to pre-configure one in config.yml.
        //
        // We exclude the built-in LOCAL_GUEST_EMAIL from the count so that
        // transitioning from no-auth → auth mode still produces an owner:
        // the guest user pre-exists in the DB, but it is synthetic and should
        // not prevent the first real user from getting the Owner role.
        let existing_count = Users::find()
            .filter(users::Column::Email.ne(LOCAL_GUEST_EMAIL))
            .count(&connection)
            .await
            .unwrap_or(1); // fail-safe: don't grant owner if the count query errors
        // The very first real user becomes Owner (top-level, can grant admin).
        // OXY_ADMINS users get Admin (recovery path, not ownership).
        let initial_role = if existing_count == 0 {
            UserRole::Owner
        } else if should_be_admin(&identity.email) {
            UserRole::Admin
        } else {
            UserRole::Member
        };

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
            role: Set(initial_role),
            status: Set(UserStatus::Active),
            created_at: ActiveValue::not_set(), // Will use database default
            last_login_at: ActiveValue::not_set(), // Will use database default
        };

        match new_user.insert(&connection).await {
            Ok(user) => {
                tracing::info!(
                    "Created new user: {} ({}) with role: {}",
                    user.email,
                    user.id,
                    user.role.as_str()
                );
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

    pub async fn update_user_role(user_id: Uuid, role: UserRole) -> Result<(), OxyError> {
        let connection = establish_connection().await?;

        let user = Users::find_by_id(user_id)
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?
            .ok_or_else(|| OxyError::DBError("User not found".to_string()))?;

        let mut user: users::ActiveModel = user.into();
        user.role = Set(role);

        user.update(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to update user role: {e}")))?;

        Ok(())
    }
}

/// Check if a database error is a unique constraint violation.
fn is_unique_violation(err: &DbErr) -> bool {
    let err_str = err.to_string().to_lowercase();
    err_str.contains("duplicate key") || err_str.contains("unique constraint")
}
