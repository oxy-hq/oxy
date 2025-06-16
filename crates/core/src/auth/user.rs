use crate::{db::client::establish_connection, errors::OxyError};
use entity::prelude::Users;
use entity::users;
use sea_orm::{ActiveValue, EntityTrait, Set, prelude::*};
use uuid::Uuid;

use super::types::{AuthenticatedUser, Identity};

pub struct UserService;

impl UserService {
    /// Find or create a user based on IAP claims
    pub async fn get_or_create_user(identity: &Identity) -> Result<AuthenticatedUser, OxyError> {
        let connection = establish_connection().await;

        match Users::find()
            .filter(users::Column::Email.eq(&identity.email))
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {}", e)))?
        {
            Some(existing_users) => Ok(existing_users.into()),
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
                    created_at: ActiveValue::not_set(), // Will use database default
                    last_login_at: ActiveValue::not_set(), // Will use database default
                };

                let user = new_user
                    .insert(&connection)
                    .await
                    .map_err(|e| OxyError::DBError(format!("Failed to create user: {}", e)))?;

                tracing::info!("Created new user: {} ({})", user.email, user.id);
                Ok(user.into())
            }
        }
    }

    /// Update user profile information
    pub async fn update_user_profile(
        user_id: Uuid,
        name: Option<String>,
        picture: Option<String>,
    ) -> Result<AuthenticatedUser, OxyError> {
        let connection = establish_connection().await;

        let user = Users::find_by_id(user_id)
            .one(&connection)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to query user: {}", e)))?
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
            .map_err(|e| OxyError::DBError(format!("Failed to update user: {}", e)))?;

        Ok(updated_user.into())
    }
}
