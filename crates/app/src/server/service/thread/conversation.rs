//! Generic Oxy-thread lifecycle helpers — surface-agnostic.
//!
//! Each surface that hosts an agent conversation (Slack, web, CLI when it
//! grows persistence) needs the same primitives: create the thread, persist
//! the user's message, load the memory window for the agent, persist the
//! agent's output once it finishes, and update the thread's terminal state.
//!
//! Before this module existed, these primitives lived inside
//! `slack/events/execution.rs` as private helpers. That meant any future
//! surface had to either duplicate them or call into a Slack-specific
//! module — both of which contradict the "surface-agnostic core, surface-
//! specific shell" architecture we're moving toward.
//!
//! The helpers here speak only in terms of `Uuid`s, `&str`s, and the
//! shared [`Message`] / [`BlockHandlerReader`] types. They do NOT know
//! about Slack threads, SSE channels, terminal output — the surface
//! orchestrators handle that.

use entity::threads;
use oxy::database::client::establish_connection;
use oxy_shared::errors::OxyError;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use uuid::Uuid;

use crate::server::service::agent::Message;
use crate::server::service::formatters::BlockHandlerReader;

/// Memory window: last N messages handed to the agent as conversation
/// history. Kept intentionally small — agent context tokens are
/// expensive and the relevant signal for follow-ups is the recent
/// turns, not the entire backlog.
const MEMORY_WINDOW: u64 = 10;

/// Create a new Oxy thread row.
///
/// The `title` is what the web UI's threads list shows. Surface
/// orchestrators decide the prefix convention (e.g. Slack tags with
/// `"Slack: …"`); this helper just stores whatever they pass.
pub async fn create_thread(
    workspace_id: Uuid,
    user_id: Uuid,
    title: &str,
    input: &str,
    agent_path: &str,
) -> Result<Uuid, OxyError> {
    let conn = establish_connection().await?;
    let new_thread = threads::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        user_id: ActiveValue::Set(Some(user_id)),
        created_at: ActiveValue::NotSet,
        title: ActiveValue::Set(title.to_string()),
        input: ActiveValue::Set(input.to_string()),
        output: ActiveValue::Set(String::new()),
        source_type: ActiveValue::Set("agent".to_string()),
        source: ActiveValue::Set(agent_path.to_string()),
        references: ActiveValue::Set("[]".to_string()),
        is_processing: ActiveValue::Set(true),
        project_id: ActiveValue::Set(workspace_id),
        sandbox_info: ActiveValue::Set(None),
    };
    let thread = new_thread
        .insert(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;
    Ok(thread.id)
}

/// Persist the user's input as a `messages` row tagged `is_human=true`.
pub async fn persist_user_message(thread_id: Uuid, content: &str) -> Result<Uuid, OxyError> {
    persist_message(thread_id, content, true).await
}

/// Persist a plain-text agent reply (used on error paths where there
/// are no structured blocks to flatten — the error string IS the body).
pub async fn persist_plain_agent_message(thread_id: Uuid, content: &str) -> Result<Uuid, OxyError> {
    persist_message(thread_id, content, false).await
}

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

/// Walk the `BlockHandler`'s captured tree (the structured agent output
/// minus reasoning) into a `messages` row + zero-or-more `artifacts`
/// rows. Returns `(message_id, message_content, artifact_count)`.
pub async fn persist_agent_output_from_blocks(
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
            .map_err(|e| OxyError::DBError(format!("Failed to insert artifact: {e}")))?;
        count += 1;
    }

    Ok((message_id, message_content, count))
}

/// Load the last `MEMORY_WINDOW` messages for a thread, oldest-first
/// (so the agent's context array reads chronologically).
pub async fn load_memory(thread_id: Uuid) -> Result<Vec<Message>, OxyError> {
    let conn = establish_connection().await?;
    let messages = entity::prelude::Messages::find()
        .filter(entity::messages::Column::ThreadId.eq(thread_id))
        .order_by(entity::messages::Column::CreatedAt, sea_orm::Order::Desc)
        .limit(MEMORY_WINDOW)
        .all(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;

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

/// Update the thread's terminal columns once the agent run finishes.
/// `is_processing` flips to `false` when the run is complete (success or
/// failure); leaving it at `true` is reserved for partial-state diagnostics.
pub async fn update_thread_with_output(
    thread_id: Uuid,
    output: &str,
    is_processing: bool,
) -> Result<(), OxyError> {
    let conn = establish_connection().await?;
    let thread = entity::prelude::Threads::find_by_id(thread_id)
        .one(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?
        .ok_or_else(|| OxyError::DBError("Thread not found".to_string()))?;

    let mut active_thread: threads::ActiveModel = thread.into();
    active_thread.output = ActiveValue::Set(output.to_string());
    active_thread.is_processing = ActiveValue::Set(is_processing);
    active_thread
        .update(&conn)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;
    Ok(())
}
