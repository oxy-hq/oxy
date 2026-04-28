//! Shared Oxy execution logic for Slack events.
//!
//! This module handles the full execution flow when a Slack user sends a message:
//! creating/reusing threads, persisting messages/artifacts, running the agent,
//! and posting the response as a Block Kit message.
//!
//! UX shape (intentionally Claude-like):
//!
//!   1. `assistant.threads.setStatus` with rotating `loading_messages` shows a
//!      "is working on your request…" indicator while the agent runs.
//!   2. The agent's prose text is **NOT streamed** — we accumulate it as it
//!      arrives and only post the finished result at the end. Live token reveal
//!      looked cramped against artifacts + chart placeholders, and Claude's
//!      all-at-once finish reads better.
//!   3. `chat.postMessage` delivers the final answer as a Block Kit payload:
//!      section blocks for prose, image blocks for chart URLs, a divider, and a
//!      footer card with a "View thread" deep-link. Slack auto-clears the
//!      setStatus indicator when the bot posts a reply.

use base64::Engine;

use crate::integrations::slack::blocks;
use crate::integrations::slack::client::SlackClient;
use crate::integrations::slack::config::SlackConfig;
use crate::integrations::slack::error::SlackError;
use crate::integrations::slack::render::SlackRenderer;
use crate::integrations::slack::resolution::thread_context::{
    CreateThreadContext, ThreadContextService,
};
use crate::server::service::agent::{ExecutionSource, Message, run_agent};
use crate::server::service::formatters::BlockHandler;
use crate::server::service::thread::conversation;
use entity::slack_installations::Model as InstallationRow;
use entity::slack_user_links::Model as UserLinkRow;
use oxy::adapters::workspace::builder::WorkspaceBuilder;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::adapters::workspace::resolve_workspace_path;
use oxy::database::client::establish_connection;
use oxy::types::AnswerStream;
use oxy_shared::errors::OxyError;
use sea_orm::EntityTrait;
use tokio::sync::mpsc;
use uuid::Uuid;

// ============================================================================
// Public Types
// ============================================================================

/// All parameters needed to execute the agent from a Slack event.
pub struct SlackRunRequest {
    pub installation: InstallationRow,
    /// Pre-decrypted bot token — avoids a redundant AES-GCM decrypt per event.
    pub bot_token: String,
    pub user_link: UserLinkRow,
    pub workspace_id: Uuid,
    pub agent_path: String,
    pub question: String,
    pub channel_id: String,
    pub thread_ts: String,
}

// ============================================================================
// Public API
// ============================================================================

/// Execute an agent from a Slack event and deliver the response.
///
/// Steps:
/// 1. Find-or-create (or update-workspace on) the slack_threads row + Oxy thread
/// 2. Persist user message
/// 3. Load conversation memory
/// 4. `setStatus` with rotating loading messages while the agent runs
/// 5. Spawn agent, drain AnswerStream into SlackRenderer (text accumulates)
/// 6. `chat.postMessage` with the full Block Kit body — sections + chart
///    image blocks + divider + footer card
/// 7. Persist agent output + artifacts
/// 8. update_last_ts on slack_threads
pub async fn run_for_slack(req: SlackRunRequest) -> Result<(), SlackError> {
    tracing::info!(
        workspace_id = %req.workspace_id,
        agent = %req.agent_path,
        channel = %req.channel_id,
        "run_for_slack: starting execution"
    );

    let bot_token = req.bot_token.clone();
    let client = SlackClient::new();

    // Create/find the Slack + Oxy thread rows first. After this point we have
    // enough context to build a `thread_url` that can be embedded in any
    // subsequent `SlackError::Internal` so the user can navigate to whatever
    // was partially computed.
    let (slack_thread_row, oxy_thread_id, is_new_thread) = find_or_create_slack_thread(&req)
        .await
        .map_err(|e| SlackError::Internal {
            source: e,
            thread_url: None,
        })?;

    // On the first message of a new thread, set the human-readable title
    // shown in the bot's split-view History tab to a short version of the
    // user's question. This is what differentiates entries in the user's
    // chat history from one another. Slack caps thread titles at ~120 chars.
    if is_new_thread {
        let title = truncate(&req.question, 100);
        let _ = client
            .assistant_threads_set_title(&bot_token, &req.channel_id, &req.thread_ts, &title)
            .await;
    }

    // Compute thread_url immediately after thread creation. From this point on,
    // any SlackError::Internal includes this URL so the user can navigate to
    // whatever was partially computed even if the agent never starts.
    let thread_url = match SlackConfig::cached().as_runtime() {
        Some(c) => Some(
            build_thread_url(
                &c.app_base_url,
                req.installation.org_id,
                req.workspace_id,
                oxy_thread_id,
            )
            .await,
        ),
        None => None,
    };

    // Helper closure: wrap an OxyError into SlackError::Internal with the
    // thread_url we already computed, so every `?` below gets the link.
    let internal = |e: OxyError| SlackError::Internal {
        source: e,
        thread_url: thread_url.clone(),
    };

    // Persist user message.
    conversation::persist_user_message(oxy_thread_id, &req.question)
        .await
        .map_err(&internal)?;

    // Load conversation history.
    let memory = conversation::load_memory(oxy_thread_id)
        .await
        .map_err(&internal)?;

    // Slack's AI-assistant indicator. Per docs:
    //   <https://docs.slack.dev/reference/methods/assistant.threads.setStatus>
    //
    //   "An empty string in the status field will clear the status indicator."
    //
    // So `status` MUST be non-empty — passing "" silently clears the indicator
    // and Slack falls back to its default "Thinking…" affordance, which was
    // the bug we kept failing to diagnose. Slack renders this as
    // "<App Name> <status>" (e.g. "OxyDev is working on your request…").
    //
    // `loading_messages` rotates over this static base. The list is kept
    // short (3 entries) so it reads as ambient motion rather than a
    // shuffly carousel — the plan-mode task block carries the real
    // "where the agent is right now" signal.
    //
    // There is a hard 2-minute auto-clear timeout on the indicator: any
    // agent run >2 min loses the indicator partway through. Refreshing
    // periodically before that timeout would require a background task;
    // not added yet — most runs are well under 2 min.
    //
    // No `chat.startStream` here: it's an unrelated streaming-text API
    // and `chat.appendStream` activity actively clears the status. We
    // post the final answer via `chat.postMessage` at the end, which
    // also naturally clears the status (Slack auto-clears on app reply).
    if let Err(e) = client
        .assistant_threads_set_status(
            &bot_token,
            &req.channel_id,
            &req.thread_ts,
            "is working on your request…",
            Some(crate::integrations::slack::render::LOADING_MESSAGES),
        )
        .await
    {
        tracing::warn!(
            channel = %req.channel_id,
            thread_ts = %req.thread_ts,
            "assistant.threads.setStatus failed: {e}"
        );
    }

    // Build the WorkspaceManager once here so we can extract the chart
    // image publisher before spawning the agent task (which consumes the
    // manager). Doing it here also means the S3 client is constructed once
    // rather than once per request path.
    let repo_path = resolve_workspace_path(req.workspace_id)
        .await
        .map_err(&internal)?;
    let workspace_manager = WorkspaceBuilder::new(req.workspace_id)
        .with_workspace_path_and_fallback_config(&repo_path)
        .await
        .map_err(&internal)?
        .try_with_intent_classifier()
        .await
        .build()
        .await
        .map_err(&internal)?;

    // Extract chart publishing resources before moving workspace_manager
    // into the agent task. `get_charts_dir` is cheap — it just joins the
    // state path — but it is async so we call it here.
    let chart_image_publisher = workspace_manager.chart_image_publisher();
    let charts_dir = workspace_manager.config_manager.get_charts_dir().await.ok();

    // Set up the block handler with a live channel for streaming.
    let (tx, rx) = mpsc::channel::<AnswerStream>(256);
    let block_handler = BlockHandler::new(tx);
    let block_handler_reader = block_handler.get_reader();

    // Run the agent concurrently.
    let agent_path = req.agent_path.clone();
    let question = req.question.clone();
    let channel_id = req.channel_id.clone();

    let agent_handle = tokio::spawn(async move {
        execute_agent_inner(
            oxy_thread_id,
            workspace_manager,
            &question,
            &agent_path,
            &channel_id,
            memory,
            block_handler,
        )
        .await
    });

    // Drain the AnswerStream into `SlackRenderer`. Body is accumulated
    // locally and posted as a single `chat.postMessage` at the end.
    let renderer = SlackRenderer::new(
        &client,
        &bot_token,
        &req.channel_id,
        &req.thread_ts,
        thread_url.clone(),
        req.workspace_id,
        chart_image_publisher,
        charts_dir,
    );
    let (body_markdown, chart_image_urls, chart_local_paths, attempted_chart_count) =
        oxy::render_stream(rx, renderer).await;

    // Await agent completion.
    let agent_result = agent_handle
        .await
        .map_err(|e| internal(OxyError::RuntimeError(format!("agent task panicked: {e}"))))?;

    let (final_markdown, agent_errored) = match agent_result {
        Ok(markdown) => (markdown, false),
        Err(e) => {
            let msg = format!("Agent run failed: {e}");
            let _ = conversation::persist_plain_agent_message(oxy_thread_id, &msg).await;
            let _ = conversation::update_thread_with_output(oxy_thread_id, &msg, false).await;
            (msg, true)
        }
    };

    // Persist agent output + artifacts.
    if let Err(e) =
        conversation::persist_agent_output_from_blocks(oxy_thread_id, block_handler_reader).await
    {
        tracing::warn!("Failed to persist agent output: {}", e);
    }
    if let Err(e) =
        conversation::update_thread_with_output(oxy_thread_id, &final_markdown, false).await
    {
        tracing::warn!("Failed to update thread output: {}", e);
    }

    // Build the body blocks (sections + chart links) from accumulated
    // markdown. On error, swap the body for an alert block carrying the
    // failure message.
    let body_blocks = if agent_errored {
        blocks::build_error_alert_blocks(&final_markdown)
    } else {
        let prose = if body_markdown.trim().is_empty() {
            final_markdown.clone()
        } else {
            body_markdown.clone()
        };
        blocks::build_body_blocks(&prose)
    };

    // Claude-style footer:
    //   1. body section blocks
    //   2. inline chart image blocks (one per published chart URL)
    //   3. context block linking to the Oxy thread when charts were
    //      attempted but no public image URL is available
    //   4. divider
    //   5. actions block with one "View thread" button (omitted when no URL)
    //   6. context block: "Requested by @user · Oxy can make mistakes…"
    //
    // Image blocks are inserted between the prose sections and the footer
    // so they appear inline with the answer rather than below attribution.
    // They require a publicly fetchable URL; when none is available
    // (no `ChartImageRenderer` wired up, local-disk backend, etc.) we
    // emit a single "View chart in Oxy" link instead — the Oxy thread
    // page renders charts client-side via echarts, so the user can still
    // see them, just not inline in Slack.
    let mut all_blocks: Vec<serde_json::Value> = body_blocks;

    // Inline chart image blocks — emitted only when the publisher returned
    // a public URL (S3 backend). Slack's `image` block: alt_text ≤ 2000
    // chars, image_url ≤ 3000 chars, max 50 blocks per message.
    if !agent_errored {
        for url in &chart_image_urls {
            all_blocks.push(serde_json::json!({
                "type": "image",
                "image_url": url,
                "alt_text": "Chart",
            }));
        }
    }

    // Local-render breadcrumbs: surface the on-disk PNG paths so a
    // developer running locally can `open` the file and validate the
    // chart visually. Slack itself can't fetch a localhost path, so
    // this is a debug affordance, not a real preview. The path appears
    // as a code-formatted snippet inside a context block — keeps it
    // visually muted but copyable.
    if !agent_errored {
        for path in &chart_local_paths {
            all_blocks.push(serde_json::json!({
                "type": "context",
                "elements": [{
                    "type": "mrkdwn",
                    "text": format!(
                        "📊 Chart rendered locally — `{}` (open this file to validate; Slack can't fetch localhost paths so no inline preview)",
                        path.display()
                    ),
                }],
            }));
        }
    }

    // Charts-fallback context: when at least one chart was emitted but
    // produced no inline image (the typical state today), point users at
    // the Oxy thread where the chart will render client-side.
    let unrendered_charts = attempted_chart_count.saturating_sub(chart_image_urls.len());
    if !agent_errored
        && unrendered_charts > 0
        && let Some(url) = thread_url.as_deref()
    {
        let label = if unrendered_charts == 1 {
            "📊 View chart in Oxygen →".to_string()
        } else {
            format!("📊 View {unrendered_charts} charts in Oxygen →")
        };
        all_blocks.push(serde_json::json!({
            "type": "context",
            "elements": [{
                "type": "mrkdwn",
                "text": format!("<{url}|{label}>"),
            }],
        }));
    }

    if !all_blocks.is_empty() {
        all_blocks.push(serde_json::json!({ "type": "divider" }));
    }
    if !agent_errored && let Some(url) = thread_url.as_deref() {
        // Show "Wrong workspace?" alongside "View thread" only when the user
        // actually has another workspace to switch to. The COUNT is cheap;
        // failing it just suppresses the button (footer still renders).
        let reopen_q =
            match crate::integrations::slack::resolution::workspace_agent::count_org_workspaces(
                req.installation.org_id,
            )
            .await
            {
                Ok(n) if n > 1 => {
                    Some(base64::engine::general_purpose::STANDARD.encode(req.question.as_bytes()))
                }
                Ok(_) => None,
                Err(e) => {
                    tracing::warn!(
                        org_id = %req.installation.org_id,
                        "count_org_workspaces failed, hiding reopen-picker button: {e}"
                    );
                    None
                }
            };
        all_blocks.push(blocks::build_footer_actions(url, reopen_q.as_deref()));
    }
    if !agent_errored {
        all_blocks.push(blocks::build_attribution_context(
            &req.user_link.slack_user_id,
            &blocks::agent_display_name(&req.agent_path),
        ));
    }
    let stop_blocks = serde_json::Value::Array(all_blocks);

    // Final answer lands as a single chat.postMessage. Slack auto-clears
    // the assistant.threads.setStatus indicator on app reply.
    let fallback_text = blocks::pick_fallback_text(agent_errored, &final_markdown);
    if let Err(e) = client
        .chat_post_message_with_blocks(
            &bot_token,
            &req.channel_id,
            &fallback_text,
            Some(&req.thread_ts),
            Some(stop_blocks),
        )
        .await
    {
        tracing::warn!("chat.postMessage failed: {e}");
    }

    ThreadContextService::update_last_ts(slack_thread_row.id, &req.thread_ts)
        .await
        .map_err(&internal)?;

    Ok(())
}

// ============================================================================
// Workspace + URL helpers
// ============================================================================

/// Fetch the org slug for an installation. Used to build cloud-mode URLs
/// (`/{slug}/workspaces/{ws_id}/threads/{tid}`) — without the slug, the
/// thread route 404s in cloud mode because the SPA dispatches by org.
async fn fetch_org_slug(org_id: Uuid) -> Option<String> {
    let conn = establish_connection().await.ok()?;
    entity::prelude::Organizations::find_by_id(org_id)
        .one(&conn)
        .await
        .ok()
        .flatten()
        .map(|o| o.slug)
}

/// Build the public Oxy URL for an Oxy thread. Cloud-mode shape is
/// `{base}/{org_slug}/workspaces/{workspace_id}/threads/{thread_id}`; we
/// fall back to the older `{base}/threads/{thread_id}` when the org slug
/// can't be resolved (e.g. local-mode deployments, transient DB error)
/// rather than building a link that's guaranteed to 404 in cloud mode.
async fn build_thread_url(
    base_url: &str,
    org_id: Uuid,
    workspace_id: Uuid,
    oxy_thread_id: Uuid,
) -> String {
    match fetch_org_slug(org_id).await {
        Some(slug) => {
            format!("{base_url}/{slug}/workspaces/{workspace_id}/threads/{oxy_thread_id}")
        }
        None => format!("{base_url}/threads/{oxy_thread_id}"),
    }
}

// ============================================================================
// Thread management
// ============================================================================

/// Returns (slack_thread_row, oxy_thread_id, is_new).
/// Creates both rows if the slack thread doesn't exist yet. The `is_new`
/// flag lets the caller distinguish first-message threads (where we set
/// the History-tab title from the user's question) from follow-ups
/// (where the title is already set and shouldn't be overwritten).
///
/// When an existing row is found but the workspace differs (e.g. the user
/// switched via "Wrong workspace?"), the row is updated in-place so that
/// subsequent follow-ups continue against the newly chosen workspace.
async fn find_or_create_slack_thread(
    req: &SlackRunRequest,
) -> Result<(entity::slack_threads::Model, Uuid, bool), OxyError> {
    if let Some(mut row) =
        ThreadContextService::find(req.installation.id, &req.channel_id, &req.thread_ts).await?
    {
        if row.workspace_id != req.workspace_id || row.agent_path != req.agent_path {
            ThreadContextService::update_workspace(row.id, req.workspace_id, &req.agent_path)
                .await?;
            row.workspace_id = req.workspace_id;
            row.agent_path = req.agent_path.clone();
        }
        let oxy_thread_id = row.oxy_thread_id;
        return Ok((row, oxy_thread_id, false));
    }

    // Create Oxy thread first. Slack-specific title prefix (`"Slack: …"`)
    // tags the entry in the web UI's thread list so it's distinguishable
    // from web-originated threads at a glance.
    let title = format!("Slack: {}", truncate(&req.question, 50));
    let oxy_thread_id = conversation::create_thread(
        req.workspace_id,
        req.user_link.oxy_user_id,
        &title,
        &req.question,
        &req.agent_path,
    )
    .await?;

    // Bind to Slack context.
    let row = ThreadContextService::create(CreateThreadContext {
        installation_id: req.installation.id,
        slack_channel_id: req.channel_id.clone(),
        slack_thread_ts: req.thread_ts.clone(),
        workspace_id: req.workspace_id,
        agent_path: req.agent_path.clone(),
        oxy_thread_id,
        initiated_by_user_link_id: Some(req.user_link.id),
    })
    .await?;

    Ok((row, oxy_thread_id, true))
}

// ============================================================================
// Agent execution
// ============================================================================

async fn execute_agent_inner(
    thread_id: Uuid,
    workspace_manager: WorkspaceManager,
    question: &str,
    agent_path: &str,
    channel_id: &str,
    memory: Vec<Message>,
    block_handler: BlockHandler,
) -> Result<String, OxyError> {
    let result = run_agent(
        workspace_manager,
        std::path::Path::new(agent_path),
        question.to_string(),
        block_handler,
        memory,
        None,
        None,
        None,
        Some(ExecutionSource::Slack {
            thread_id: thread_id.to_string(),
            channel_id: Some(channel_id.to_string()),
        }),
        None,
        None,
    )
    .await?;

    let markdown = result.to_markdown();
    if markdown.trim().is_empty() {
        Ok("✅ Task completed".to_string())
    } else {
        Ok(markdown)
    }
}

// ============================================================================
// Utilities
// ============================================================================

/// Truncate `s` to at most `max_len` **characters** (not bytes), appending `...`
/// if truncation occurred. Safe for UTF-8 text (CJK, emoji).
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let boundary = s
        .char_indices()
        .nth(max_len)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    format!("{}...", &s[..boundary])
}

#[cfg(test)]
mod truncate_tests {
    use super::truncate;

    #[test]
    fn passes_through_short_ascii() {
        assert_eq!(truncate("hi", 50), "hi");
    }

    #[test]
    fn truncates_long_ascii() {
        assert_eq!(truncate("abcdefghij", 5), "abcde...");
    }

    #[test]
    fn truncates_multibyte_utf8_without_panic() {
        // Each emoji is 4 bytes; naïve byte slicing at max_len=3 would split one.
        let input = "🙂🙂🙂🙂🙂";
        let out = truncate(input, 2);
        assert_eq!(out, "🙂🙂...");
    }

    #[test]
    fn truncates_cjk_at_character_boundary() {
        // "你好世界" is 4 chars, 12 bytes. max_len=2 chars → first 2 chars.
        assert_eq!(truncate("你好世界", 2), "你好...");
    }

    #[test]
    fn max_len_equal_to_char_count_returns_original() {
        assert_eq!(truncate("你好", 2), "你好");
    }
}
