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
//!   3. `chat.postMessage` delivers the final answer as a single Block Kit payload:
//!      section blocks for prose, image blocks for chart URLs, a divider, and a
//!      footer card with a "View thread" deep-link. Slack auto-clears the
//!      setStatus indicator when the bot posts a reply.

use std::time::Duration;

use base64::Engine;

use crate::integrations::slack::blocks;
use crate::integrations::slack::client::SlackClient;
use crate::integrations::slack::config::SlackConfig;
use crate::integrations::slack::error::SlackError;
use crate::integrations::slack::render::{CapturedSqlArtifact, QueuedChart, SlackRenderer};
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

/// Per-file upload budget. Used by both chart PNG uploads and SQL
/// artifact uploads — the same `files.uploadV2` shape, the same edge
/// node behavior. Slack typically responds in well under a second;
/// a stuck multipart POST shouldn't block the rest of the queue or
/// the post-message bookkeeping.
pub(crate) const FILE_UPLOAD_TIMEOUT: Duration = Duration::from_secs(15);

/// Cap on uploaded SQL `.sql` files per Slack message. Beyond this the
/// inline `> 📎 _title_ ✓` placeholder in the prose marks each artifact
/// regardless, and a follow-up "📎 N more queries — view in Oxygen →"
/// context block is posted in the same thread (when a thread URL exists)
/// so the overflow is visible to the user, not just to log readers.
const MAX_SQL_ARTIFACTS_PER_MESSAGE: usize = 10;

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

    set_loading_status(&client, &bot_token, &req.channel_id, &req.thread_ts).await;

    let workspace_manager = build_workspace_manager(req.workspace_id, &internal).await?;
    let upload_charts = crate::integrations::slack::config::chart_upload_enabled();

    let exec = run_agent_for_slack(
        oxy_thread_id,
        req.workspace_id,
        workspace_manager,
        &req.agent_path,
        &req.question,
        &req.channel_id,
        &req.thread_ts,
        thread_url.clone(),
        memory,
        upload_charts,
        &client,
        &bot_token,
    )
    .await
    .map_err(&internal)?;

    let (all_blocks, view_sql, sql_overflow) = build_message_blocks(
        &exec,
        thread_url.as_deref(),
        &req.question,
        req.installation.org_id,
        &req.user_link.slack_user_id,
        &req.agent_path,
    )
    .await;

    let fallback_text = blocks::pick_fallback_text(exec.agent_errored, &exec.final_markdown);
    if let Err(e) = client
        .chat_post_message_with_blocks(
            &bot_token,
            &req.channel_id,
            &fallback_text,
            Some(&req.thread_ts),
            Some(serde_json::Value::Array(all_blocks)),
        )
        .await
    {
        tracing::warn!("chat.postMessage failed: {e}");
    }

    let upload_failures = upload_charts_sequentially(
        &client,
        &bot_token,
        &req.channel_id,
        &req.thread_ts,
        exec.queued_charts,
        &req.question,
        exec.agent_errored,
    )
    .await;

    post_overflow_followups(
        &client,
        &bot_token,
        &req.channel_id,
        &req.thread_ts,
        upload_failures,
        sql_overflow,
        view_sql,
        thread_url.as_deref(),
        exec.agent_errored,
    )
    .await;

    ThreadContextService::update_last_ts(slack_thread_row.id, &req.thread_ts)
        .await
        .map_err(&internal)?;

    Ok(())
}

// ============================================================================
// Focused helpers
// ============================================================================

/// Output produced by [`run_agent_for_slack`].
struct AgentExecOutput {
    /// Accumulated prose markdown from the renderer.
    body_markdown: String,
    /// Chart PNG blobs ready to upload.
    queued_charts: Vec<QueuedChart>,
    /// On-disk PNG paths for local-dev inspection.
    chart_local_paths: Vec<std::path::PathBuf>,
    /// Number of charts that failed to render (Chromium crash / empty bytes).
    failed_chart_count: usize,
    /// SQL/semantic-query artifacts captured during the run.
    captured_sql_artifacts: Vec<CapturedSqlArtifact>,
    /// Authoritative agent answer (may be an error message when `agent_errored`).
    final_markdown: String,
    /// True when the agent task returned an Err — prose becomes an error alert.
    agent_errored: bool,
}

/// Build the workspace, spawn the agent, drain the stream, await completion,
/// and persist the output. Returns a summary of everything the caller needs to
/// assemble the Slack message.
async fn run_agent_for_slack(
    oxy_thread_id: Uuid,
    workspace_id: Uuid,
    workspace_manager: WorkspaceManager,
    agent_path: &str,
    question: &str,
    channel_id: &str,
    thread_ts: &str,
    thread_url: Option<String>,
    memory: Vec<Message>,
    upload_charts: bool,
    client: &SlackClient,
    bot_token: &str,
) -> Result<AgentExecOutput, OxyError> {
    let (tx, rx) = mpsc::channel::<AnswerStream>(256);
    let block_handler = BlockHandler::new(tx);
    let block_handler_reader = block_handler.get_reader();

    let agent_path_owned = agent_path.to_owned();
    let question_owned = question.to_owned();
    let channel_id_owned = channel_id.to_owned();

    let agent_handle = tokio::spawn(async move {
        execute_agent_inner(
            oxy_thread_id,
            workspace_manager,
            &question_owned,
            &agent_path_owned,
            &channel_id_owned,
            memory,
            block_handler,
        )
        .await
    });

    let renderer = SlackRenderer::new(
        client,
        bot_token,
        channel_id,
        thread_ts,
        thread_url,
        workspace_id,
        upload_charts,
    );
    let render_result = oxy::render_stream(rx, renderer).await;
    tracing::info!(
        captured_count = render_result.captured_sql_artifacts.len(),
        body_len = render_result.body.len(),
        queued_charts = render_result.queued_charts.len(),
        "slack run_agent_for_slack: render_stream finished"
    );

    let agent_result = agent_handle
        .await
        .map_err(|e| OxyError::RuntimeError(format!("agent task panicked: {e}")))?;

    let (final_markdown, agent_errored) = match agent_result {
        Ok(markdown) => (markdown, false),
        Err(e) => {
            let msg = format!("Agent run failed: {e}");
            let _ = conversation::persist_plain_agent_message(oxy_thread_id, &msg).await;
            let _ = conversation::update_thread_with_output(oxy_thread_id, &msg, false).await;
            (msg, true)
        }
    };

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

    Ok(AgentExecOutput {
        body_markdown: render_result.body,
        queued_charts: render_result.queued_charts,
        chart_local_paths: render_result.chart_local_paths,
        failed_chart_count: render_result.failed_chart_count,
        captured_sql_artifacts: render_result.captured_sql_artifacts,
        final_markdown,
        agent_errored,
    })
}

/// Assemble all Block Kit blocks for the `chat.postMessage` payload.
///
/// Returns `(blocks, view_sql, sql_overflow)`:
/// - `blocks` — the full ordered block list ready to post
/// - `view_sql` — optional `(upload_id, count)` stashed for the SQL button
/// - `sql_overflow` — number of SQL artifacts beyond the cap (used for
///   the follow-up "N more queries" post)
async fn build_message_blocks(
    exec: &AgentExecOutput,
    thread_url: Option<&str>,
    question: &str,
    org_id: Uuid,
    slack_user_id: &str,
    agent_path: &str,
) -> (Vec<serde_json::Value>, Option<(Uuid, usize)>, usize) {
    // Body: error alert or prose sections.
    let mut all_blocks: Vec<serde_json::Value> = if exec.agent_errored {
        blocks::build_error_alert_blocks(&exec.final_markdown)
    } else {
        let prose = if exec.body_markdown.trim().is_empty() {
            exec.final_markdown.clone()
        } else {
            exec.body_markdown.clone()
        };
        blocks::build_body_blocks(&prose)
    };

    // Local-render breadcrumbs: on-disk PNG paths so a developer running
    // locally can `open` the file and validate the chart visually. Slack
    // itself can't fetch a localhost path — this is a debug affordance only.
    if !exec.agent_errored {
        for path in &exec.chart_local_paths {
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

    // Render-failure footer: charts that never produced bytes are surfaced
    // via a "view in Oxygen" link. Upload failures are handled separately
    // in post_overflow_followups once we know the count.
    if !exec.agent_errored
        && exec.failed_chart_count > 0
        && let Some(url) = thread_url
    {
        let label = if exec.failed_chart_count == 1 {
            "⚠️ Chart render failed — view in Oxygen →".to_string()
        } else {
            format!("⚠️ {} chart renders failed — view in Oxygen →", exec.failed_chart_count)
        };
        all_blocks.push(serde_json::json!({
            "type": "context",
            "elements": [{"type": "mrkdwn", "text": format!("<{url}|{label}>")}],
        }));
    }

    // SQL artifacts: stash behind a deferred-upload button.
    let captured_total = exec.captured_sql_artifacts.len();
    let sql_to_upload = captured_total.min(MAX_SQL_ARTIFACTS_PER_MESSAGE);
    let sql_overflow = captured_total.saturating_sub(MAX_SQL_ARTIFACTS_PER_MESSAGE);
    if sql_overflow > 0 {
        tracing::warn!(
            captured = captured_total,
            uploaded = sql_to_upload,
            dropped = sql_overflow,
            "sql artifact cap reached; some queries will not be uploaded even if the user clicks the button"
        );
    }
    let view_sql: Option<(Uuid, usize)> = if !exec.agent_errored && sql_to_upload > 0 {
        let to_stash: Vec<_> = exec
            .captured_sql_artifacts
            .iter()
            .cloned()
            .take(sql_to_upload)
            .collect();
        let upload_id =
            crate::integrations::slack::services::pending_sql_uploads::insert(to_stash).await;
        Some((upload_id, sql_to_upload))
    } else {
        None
    };

    if !all_blocks.is_empty() {
        all_blocks.push(serde_json::json!({ "type": "divider" }));
    }

    // Footer: "View thread" + optional "Wrong workspace?" + optional SQL button.
    if !exec.agent_errored && let Some(url) = thread_url {
        let reopen_q = resolve_reopen_query(org_id, question).await;
        all_blocks.push(blocks::build_footer_actions(url, reopen_q.as_deref(), view_sql));
    } else if let Some((upload_id, count)) = view_sql {
        // No thread URL but SQL artifacts exist — render SQL button standalone.
        all_blocks.push(blocks::build_view_sql_only_actions(upload_id, count));
    }

    if !exec.agent_errored {
        all_blocks.push(blocks::build_attribution_context(
            slack_user_id,
            &blocks::agent_display_name(agent_path),
        ));
    }

    (all_blocks, view_sql, sql_overflow)
}

/// Upload chart PNGs sequentially into the Slack thread, returning the number
/// of failures. Sequential order preserves the agent's chart emission order
/// (parallel completion would scramble it). Each upload is bounded by
/// [`FILE_UPLOAD_TIMEOUT`].
async fn upload_charts_sequentially(
    client: &SlackClient,
    bot_token: &str,
    channel_id: &str,
    thread_ts: &str,
    queued_charts: Vec<QueuedChart>,
    question: &str,
    agent_errored: bool,
) -> usize {
    if agent_errored || queued_charts.is_empty() {
        return 0;
    }

    let chart_label_base = chart_label_from_question(question);
    let chart_filename_stem = chart_filename_stem_from_question(question);
    let total = queued_charts.len();
    let mut failures: usize = 0;

    for (idx, chart) in queued_charts.into_iter().enumerate() {
        let title = if total > 1 {
            format!("{chart_label_base} ({} of {total})", idx + 1)
        } else {
            chart_label_base.clone()
        };
        let filename = if total > 1 {
            format!("{}-{}.png", chart_filename_stem, idx + 1)
        } else {
            format!("{chart_filename_stem}.png")
        };
        let chart_src = chart.chart_src;
        let upload = client.files_upload_v2(
            bot_token,
            channel_id,
            Some(thread_ts),
            &filename,
            chart.png_bytes,
            Some(&title),
            "image/png",
        );
        match tokio::time::timeout(FILE_UPLOAD_TIMEOUT, upload).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::warn!(chart_src = %chart_src, "files.uploadV2 failed: {e}");
                failures += 1;
            }
            Err(_) => {
                tracing::warn!(
                    chart_src = %chart_src,
                    timeout_secs = FILE_UPLOAD_TIMEOUT.as_secs(),
                    "files.uploadV2 timed out"
                );
                failures += 1;
            }
        }
    }

    failures
}

/// Post any needed follow-up context messages into the thread:
/// - chart-upload failures ("⚠️ N chart uploads failed — view in Oxygen →")
/// - SQL artifact cap overflow ("📎 N more queries — view in Oxygen →")
///
/// All errors are logged and swallowed; follow-up failures should never
/// propagate as errors to the caller.
async fn post_overflow_followups(
    client: &SlackClient,
    bot_token: &str,
    channel_id: &str,
    thread_ts: &str,
    upload_failures: usize,
    sql_overflow: usize,
    view_sql: Option<(Uuid, usize)>,
    thread_url: Option<&str>,
    agent_errored: bool,
) {
    if agent_errored {
        return;
    }

    // Chart-upload failure follow-up.
    if upload_failures > 0 && let Some(url) = thread_url {
        let label = if upload_failures == 1 {
            "⚠️ Chart upload failed — view in Oxygen →".to_string()
        } else {
            format!("⚠️ {upload_failures} chart uploads failed — view in Oxygen →")
        };
        let blocks = serde_json::json!([{
            "type": "context",
            "elements": [{"type": "mrkdwn", "text": format!("<{url}|{label}>")}],
        }]);
        if let Err(e) = client
            .chat_post_message_with_blocks(
                bot_token,
                channel_id,
                "Some charts couldn't be uploaded",
                Some(thread_ts),
                Some(blocks),
            )
            .await
        {
            tracing::warn!("upload-failure follow-up post failed: {e}");
        }
    }

    // SQL cap-overflow follow-up: inline placeholders in prose already show all
    // artifacts; this post closes the gap for the ones we won't upload.
    if sql_overflow > 0 && view_sql.is_some() && let Some(url) = thread_url {
        let plural = if sql_overflow == 1 { "query" } else { "queries" };
        let label = format!("📎 {sql_overflow} more {plural} — view in Oxygen →");
        let blocks = serde_json::json!([{
            "type": "context",
            "elements": [{"type": "mrkdwn", "text": format!("<{url}|{label}>")}],
        }]);
        if let Err(e) = client
            .chat_post_message_with_blocks(
                bot_token,
                channel_id,
                "More SQL queries are available in Oxygen",
                Some(thread_ts),
                Some(blocks),
            )
            .await
        {
            tracing::warn!("sql cap-overflow follow-up post failed: {e}");
        }
    }
}

// ============================================================================
// Setup helpers (keep run_for_slack thin)
// ============================================================================

/// Send the rotating "is working on your request…" indicator.
///
/// Slack's AI-assistant indicator rules:
/// - Status MUST be non-empty; `""` silently clears the indicator.
/// - There is a hard 2-minute auto-clear timeout — any run >2 min loses
///   the indicator partway through.
/// - `chat.appendStream` actively clears the status; we use `postMessage`
///   at the end, which also auto-clears (Slack clears on app reply).
async fn set_loading_status(
    client: &SlackClient,
    bot_token: &str,
    channel_id: &str,
    thread_ts: &str,
) {
    if let Err(e) = client
        .assistant_threads_set_status(
            bot_token,
            channel_id,
            thread_ts,
            "is working on your request…",
            Some(crate::integrations::slack::render::LOADING_MESSAGES),
        )
        .await
    {
        tracing::warn!(
            channel = %channel_id,
            thread_ts = %thread_ts,
            "assistant.threads.setStatus failed: {e}"
        );
    }
}

/// Build the `WorkspaceManager` for the given workspace.
async fn build_workspace_manager<F>(
    workspace_id: Uuid,
    internal: &F,
) -> Result<WorkspaceManager, SlackError>
where
    F: Fn(OxyError) -> SlackError,
{
    let repo_path = resolve_workspace_path(workspace_id)
        .await
        .map_err(internal)?;
    WorkspaceBuilder::new(workspace_id)
        .with_workspace_path_and_fallback_config(&repo_path)
        .await
        .map_err(internal)?
        .try_with_intent_classifier()
        .await
        .build()
        .await
        .map_err(internal)
}

/// Return the base64-encoded question when the user's org has >1 workspace
/// (used for the "Wrong workspace?" button). Returns `None` on single-workspace
/// orgs or on query failure.
async fn resolve_reopen_query(org_id: Uuid, question: &str) -> Option<String> {
    match crate::integrations::slack::resolution::workspace_agent::count_org_workspaces(org_id)
        .await
    {
        Ok(n) if n > 1 => {
            Some(base64::engine::general_purpose::STANDARD.encode(question.as_bytes()))
        }
        Ok(_) => None,
        Err(e) => {
            tracing::warn!(
                org_id = %org_id,
                "count_org_workspaces failed, hiding reopen-picker button: {e}"
            );
            None
        }
    }
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
///
/// The returned URL is later embedded into Slack mrkdwn link syntax
/// `<url|label>`, which has no escape mechanism — a `|` inside the
/// URL would break label parsing and the rendered link silently drops
/// to plain text. Org slugs are validated to alphanumeric + dash on
/// creation, so a `|` here would be a constraint-violation upstream.
/// We strip any defensively (silently — a degraded link is better than
/// a panicking webhook handler) and `debug_assert!` so a regression
/// in slug validation surfaces in dev/test rather than production.
async fn build_thread_url(
    base_url: &str,
    org_id: Uuid,
    workspace_id: Uuid,
    oxy_thread_id: Uuid,
) -> String {
    let url = match fetch_org_slug(org_id).await {
        Some(slug) => {
            format!("{base_url}/{slug}/workspaces/{workspace_id}/threads/{oxy_thread_id}")
        }
        None => format!("{base_url}/threads/{oxy_thread_id}"),
    };
    debug_assert!(
        !url.contains('|'),
        "thread URL must not contain '|' — Slack mrkdwn `<url|label>` has no escape: {url}"
    );
    url.replace('|', "")
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

/// Build the human-readable Slack file title for a chart from the user's
/// question. Caps at 80 chars (well under Slack's 250-char title limit
/// but still scannable in the file card). Falls back to "Chart" when the
/// question is empty / whitespace-only — every file must have a title.
fn chart_label_from_question(question: &str) -> String {
    let trimmed = question.trim();
    if trimmed.is_empty() {
        return "Chart".to_string();
    }
    truncate(trimmed, 80)
}

/// Build a filesystem-safe stem (no extension) for the chart download
/// filename, derived from the user's question. Lowercases, replaces
/// runs of non-alphanumerics with `-`, trims leading/trailing dashes,
/// and caps at 60 chars. Returns `"chart"` for empty/whitespace-only
/// questions or questions that contain no alphanumerics.
fn chart_filename_stem_from_question(question: &str) -> String {
    let mut out = String::with_capacity(question.len());
    let mut last_was_dash = false;
    for ch in question.chars() {
        if ch.is_ascii_alphanumeric() {
            for c in ch.to_lowercase() {
                out.push(c);
            }
            last_was_dash = false;
        } else if !last_was_dash && !out.is_empty() {
            out.push('-');
            last_was_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        return "chart".to_string();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 60 {
        return trimmed.to_string();
    }
    chars[..60]
        .iter()
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

/// Filename-safe slug for artifact titles. Preserves `_` (semantic-query
/// tool names use it heavily — `query_retail_analytics`) and collapses
/// any other non-alphanumeric run to a single `_`. Falls back to `query`
/// for an empty input.
pub(crate) fn sanitize_filename(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_underscore = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
            last_was_underscore = ch == '_';
        } else if !last_was_underscore && !out.is_empty() {
            out.push('_');
            last_was_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        return "query".to_string();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 60 {
        return trimmed.to_string();
    }
    chars[..60]
        .iter()
        .collect::<String>()
        .trim_end_matches('_')
        .to_string()
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

#[cfg(test)]
mod chart_label_tests {
    use super::{chart_filename_stem_from_question, chart_label_from_question};

    #[test]
    fn label_uses_question_text_when_short() {
        assert_eq!(
            chart_label_from_question("Which store has the highest sales?"),
            "Which store has the highest sales?"
        );
    }

    #[test]
    fn label_truncates_long_questions() {
        let long = "a".repeat(200);
        let label = chart_label_from_question(&long);
        assert!(label.starts_with("aaa"));
        assert!(label.ends_with("..."));
        assert!(label.chars().count() <= 83); // 80 chars + "..."
    }

    #[test]
    fn label_falls_back_to_chart_for_empty_question() {
        assert_eq!(chart_label_from_question(""), "Chart");
        assert_eq!(chart_label_from_question("   "), "Chart");
    }

    #[test]
    fn filename_stem_slugifies_question() {
        assert_eq!(
            chart_filename_stem_from_question("Which store has the highest sales?"),
            "which-store-has-the-highest-sales"
        );
    }

    #[test]
    fn filename_stem_collapses_runs_of_non_alphanumeric() {
        assert_eq!(
            chart_filename_stem_from_question("Hello,   world!!! 👋 foo"),
            "hello-world-foo"
        );
    }

    #[test]
    fn filename_stem_caps_length_and_trims_trailing_dash() {
        let q = "a".repeat(70) + " " + &"b".repeat(10);
        let stem = chart_filename_stem_from_question(&q);
        assert!(stem.chars().count() <= 60);
        assert!(!stem.ends_with('-'));
    }

    #[test]
    fn filename_stem_falls_back_for_alphanumeric_free_input() {
        assert_eq!(chart_filename_stem_from_question(""), "chart");
        assert_eq!(chart_filename_stem_from_question("   "), "chart");
        assert_eq!(chart_filename_stem_from_question("???"), "chart");
        // Non-ASCII alphanumerics are stripped (we only keep ASCII alnum).
        assert_eq!(chart_filename_stem_from_question("你好"), "chart");
    }
}

#[cfg(test)]
mod sanitize_filename_tests {
    use super::sanitize_filename;

    #[test]
    fn preserves_underscores_in_semantic_query_names() {
        // Semantic-query tool names use `_` heavily (e.g. `query_retail_analytics`,
        // `query_store_performance`); the sanitizer must keep them verbatim.
        assert_eq!(
            sanitize_filename("query_retail_analytics"),
            "query_retail_analytics"
        );
        assert_eq!(
            sanitize_filename("query_store_performance"),
            "query_store_performance"
        );
    }

    #[test]
    fn slugifies_titles_with_punctuation_and_spaces() {
        // En-dash / em-dash, spaces, and other punctuation collapse to a
        // single underscore — never a doubled `__`.
        assert_eq!(
            sanitize_filename("Top Stores — Weekly Sales"),
            "Top_Stores_Weekly_Sales"
        );
        assert_eq!(
            sanitize_filename("Sales by Region (2024)"),
            "Sales_by_Region_2024"
        );
    }

    #[test]
    fn alphanumerics_pass_through_unchanged() {
        assert_eq!(sanitize_filename("abc123"), "abc123");
    }

    #[test]
    fn falls_back_to_query_for_empty_or_punctuation_only() {
        assert_eq!(sanitize_filename(""), "query");
        assert_eq!(sanitize_filename("   "), "query");
        assert_eq!(sanitize_filename("!!!"), "query");
    }

    #[test]
    fn strips_leading_and_trailing_underscores() {
        // Punctuation at edges shouldn't leave `_` runs.
        assert_eq!(sanitize_filename("  hello  "), "hello");
        assert_eq!(sanitize_filename("---hi---"), "hi");
    }

    #[test]
    fn collapses_runs_of_separators_to_one_underscore() {
        assert_eq!(sanitize_filename("a   b   c"), "a_b_c");
        assert_eq!(sanitize_filename("a---b"), "a_b");
    }

    #[test]
    fn truncates_at_60_chars() {
        let long = "a".repeat(80);
        let out = sanitize_filename(&long);
        assert_eq!(out.chars().count(), 60);
    }

    #[test]
    fn non_ascii_alphanumerics_are_stripped() {
        // We only keep ASCII alphanumerics; CJK chars are not preserved.
        assert_eq!(sanitize_filename("你好"), "query");
        assert_eq!(sanitize_filename("hello 你好 world"), "hello_world");
    }
}
