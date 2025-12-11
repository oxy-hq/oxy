//! Slack channel binding service

use crate::db::client::establish_connection;
use crate::errors::OxyError;
use entity::prelude::SlackChannelBindings;
use entity::slack_channel_bindings;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

pub struct ChannelBindingService;

impl ChannelBindingService {
    /// Bind a Slack channel to an Oxy project and agent
    pub async fn bind_channel(
        slack_team_id: String,
        slack_channel_id: String,
        oxy_project_id: Uuid,
        default_agent_id: String,
        created_by_slack_user_id: String,
    ) -> Result<slack_channel_bindings::Model, OxyError> {
        let conn = establish_connection().await?;

        // Check if binding already exists
        let existing = SlackChannelBindings::find()
            .filter(slack_channel_bindings::Column::SlackTeamId.eq(&slack_team_id))
            .filter(slack_channel_bindings::Column::SlackChannelId.eq(&slack_channel_id))
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?;

        if let Some(existing_model) = existing {
            // Update existing binding
            let mut active_model: slack_channel_bindings::ActiveModel = existing_model.into();
            active_model.oxy_project_id = ActiveValue::Set(oxy_project_id);
            active_model.default_agent_id = ActiveValue::Set(default_agent_id);
            active_model.created_by_slack_user_id = ActiveValue::Set(created_by_slack_user_id);

            active_model
                .update(&conn)
                .await
                .map_err(|e| OxyError::DBError(e.to_string()))
        } else {
            // Create new binding
            let new_binding = slack_channel_bindings::ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                slack_team_id: ActiveValue::Set(slack_team_id),
                slack_channel_id: ActiveValue::Set(slack_channel_id),
                oxy_project_id: ActiveValue::Set(oxy_project_id),
                default_agent_id: ActiveValue::Set(default_agent_id),
                created_by_slack_user_id: ActiveValue::Set(created_by_slack_user_id),
                created_at: ActiveValue::NotSet,
            };

            new_binding
                .insert(&conn)
                .await
                .map_err(|e| OxyError::DBError(e.to_string()))
        }
    }

    /// Unbind a Slack channel
    pub async fn unbind_channel(
        slack_team_id: &str,
        slack_channel_id: &str,
    ) -> Result<(), OxyError> {
        let conn = establish_connection().await?;

        let binding = SlackChannelBindings::find()
            .filter(slack_channel_bindings::Column::SlackTeamId.eq(slack_team_id))
            .filter(slack_channel_bindings::Column::SlackChannelId.eq(slack_channel_id))
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))?;

        if let Some(model) = binding {
            let active_model: slack_channel_bindings::ActiveModel = model.into();
            active_model
                .delete(&conn)
                .await
                .map_err(|e| OxyError::DBError(e.to_string()))?;
        }

        Ok(())
    }

    /// Find binding for a Slack channel
    pub async fn find_binding(
        slack_team_id: &str,
        slack_channel_id: &str,
    ) -> Result<Option<slack_channel_bindings::Model>, OxyError> {
        let conn = establish_connection().await?;

        SlackChannelBindings::find()
            .filter(slack_channel_bindings::Column::SlackTeamId.eq(slack_team_id))
            .filter(slack_channel_bindings::Column::SlackChannelId.eq(slack_channel_id))
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))
    }
}
