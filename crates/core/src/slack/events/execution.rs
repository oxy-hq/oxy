//! Shared Oxy execution logic for Slack events
//!
//! This module contains the common execution flow used by both @mentions
//! and DM messages to interact with Oxy chat. It uses the ChatService
//! directly for conversation context and uses Slack's native streaming APIs.
//!
//! We use BlockHandler to capture artifacts (SQL queries, etc.) and persist them
//! to the database so they appear on the thread page in the web app.

use crate::adapters::project::builder::ProjectBuilder;
use crate::adapters::project::resolve_project_path;
use crate::adapters::secrets::SecretsManager;
use crate::config::model::SlackSettings;
use crate::config::{ConfigBuilder, resolve_local_project_path};
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::service::agent::{Message, run_agent};
use crate::service::formatters::{BlockHandler, BlockHandlerReader};
use crate::service::types::AnswerStream;
use crate::slack::client::SlackClient;
use crate::slack::mrkdwn::markdown_to_mrkdwn;
use crate::slack::services::{ConversationContextService, UserIdentityService};
use entity::threads;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use uuid::Uuid;

const WORKING_ON_IT_STATUS: &str = "🤔 Working on it...";

// ============================================================================
// Public Types
// ============================================================================

/// Request parameters for executing Oxy chat from Slack
///
/// This struct consolidates all the parameters needed to execute an agent
/// from a Slack event. It uses owned strings for simplicity and async-friendliness.
pub struct SlackChatRequest {
    /// Slack team/workspace ID
    pub team_id: String,
    /// Slack channel or DM ID
    pub channel_id: String,
    /// Slack user ID who sent the message
    pub user_id: String,
    /// The message text (already cleaned of bot mentions if applicable)
    pub text: String,
    /// Thread timestamp (for thread replies)
    pub thread_ts: Option<String>,
    /// Event timestamp
    pub event_ts: String,
    /// Oxy project ID (nil UUID for local projects)
    pub project_id: Uuid,
    /// Agent config path
    pub agent_id: String,
    /// Slack settings from config.yml
    pub slack_settings: SlackSettings,
    /// Whether this is a DM (affects delivery method)
    pub is_dm: bool,
}

// ============================================================================
// Public API
// ============================================================================

/// Load Slack settings from project config.yml
///
/// Returns the SlackSettings or an error with helpful instructions.
pub async fn load_slack_settings() -> Result<SlackSettings, OxyError> {
    let project_path = resolve_local_project_path().map_err(|e| {
        OxyError::ConfigurationError(format!(
            "Failed to find project config.yml: {}. \
             Make sure to run Oxy from a directory with a config.yml file.",
            e
        ))
    })?;

    let config_manager = ConfigBuilder::new()
        .with_project_path(project_path)?
        .build()
        .await
        .map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to load project config: {}", e))
        })?;

    config_manager.get_config().slack.clone().ok_or_else(|| {
        OxyError::ConfigurationError(
            "Slack integration requires 'slack' section in config.yml. \
                 Example:\n  slack:\n    default_agent: agents/sql-generator.agent.yml\n    \
                 bot_token_var: SLACK_BOT_TOKEN\n    signing_secret_var: SLACK_SIGNING_SECRET"
                .to_string(),
        )
    })
}

/// Execute Oxy chat request and stream response to Slack
///
/// This is the core function that handles the full Oxy execution flow:
/// 1. Resolves Oxy user from Slack user
/// 2. Creates or reuses chat session/thread
/// 3. Posts initial "Working on it..." message
/// 4. Executes agent and streams response
/// 5. Updates Slack with final answer and deep link
///
/// This function is used by both app_mention and message_im handlers.
pub async fn execute_oxy_chat_for_slack(request: SlackChatRequest) -> Result<(), OxyError> {
    tracing::info!(
        "Executing Oxy chat: team={}, channel={}, user={}, project={}, agent={}",
        request.team_id,
        request.channel_id,
        request.user_id,
        request.project_id,
        request.agent_id
    );

    // Determine thread timestamp
    // For both DMs and channels, use thread_ts if present, otherwise event_ts
    // Note: AI sidebar in DMs uses threads! Each conversation is a separate thread.
    let slack_thread_ts = request.thread_ts.as_deref().unwrap_or(&request.event_ts);

    // Step 1: Resolve bot token from settings
    let secrets_manager = SecretsManager::from_environment()?;
    let bot_token = request
        .slack_settings
        .get_bot_token(&secrets_manager)
        .await?;

    // Step 2: Resolve Oxy user (requires bot token for email lookup)
    let oxy_user_id =
        UserIdentityService::ensure_link(&bot_token, &request.team_id, &request.user_id).await?;
    tracing::info!("Slack user linked to Oxy user: {}", oxy_user_id);

    // Step 3: Check if session already exists for this thread
    let oxy_session_id = ConversationContextService::find_session_for_thread(
        &request.team_id,
        &request.channel_id,
        slack_thread_ts,
    )
    .await?;

    let thread_id = if let Some(session_id) = oxy_session_id {
        tracing::info!("Reusing existing Oxy session: {}", session_id);
        session_id
    } else {
        // Create new thread/session
        tracing::info!("Creating new Oxy session");
        let thread_id = create_oxy_thread(
            request.project_id,
            oxy_user_id,
            &request.text,
            &request.agent_id,
        )
        .await?;

        // Bind thread to Slack conversation context
        ConversationContextService::bind_thread(
            request.team_id.clone(),
            request.channel_id.clone(),
            slack_thread_ts.to_string(),
            thread_id,
        )
        .await?;

        thread_id
    };

    // Step 3: Execute agent and send response to Slack
    let client = SlackClient::new();
    let delivery = determine_delivery_method(
        &client,
        &bot_token,
        &request.channel_id,
        slack_thread_ts,
        &request.team_id,
        request.is_dm,
    )
    .await;

    // Set loading status if needed
    if matches!(delivery, SlackDeliveryMethod::Fallback { .. }) {
        let _ = client
            .set_thread_status(
                &bot_token,
                &request.channel_id,
                slack_thread_ts,
                WORKING_ON_IT_STATUS,
            )
            .await;
    }

    // Execute agent and deliver to Slack
    let oxy_app_url = request.slack_settings.oxy_app_url.as_deref();
    let result = execute_and_deliver_to_slack(
        thread_id,
        &request.text,
        request.project_id,
        &request.agent_id,
        &bot_token,
        oxy_app_url,
        &client,
        &delivery,
    )
    .await;

    // Clear status and update context
    if matches!(delivery, SlackDeliveryMethod::Fallback { .. }) {
        let _ = client
            .set_thread_status(&bot_token, &request.channel_id, slack_thread_ts, "")
            .await;
    }

    let last_message_ts = match &delivery {
        SlackDeliveryMethod::Native { stream_id } => stream_id.clone(),
        SlackDeliveryMethod::Fallback { .. } => slack_thread_ts.to_string(),
    };

    ConversationContextService::update_last_message_ts(
        &request.team_id,
        &request.channel_id,
        slack_thread_ts,
        last_message_ts,
    )
    .await?;

    result
}

// ============================================================================
// Slack Delivery
// ============================================================================

/// How to deliver messages to Slack
enum SlackDeliveryMethod {
    /// Native streaming: append_stream + stop_stream
    Native { stream_id: String },
    /// Fallback: post_message
    Fallback {
        channel_id: String,
        thread_ts: String,
    },
}

/// Determine the best delivery method for this Slack message
async fn determine_delivery_method(
    client: &SlackClient,
    bot_token: &str,
    channel_id: &str,
    thread_ts: &str,
    team_id: &str,
    is_dm: bool,
) -> SlackDeliveryMethod {
    // DMs must use fallback (Slack creates a placeholder that expects post_message)
    if is_dm {
        tracing::info!("Using fallback delivery for DM");
        return SlackDeliveryMethod::Fallback {
            channel_id: channel_id.to_string(),
            thread_ts: thread_ts.to_string(),
        };
    }

    // Try native streaming for channel mentions
    match client
        .start_stream(bot_token, channel_id, Some(thread_ts), Some(team_id))
        .await
    {
        Ok(stream_id) => {
            tracing::info!("Using native streaming with stream_id: {}", stream_id);
            SlackDeliveryMethod::Native { stream_id }
        }
        Err(e) => {
            tracing::warn!("Native streaming unavailable ({}), using fallback", e);
            SlackDeliveryMethod::Fallback {
                channel_id: channel_id.to_string(),
                thread_ts: thread_ts.to_string(),
            }
        }
    }
}

/// Execute agent and deliver response to Slack
#[allow(clippy::too_many_arguments)]
async fn execute_and_deliver_to_slack(
    thread_id: Uuid,
    question: &str,
    project_id: Uuid,
    agent_path: &str,
    bot_token: &str,
    oxy_app_url: Option<&str>,
    client: &SlackClient,
    delivery: &SlackDeliveryMethod,
) -> Result<(), OxyError> {
    // Persist user message
    let message_id = persist_user_message(thread_id, question).await?;
    tracing::debug!(
        "Persisted user message {} to thread {}",
        message_id,
        thread_id
    );

    // Execute agent
    let result = execute_agent(thread_id, question, project_id, agent_path).await;

    match result {
        Ok((final_markdown, block_handler_reader)) => {
            // Persist agent response and artifacts
            let (agent_message_id, _, artifact_count) =
                persist_agent_output_from_blocks(thread_id, block_handler_reader).await?;
            tracing::info!(
                "Persisted agent response {} with {} artifacts",
                agent_message_id,
                artifact_count
            );

            // Build Slack message, optionally with deep link if oxy_app_url is configured.
            // Note: in the case of a thread on a Slack channel with Oxy, this link will
            // technically be accessible only to the user who initiated the thread. This may
            // be relevant for cloud deployments eventually (TODO), but for local
            // deployments, there is de facto a single Oxy user and this is not a concern.
            let mrkdwn_content = markdown_to_mrkdwn(&final_markdown);
            let slack_text = if let Some(url) = oxy_app_url {
                let deep_link = build_thread_deep_link(url, project_id, thread_id);
                format!("{}\n\n<{}|View in Oxy>", mrkdwn_content, deep_link)
            } else {
                mrkdwn_content
            };

            // Deliver to Slack
            deliver_message(client, bot_token, delivery, &slack_text).await;

            // Update thread in database
            update_thread_with_output(thread_id, &final_markdown).await?;

            Ok(())
        }
        Err(e) => {
            let error_msg = format!("❌ Error: {}", e);

            // Persist and deliver error
            let _ = persist_plain_agent_message(thread_id, &error_msg).await;
            deliver_message(client, bot_token, delivery, &error_msg).await;
            update_thread_with_output(thread_id, &error_msg).await?;

            Err(e)
        }
    }
}

/// Deliver a message to Slack using the appropriate method
async fn deliver_message(
    client: &SlackClient,
    token: &str,
    delivery: &SlackDeliveryMethod,
    text: &str,
) {
    match delivery {
        SlackDeliveryMethod::Native { stream_id } => {
            if let Err(e) = client.append_stream(token, stream_id, text).await {
                tracing::error!("Failed to append to stream: {}", e);
            }
            if let Err(e) = client.stop_stream(token, stream_id).await {
                tracing::error!("Failed to stop stream: {}", e);
            }
        }
        SlackDeliveryMethod::Fallback {
            channel_id,
            thread_ts,
        } => {
            if let Err(e) = client
                .post_message(token, channel_id, text, Some(thread_ts))
                .await
            {
                tracing::error!("Failed to post message: {}", e);
            }
        }
    }
}

// ============================================================================
// Agent Execution
// ============================================================================

/// Execute the agent and return the result with block handler for artifact extraction
async fn execute_agent(
    thread_id: Uuid,
    question: &str,
    project_id: Uuid,
    agent_path: &str,
) -> Result<(String, BlockHandlerReader), OxyError> {
    let repo_path = resolve_project_path(project_id.clone()).await?;
    let project_manager = ProjectBuilder::new(project_id)
        .with_project_path_and_fallback_config(&repo_path)
        .await?
        .build()
        .await?;
    let memory = load_conversation_memory(thread_id).await?;

    tracing::debug!("Loaded {} messages for conversation context", memory.len());

    let (block_handler, block_handler_reader) = create_block_handler();

    let result = run_agent(
        project_manager,
        std::path::Path::new(agent_path),
        question.to_string(),
        block_handler,
        memory,
        None,
        None,
        None,
        None,
        None,
    )
    .await?;

    let final_markdown = result.to_markdown();
    let final_markdown = if final_markdown.trim().is_empty() {
        "✅ Task completed".to_string()
    } else {
        final_markdown
    };

    Ok((final_markdown, block_handler_reader))
}

/// Create a BlockHandler for capturing artifacts
///
/// Returns (handler, reader) where the handler is passed to run_agent
/// and the reader is used to extract artifacts afterward.
fn create_block_handler() -> (BlockHandler, BlockHandlerReader) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AnswerStream>(100);
    let handler = BlockHandler::new(tx);
    let reader = handler.get_reader();

    // Spawn task to drain the channel (we only need artifacts, not streaming events).
    // This task terminates when BlockHandler is dropped (which drops the sender tx,
    // causing rx.recv() to return None and exit the loop).
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    (handler, reader)
}

// ============================================================================
// Database Operations
// ============================================================================

/// Create a new Oxy thread for the Slack conversation
async fn create_oxy_thread(
    project_id: Uuid,
    user_id: Uuid,
    input: &str,
    agent_id: &str,
) -> Result<Uuid, OxyError> {
    let conn = establish_connection().await?;

    let new_thread = threads::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        user_id: ActiveValue::Set(Some(user_id)),
        created_at: ActiveValue::NotSet,
        title: ActiveValue::Set(format!("Slack: {}", truncate(input, 50))),
        input: ActiveValue::Set(input.to_string()),
        output: ActiveValue::Set(String::new()),
        source_type: ActiveValue::Set("agent".to_string()),
        source: ActiveValue::Set(agent_id.to_string()),
        references: ActiveValue::Set("[]".to_string()),
        is_processing: ActiveValue::Set(true),
        project_id: ActiveValue::Set(project_id),
        sandbox_info: ActiveValue::Set(None),
    };

    let thread = new_thread
        .insert(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

    Ok(thread.id)
}

/// Persist a user message to the database
async fn persist_user_message(thread_id: Uuid, content: &str) -> Result<Uuid, OxyError> {
    persist_message(thread_id, content, true).await
}

/// Persist an agent message to the database
async fn persist_plain_agent_message(thread_id: Uuid, content: &str) -> Result<Uuid, OxyError> {
    persist_message(thread_id, content, false).await
}

/// Persist a message to the database
async fn persist_message(thread_id: Uuid, content: &str, is_human: bool) -> Result<Uuid, OxyError> {
    let conn = establish_connection().await?;
    let message_id = Uuid::new_v4();

    let new_message = entity::messages::ActiveModel {
        id: ActiveValue::Set(message_id),
        thread_id: ActiveValue::Set(thread_id),
        content: ActiveValue::Set(content.to_string()),
        is_human: ActiveValue::Set(is_human),
        created_at: ActiveValue::NotSet,
        input_tokens: ActiveValue::Set(0),
        output_tokens: ActiveValue::Set(0),
    };

    new_message
        .insert(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

    Ok(message_id)
}

/// Persist agent output and artifacts collected via BlockHandler
async fn persist_agent_output_from_blocks(
    thread_id: Uuid,
    block_handler_reader: BlockHandlerReader,
) -> Result<(Uuid, String, usize), OxyError> {
    let conn = establish_connection().await?;

    let (mut message_model, artifacts, _sandbox_info) =
        block_handler_reader.into_active_models().await?;
    message_model.thread_id = ActiveValue::Set(thread_id);
    message_model.is_human = ActiveValue::Set(false);

    let message_id = match message_model.id.clone() {
        ActiveValue::Set(id) => id,
        _ => {
            let new_id = Uuid::new_v4();
            message_model.id = ActiveValue::Set(new_id);
            new_id
        }
    };

    let message_content = match &message_model.content {
        ActiveValue::Set(content) | ActiveValue::Unchanged(content) => content.clone(),
        _ => String::new(),
    };

    message_model
        .insert(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

    let mut count = 0;
    for mut artifact in artifacts {
        artifact.thread_id = ActiveValue::Set(thread_id);
        artifact.message_id = ActiveValue::Set(message_id);
        artifact
            .insert(&conn)
            .await
            .map_err(|e| OxyError::DBError(format!("Failed to insert artifact: {}", e)))?;
        count += 1;
    }

    Ok((message_id, message_content, count))
}

/// Load conversation memory (last 10 messages) from the thread
async fn load_conversation_memory(thread_id: Uuid) -> Result<Vec<Message>, OxyError> {
    let conn = establish_connection().await?;

    let messages = entity::prelude::Messages::find()
        .filter(entity::messages::Column::ThreadId.eq(thread_id))
        .order_by(entity::messages::Column::CreatedAt, sea_orm::Order::Desc)
        .limit(10)
        .all(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

    // Reverse to get chronological order
    Ok(messages
        .into_iter()
        .rev()
        .map(|m| Message {
            content: m.content,
            is_human: m.is_human,
            created_at: m.created_at,
        })
        .collect())
}

/// Update thread with output text
async fn update_thread_with_output(thread_id: Uuid, output: &str) -> Result<(), OxyError> {
    let conn = establish_connection().await?;

    let thread = entity::prelude::Threads::find_by_id(thread_id)
        .one(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?
        .ok_or_else(|| OxyError::DBError("Thread not found".to_string()))?;

    let mut active_thread: threads::ActiveModel = thread.into();
    active_thread.output = ActiveValue::Set(output.to_string());
    active_thread.is_processing = ActiveValue::Set(false);

    active_thread
        .update(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

    Ok(())
}

// ============================================================================
// Utilities
// ============================================================================

/// Build a deep link to the thread in the Oxy web app
fn build_thread_deep_link(base_url: &str, project_id: Uuid, thread_id: Uuid) -> String {
    if project_id.is_nil() {
        format!("{}/threads/{}", base_url, thread_id)
    } else {
        format!("{}/projects/{}/threads/{}", base_url, project_id, thread_id)
    }
}

/// Truncate string to max length
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}
