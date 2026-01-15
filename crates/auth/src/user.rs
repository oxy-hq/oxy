use entity::prelude::Users;
use entity::users;
use oxy::database::{client::establish_connection, filters::UserQueryFilterExt};
use oxy_shared::errors::OxyError;
use sea_orm::{ActiveValue, EntityTrait, Set, prelude::*};
use uuid::Uuid;

use crate::types::{AuthenticatedUser, Identity};
use entity::users::{UserRole, UserStatus};

pub struct UserService;

impl UserService {
    pub async fn get_or_create_user(identity: &Identity) -> Result<AuthenticatedUser, OxyError> {
        let connection = establish_connection().await?;

        match Users::find()
            .filter_by_email(&identity.email)
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {e}")))?
        {
            Some(existing_user) => Ok(existing_user.into()),
            None => {
                let new_user = users::ActiveModel {
                    id: Set(Uuid::new_v4()),
                    email: Set(identity.email.clone()),
                    name: Set(identity
                        .name
                        .clone()
                        .unwrap_or_else(|| identity.email.clone())),
                    picture: Set(identity.picture.clone()),
                    password_hash: ActiveValue::not_set(),
                    email_verified: Set(true),
                    email_verification_token: ActiveValue::not_set(),
                    role: Set(users::UserRole::Member),
                    status: Set(UserStatus::Active),
                    created_at: ActiveValue::not_set(), // Will use database default
                    last_login_at: ActiveValue::not_set(), // Will use database default
                };

                let user = new_user
                    .insert(&connection)
                    .await
                    .map_err(|e| OxyError::DBError(format!("Failed to create user: {e}")))?;

                tracing::info!(
                    "Created new user: {} ({}) with role: {}",
                    user.email,
                    user.id,
                    user.role.as_str()
                );
                Ok(user.into())
            }
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
