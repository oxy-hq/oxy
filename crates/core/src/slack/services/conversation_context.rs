//! Slack conversation context service

use crate::db::client::establish_connection;
use crate::errors::OxyError;
use entity::prelude::SlackConversationContexts;
use entity::slack_conversation_contexts;
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

pub struct ConversationContextService;

impl ConversationContextService {
    /// Bind a Slack thread to an Oxy session
    pub async fn bind_thread(
        slack_team_id: String,
        slack_channel_id: String,
        slack_thread_ts: String,
        oxy_session_id: Uuid,
    ) -> Result<slack_conversation_contexts::Model, OxyError> {
        let conn = establish_connection().await?;

        let new_context = slack_conversation_contexts::ActiveModel {
            id: ActiveValue::Set(Uuid::new_v4()),
            slack_team_id: ActiveValue::Set(slack_team_id),
            slack_channel_id: ActiveValue::Set(slack_channel_id),
            slack_thread_ts: ActiveValue::Set(slack_thread_ts),
            oxy_session_id: ActiveValue::Set(oxy_session_id),
            last_slack_message_ts: ActiveValue::Set(None),
            created_at: ActiveValue::NotSet,
            updated_at: ActiveValue::NotSet,
        };

        new_context
            .insert(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))
    }

    /// Find Oxy session ID for a Slack thread
    pub async fn find_session_for_thread(
        slack_team_id: &str,
        slack_channel_id: &str,
        slack_thread_ts: &str,
    ) -> Result<Option<Uuid>, OxyError> {
        Ok(
            Self::get_context(slack_team_id, slack_channel_id, slack_thread_ts)
                .await?
                .map(|c| c.oxy_session_id),
        )
    }

    /// Get conversation context for a Slack thread
    pub async fn get_context(
        slack_team_id: &str,
        slack_channel_id: &str,
        slack_thread_ts: &str,
    ) -> Result<Option<slack_conversation_contexts::Model>, OxyError> {
        let conn = establish_connection().await?;

        SlackConversationContexts::find()
            .filter(slack_conversation_contexts::Column::SlackTeamId.eq(slack_team_id))
            .filter(slack_conversation_contexts::Column::SlackChannelId.eq(slack_channel_id))
            .filter(slack_conversation_contexts::Column::SlackThreadTs.eq(slack_thread_ts))
            .one(&conn)
            .await
            .map_err(|e| OxyError::DBError(e.to_string()))
    }

    /// Update the last Slack message timestamp for a context
    pub async fn update_last_message_ts(
        slack_team_id: &str,
        slack_channel_id: &str,
        slack_thread_ts: &str,
        last_message_ts: String,
    ) -> Result<(), OxyError> {
        if let Some(model) =
            Self::get_context(slack_team_id, slack_channel_id, slack_thread_ts).await?
        {
            let conn = establish_connection().await?;
            let mut active_model: slack_conversation_contexts::ActiveModel = model.into();
            active_model.last_slack_message_ts = ActiveValue::Set(Some(last_message_ts));
            active_model.updated_at = ActiveValue::NotSet; // Will use current timestamp

            active_model
                .update(&conn)
                .await
                .map_err(|e| OxyError::DBError(e.to_string()))?;
        }

        Ok(())
    }
}
