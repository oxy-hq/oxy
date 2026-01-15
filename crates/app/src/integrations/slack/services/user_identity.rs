//! Slack user identity service

use crate::integrations::slack::client::SlackClient;
use entity::prelude::{SlackUserIdentities, Users};
use entity::{slack_user_identities, users};
use oxy::database::client::establish_connection;
use oxy_shared::errors::OxyError;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

pub struct UserIdentityService;

impl UserIdentityService {
    /// Link a Slack user to an Oxy user
    pub async fn link_user(
        slack_team_id: String,
        slack_user_id: String,
        oxy_user_id: Uuid,
    ) -> Result<slack_user_identities::Model, OxyError> {
        let conn = establish_connection().await?;

        // Check if link already exists
        let existing = SlackUserIdentities::find()
            .filter(slack_user_identities::Column::SlackTeamId.eq(&slack_team_id))
            .filter(slack_user_identities::Column::SlackUserId.eq(&slack_user_id))
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?;

        if let Some(existing_model) = existing {
            // Update existing link
            let mut active_model: slack_user_identities::ActiveModel = existing_model.into();
            active_model.oxy_user_id = ActiveValue::Set(oxy_user_id);
            active_model.last_seen_at = ActiveValue::NotSet; // Will use current timestamp

            active_model
                .update(&conn)
                .await
                .map_err(|e| OxyError::DBError(e.to_string()))
        } else {
            // Create new link
            let new_identity = slack_user_identities::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                slack_team_id: ActiveValue::Set(slack_team_id),
                slack_user_id: ActiveValue::Set(slack_user_id),
                oxy_user_id: ActiveValue::Set(oxy_user_id),
                linked_at: ActiveValue::NotSet,
                last_seen_at: ActiveValue::NotSet,
            };

            new_identity
                .insert(&conn)
                .await
                .map_err(|e| OxyError::DBError(e.to_string()))
        }
    }

    /// Get Slack user's email address
    ///
    /// # Arguments
    /// * `bot_token` - Slack bot token for API calls
    /// * `user_id` - Slack user ID
    ///
    /// # Returns
    /// User's email address if available
    pub async fn get_user_email(
        bot_token: &str,
        user_id: &str,
    ) -> Result<Option<String>, OxyError> {
        let client = SlackClient::new();
        let user_info = client.get_user_info(bot_token, user_id).await?;
        Ok(user_info.user.and_then(|u| u.profile).and_then(|p| p.email))
    }

    /// Ensure a Slack user is linked to an Oxy user
    ///
    /// Returns the Oxy user ID. If no link exists, attempts to match by email
    /// before falling back to the first available user (for local development).
    ///
    /// # Arguments
    /// * `bot_token` - Slack bot token for API calls (used to fetch user email)
    /// * `slack_team_id` - Slack team/workspace ID
    /// * `slack_user_id` - Slack user ID
    pub async fn ensure_link(
        bot_token: &str,
        slack_team_id: &str,
        slack_user_id: &str,
    ) -> Result<Uuid, OxyError> {
        let conn = establish_connection().await?;

        let identity = SlackUserIdentities::find()
            .filter(slack_user_identities::Column::SlackTeamId.eq(slack_team_id))
            .filter(slack_user_identities::Column::SlackUserId.eq(slack_user_id))
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?;

        match identity {
            Some(model) => {
                // Update last_seen_at
                let mut active_model: slack_user_identities::ActiveModel = model.clone().into();
                active_model.last_seen_at = ActiveValue::NotSet; // Will use current timestamp
                if let Err(e) = active_model.update(&conn).await {
                    tracing::warn!(
                        "Failed to update last_seen_at for Slack user (team={}, user={}): {}",
                        slack_team_id,
                        slack_user_id,
                        e
                    );
                }

                Ok(model.oxy_user_id)
            }
            None => {
                // Try to match by email first
                // Handle API failures gracefully - if we can't get email, continue with fallback
                let slack_email = match Self::get_user_email(bot_token, slack_user_id).await {
                    Ok(email) => email,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to get Slack user email (team={}, user={}), using fallback user: {}",
                            slack_team_id,
                            slack_user_id,
                            e
                        );
                        None
                    }
                };

                let matched_user = if let Some(ref email) = slack_email {
                    tracing::info!(
                        "Attempting to match Slack user by email: team={}, user={}, email={}",
                        slack_team_id,
                        slack_user_id,
                        email
                    );

                    // Look for Oxy user with matching email
                    Users::find()
                        .filter(users::Column::Email.eq(email))
                        .one(&conn)
                        .await
                        .map_err(|e| OxyError::DBError(e.to_string()))?
                } else {
                    tracing::info!(
                        "No email found for Slack user: team={}, user={}",
                        slack_team_id,
                        slack_user_id
                    );
                    None
                };

                let oxy_user = if let Some(user) = matched_user {
                    tracing::info!(
                        "Matched Slack user to Oxy user by email: oxy_user_id={}",
                        user.id
                    );
                    user
                } else {
                    // No match - fall back to default user (for local development)
                    if let Some(ref email) = slack_email {
                        tracing::warn!(
                            "No Oxy user found with email '{}' for Slack user (team={}, user={}). Using default user.",
                            email,
                            slack_team_id,
                            slack_user_id
                        );
                    } else {
                        tracing::info!(
                            "Auto-linking Slack user to default Oxy user (no email available): team={}, user={}",
                            slack_team_id,
                            slack_user_id
                        );
                    }

                    Users::find()
                        .order_by_asc(users::Column::CreatedAt)
                        .one(&conn)
                        .await
                        .map_err(|e| OxyError::DBError(e.to_string()))?
                        .ok_or_else(|| {
                            OxyError::DBError("No users found in database".to_string())
                        })?
                };

                // Create link
                Self::link_user(
                    slack_team_id.to_string(),
                    slack_user_id.to_string(),
                    oxy_user.id,
                )
                .await?;

                Ok(oxy_user.id)
            }
        }
    }
}
