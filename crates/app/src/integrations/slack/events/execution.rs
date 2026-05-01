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

use std::time::Duration;

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

    // Resolved once at startup (see `SlackConfig`); flipping the env var
    // mid-process is a no-op so production deployment semantics stay
    // predictable.
    let upload_charts = crate::integrations::slack::config::chart_upload_enabled();

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
    // `finalize` runs the parallel chart-upload step.
    let renderer = SlackRenderer::new(
        &client,
        &bot_token,
        &req.channel_id,
        &req.thread_ts,
        thread_url.clone(),
        req.workspace_id,
        upload_charts,
    );
    let render_result = oxy::render_stream(rx, renderer).await;
    let body_markdown = render_result.body;
    let queued_charts = render_result.queued_charts;
    let chart_local_paths = render_result.chart_local_paths;
    // Render-side failures only — upload failures are tracked separately
    // in the post-message upload loop and surfaced via a follow-up.
    let failed_chart_count = render_result.failed_chart_count;
    let captured_sql_artifacts = render_result.captured_sql_artifacts;
    tracing::info!(
        captured_count = captured_sql_artifacts.len(),
        body_len = body_markdown.len(),
        queued_charts = queued_charts.len(),
        "slack run_for_slack: render_stream finished"
    );

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
    //   1. body section blocks (prose only — charts auto-share as thread
    //      replies right after this message lands)
    //   2. local-disk breadcrumb context blocks (dev mode only)
    //   3. render-failure context link (when a chart never produced bytes)
    //   4. divider
    //   5. footer actions row: [📎 View N SQL queries (primary, when SQL
    //      artifacts exist)] [View thread] [Wrong workspace? (when user has
    //      multiple workspaces)] — all in one actions block so they sit on
    //      the same line; SQL button is `style: "primary"` so Slack renders
    //      it green to mark it as the actionable button on the row.
    //   6. context block: "Requested by @user · Oxy can make mistakes…"
    //
    // Why the chart/SQL split is asymmetric:
    //   • Charts upload immediately. They benefit from inline visibility —
    //     a chart is the answer for visualization questions, hiding it
    //     behind a click would defeat the purpose. They can't be
    //     `slack_file` image blocks inline because Slack rejects messages
    //     referencing un-shared files; auto-share-as-thread-reply is the
    //     proven pattern (Datadog, Grafana).
    //   • SQL queries are deferred behind a button. Most viewers in a
    //     team channel don't want to scroll past a wall of SQL — the
    //     analyst who asked the question does. Compiled semantic-layer
    //     queries are especially long. Click → upload as `.sql` snippets
    //     (collapsed by default, native syntax highlighting) gives the
    //     same end-state for users who want it without spamming everyone
    //     who doesn't.
    let mut all_blocks: Vec<serde_json::Value> = body_blocks;

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

    // Render-time failure footer: charts that never produced bytes are
    // surfaced via a "view in Oxygen" link in the prose message. Upload
    // failures are handled in a follow-up post once we know the count.
    if !agent_errored
        && failed_chart_count > 0
        && let Some(url) = thread_url.as_deref()
    {
        let label = if failed_chart_count == 1 {
            "⚠️ Chart render failed — view in Oxygen →".to_string()
        } else {
            format!("⚠️ {failed_chart_count} chart renders failed — view in Oxygen →")
        };
        all_blocks.push(serde_json::json!({
            "type": "context",
            "elements": [{
                "type": "mrkdwn",
                "text": format!("<{url}|{label}>"),
            }],
        }));
    }

    // SQL artifacts — defer the file uploads behind a "📎 View N SQL
    // queries" button rather than auto-spamming the thread with up to 10
    // `.sql` snippet replies. Stash the captured list in a process-local
    // cache keyed by a synthetic upload id and pass that id into
    // `build_footer_actions` below — the button rides on the same row
    // as "View thread" so the footer stays a single line. Charts still
    // upload immediately (right above) — the split is intentional:
    // charts benefit from inline visibility, SQL is opt-in since most
    // viewers in a team channel don't need to see the query.
    let captured_sql_total = captured_sql_artifacts.len();
    let sql_to_upload = captured_sql_total.min(MAX_SQL_ARTIFACTS_PER_MESSAGE);
    let sql_overflow = captured_sql_total.saturating_sub(MAX_SQL_ARTIFACTS_PER_MESSAGE);
    if sql_overflow > 0 {
        tracing::warn!(
            captured = captured_sql_total,
            uploaded = sql_to_upload,
            dropped = sql_overflow,
            "sql artifact cap reached; some queries will not be uploaded even if the user clicks the button"
        );
    }
    let view_sql: Option<(uuid::Uuid, usize)> = if !agent_errored && sql_to_upload > 0 {
        let to_stash: Vec<_> = captured_sql_artifacts
            .into_iter()
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
        all_blocks.push(blocks::build_footer_actions(
            url,
            reopen_q.as_deref(),
            view_sql,
        ));
    } else if let Some((upload_id, count)) = view_sql {
        // Edge case: agent succeeded with SQL artifacts but no thread URL
        // (Slack misconfigured / `app_base_url` unset). Render the SQL
        // button standalone — the user can still pull the queries even
        // though we have no Oxy thread to link to.
        all_blocks.push(blocks::build_view_sql_only_actions(upload_id, count));
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

    // Charts get uploaded *after* the prose message so the thread reads
    // text-first, then chart-by-chart. Each upload auto-shares into the
    // thread (`channel_id` + `thread_ts` on `files.completeUploadExternal`),
    // surfacing as a thread reply with the file inline.
    //
    // Sequential upload — Slack edge nodes are usually under a second per
    // file, and parallel completion would scramble the file ordering in
    // the thread (charts would arrive in network-completion order rather
    // than the order the agent emitted them).
    //
    // Each upload is bounded by `FILE_UPLOAD_TIMEOUT` so a stuck
    // multipart POST can't hold up the rest of the queue (or the
    // ThreadContextService::update_last_ts that follows). On timeout we
    // log, count it as a failure, and move on.
    //
    // The Slack file's title is derived from the user's question (the
    // same logic that builds the Assistant thread title above). When the
    // agent emits multiple charts in one answer, we suffix " (1 of N)"
    // so each chart card is uniquely labeled instead of every entry just
    // saying "Chart". Filename uses a slug of the question so the
    // download experience matches the in-Slack label.
    let chart_label_base = chart_label_from_question(&req.question);
    let chart_filename_stem = chart_filename_stem_from_question(&req.question);
    let total_charts = queued_charts.len();
    let mut upload_failures: usize = 0;
    if !agent_errored {
        for (idx, chart) in queued_charts.into_iter().enumerate() {
            let title = if total_charts > 1 {
                format!("{chart_label_base} ({} of {total_charts})", idx + 1)
            } else {
                chart_label_base.clone()
            };
            let filename = if total_charts > 1 {
                format!("{}-{}.png", chart_filename_stem, idx + 1)
            } else {
                format!("{chart_filename_stem}.png")
            };
            let chart_src = chart.chart_src;
            let upload = client.files_upload_v2(
                &bot_token,
                &req.channel_id,
                Some(&req.thread_ts),
                &filename,
                chart.png_bytes,
                Some(&title),
                "image/png",
            );
            match tokio::time::timeout(FILE_UPLOAD_TIMEOUT, upload).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    tracing::warn!(chart_src = %chart_src, "files.uploadV2 failed: {e}");
                    upload_failures += 1;
                }
                Err(_) => {
                    tracing::warn!(
                        chart_src = %chart_src,
                        timeout_secs = FILE_UPLOAD_TIMEOUT.as_secs(),
                        "files.uploadV2 timed out"
                    );
                    upload_failures += 1;
                }
            }
        }

        // Upload-failure follow-up: if any uploads failed, post a small
        // context message in the same thread linking back to Oxygen so
        // the user can still see the missing chart(s).
        if upload_failures > 0
            && let Some(url) = thread_url.as_deref()
        {
            let label = if upload_failures == 1 {
                "⚠️ Chart upload failed — view in Oxygen →".to_string()
            } else {
                format!("⚠️ {upload_failures} chart uploads failed — view in Oxygen →")
            };
            let blocks = serde_json::json!([{
                "type": "context",
                "elements": [{
                    "type": "mrkdwn",
                    "text": format!("<{url}|{label}>"),
                }],
            }]);
            if let Err(e) = client
                .chat_post_message_with_blocks(
                    &bot_token,
                    &req.channel_id,
                    "Some charts couldn't be uploaded",
                    Some(&req.thread_ts),
                    Some(blocks),
                )
                .await
            {
                tracing::warn!("upload-failure follow-up post failed: {e}");
            }
        }
    }

    // SQL upload itself happens later, on user click — see the
    // `view_sql_artifacts` interactivity handler. The button + cache
    // entry are wired into the prose message above, before postMessage.

    // Cap-overflow follow-up: when more than MAX_SQL_ARTIFACTS_PER_MESSAGE
    // SQL artifacts were captured, we only upload the first cap-many. The
    // inline `> 📎 _title_ ✓` placeholder still appears in the prose for
    // every artifact, so users would see "12 placeholders, 10 .sql files"
    // and reasonably wonder where the missing two went. A small context
    // block linking to the Oxy thread closes the gap. Only posted when a
    // thread URL is available — without it, there's nowhere to link.
    if !agent_errored
        && sql_overflow > 0
        && let Some(url) = thread_url.as_deref()
    {
        let plural = if sql_overflow == 1 {
            "query"
        } else {
            "queries"
        };
        let label = format!("📎 {sql_overflow} more {plural} — view in Oxygen →");
        let blocks = serde_json::json!([{
            "type": "context",
            "elements": [{
                "type": "mrkdwn",
                "text": format!("<{url}|{label}>"),
            }],
        }]);
        if let Err(e) = client
            .chat_post_message_with_blocks(
                &bot_token,
                &req.channel_id,
                "More SQL queries are available in Oxygen",
                Some(&req.thread_ts),
                Some(blocks),
            )
            .await
        {
            tracing::warn!("sql cap-overflow follow-up post failed: {e}");
        }
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
