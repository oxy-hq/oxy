//! Slack Block Kit construction helpers.
//!
//! Pure formatting layer. Every function here takes plain inputs (strings,
//! numbers, refs) and returns `serde_json::Value` — no IO, no Slack client
//! calls, no agent state. Keeping these out of `events/execution.rs` lets
//! the orchestrator stay focused on lifecycle (setStatus → drain agent →
//! postMessage) and lets the renderer / future surfaces share the same
//! card vocabulary without dragging in Slack-event types.
//!
//! Block kinds covered:
//!
//! - **`markdown` blocks** — body prose, split at safe boundaries to
//!   stay under Slack's per-block limits.
//! - **`actions` block** — footer CTAs ("View thread", "Wrong workspace?").
//! - **`context` block** — muted-grey attribution / disclaimer footer.
//! - **`alert` block** — error path replacement for the body.
//!
//! See `<https://docs.slack.dev/reference/block-kit/>` for the full
//! Block Kit reference; URLs to the specific block docs are inlined on
//! each helper.

/// Slack `markdown` blocks share a 12,000-char *cumulative* limit across
/// all markdown blocks in a single payload. We split prose across multiple
/// markdown blocks at ~2900 chars each (well under the cumulative cap).
/// Splits prefer newline boundaries so paragraphs don't get sliced mid-sentence.
/// <https://docs.slack.dev/reference/block-kit/blocks/markdown-block>
const MARKDOWN_BLOCK_TEXT_MAX: usize = 2900;

/// Slice the agent's already-rendered markdown body into Slack `markdown`
/// blocks. The renderer has already absorbed every directive — chart
/// links, artifact subtext — into the markdown string, so this is a pure
/// "split into blocks" pass.
pub fn build_body_blocks(body_markdown: &str) -> Vec<serde_json::Value> {
    let mut blocks: Vec<serde_json::Value> = Vec::new();
    push_text_as_sections(&mut blocks, body_markdown);
    blocks
}

/// Append `text` to `blocks` as one or more `markdown` blocks, splitting at
/// the nearest newline boundary when a single block would exceed our
/// `MARKDOWN_BLOCK_TEXT_MAX` budget. Empty / whitespace-only segments are
/// skipped.
///
/// We use the dedicated `markdown` block (not a `section` block with
/// `mrkdwn` text) because it accepts **standard markdown** — including
/// `[text](url)` link syntax — exactly as the agent's LLM emits it.
/// Section-with-mrkdwn would force us to translate every link to Slack's
/// proprietary `<url|text>` syntax, and any `[text](url)` we missed would
/// leak as raw text.
fn push_text_as_sections(blocks: &mut Vec<serde_json::Value>, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    for chunk in split_at_section_boundary(text) {
        let chunk_trim = chunk.trim();
        if chunk_trim.is_empty() {
            continue;
        }
        blocks.push(serde_json::json!({
            "type": "markdown",
            "text": chunk_trim,
        }));
    }
}

/// Split `text` into chunks each at most `MARKDOWN_BLOCK_TEXT_MAX` chars,
/// preferring the latest newline boundary inside the window. Falls back
/// to a hard char-boundary cut when no newline exists in range.
fn split_at_section_boundary(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        // Take up to MARKDOWN_BLOCK_TEXT_MAX chars (not bytes).
        let char_count = remaining.chars().count();
        let take_chars = char_count.min(MARKDOWN_BLOCK_TEXT_MAX);
        if take_chars == char_count {
            out.push(remaining.to_string());
            break;
        }
        // Look for the last `\n` within the take window for a clean split.
        let take_byte_end = remaining
            .char_indices()
            .nth(take_chars)
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());
        let head = &remaining[..take_byte_end];
        let split_at = head.rfind('\n').map(|i| i + 1).unwrap_or(take_byte_end);
        let (chunk, rest) = remaining.split_at(split_at);
        out.push(chunk.to_string());
        remaining = rest;
    }
    out
}

/// Pick the text fallback for `chat.stopStream` / `chat.postMessage`. Used
/// for notifications, search, and any client that renders `text` instead
/// of the Block Kit blocks payload.
pub fn pick_fallback_text(agent_errored: bool, final_markdown: &str) -> String {
    if agent_errored {
        return final_markdown.to_string();
    }
    if final_markdown.trim().is_empty() {
        return "✅ Task completed".to_string();
    }
    final_markdown.to_string()
}

/// Single-button actions block: "View thread" deep-links into the Oxy
/// web UI for this conversation. Mirrors Claude's "View session" button
/// — one primary CTA per response, no card chrome.
pub fn build_view_thread_actions(thread_url: &str) -> serde_json::Value {
    build_footer_actions(thread_url, None)
}

/// Footer actions block. Always emits "View thread"; emits a secondary
/// "Wrong workspace?" button when `reopen_picker_question_b64` is `Some`.
///
/// The reopen button re-uses the existing workspace picker — clicking it
/// posts the same ephemeral picker we show on first-message ambiguity,
/// pre-loaded with the original question so submitting re-runs against
/// the chosen workspace. Caller should pass `None` when the user has only
/// one workspace to choose from (button would be dead clutter).
pub fn build_footer_actions(
    thread_url: &str,
    reopen_picker_question_b64: Option<&str>,
) -> serde_json::Value {
    let mut elements = vec![serde_json::json!({
        "type": "button",
        "action_id": "slack_view_thread",
        "text": {"type": "plain_text", "text": "View thread"},
        "url": thread_url,
    })];
    if let Some(encoded) = reopen_picker_question_b64 {
        elements.push(serde_json::json!({
            "type": "button",
            "action_id": "slack_reopen_picker",
            "text": {"type": "plain_text", "text": "Wrong workspace?"},
            "value": encoded,
        }));
    }
    serde_json::json!({
        "type": "actions",
        "elements": elements,
    })
}

/// Derive a short, user-friendly agent name from a raw agent path. Matches
/// the format used by the Slack home-tab picker (raw file stem), so users
/// see the same label in the footer and in the workspace/agent selector.
///
/// Examples:
/// - `agents/analytics.agentic.yml` → `"analytics"`
/// - `agents/duckdb.agent.yml` → `"duckdb"`
/// - unknown shape → the original input (preserves debuggability)
pub fn agent_display_name(agent_path: &str) -> String {
    let file = std::path::Path::new(agent_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(agent_path);
    let stem = file
        .strip_suffix(".agentic.yml")
        .or_else(|| file.strip_suffix(".agent.yml"))
        .or_else(|| file.strip_suffix(".yml"))
        .unwrap_or(file);
    if stem.is_empty() {
        agent_path.to_string()
    } else {
        stem.to_string()
    }
}

/// Build the attribution context block — quiet metadata footer.
///
/// Rendered (smaller muted-grey text):
///   "Replied by *agent* · Requested by <@U123>"
///
/// Context blocks are Slack's idiomatic surface for footers / metadata —
/// they render in smaller, muted-grey text so they don't compete with
/// the agent's answer above. `<@user_id>` mrkdwn syntax expands into the
/// user's display name with their workspace-specific colour.
///
/// <https://docs.slack.dev/reference/block-kit/blocks/context-block>
pub fn build_attribution_context(slack_user_id: &str, agent_display: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "context",
        "elements": [{
            "type": "mrkdwn",
            "text": format!("Replied by *{agent_display}* · Requested by <@{slack_user_id}>"),
        }]
    })
}

/// Render a single-block alert for error paths (level: "error").
/// Used when the agent run failed before streaming could start, or as a
/// stopStream final override when the agent errored mid-stream.
/// <https://docs.slack.dev/reference/block-kit/blocks/alert-block>
pub fn build_error_alert_blocks(message: &str) -> Vec<serde_json::Value> {
    vec![serde_json::json!({
        "type": "alert",
        "level": "error",
        "text": {
            "type": "mrkdwn",
            "text": message,
        }
    })]
}

#[cfg(test)]
mod agent_display_name_tests {
    use super::agent_display_name;

    #[test]
    fn strips_agentic_yml() {
        assert_eq!(
            agent_display_name("agents/analytics.agentic.yml"),
            "analytics"
        );
    }

    #[test]
    fn strips_agent_yml() {
        assert_eq!(agent_display_name("agents/duckdb.agent.yml"), "duckdb");
    }

    #[test]
    fn strips_plain_yml_fallback() {
        assert_eq!(agent_display_name("custom/router.yml"), "router");
    }

    #[test]
    fn returns_original_for_unknown_shape() {
        assert_eq!(agent_display_name("agent"), "agent");
    }

    #[test]
    fn handles_bare_filename() {
        assert_eq!(agent_display_name("analytics.agentic.yml"), "analytics");
    }
}

#[cfg(test)]
mod footer_actions_tests {
    use super::{build_footer_actions, build_view_thread_actions};

    #[test]
    fn footer_emits_only_view_thread_when_no_reopen() {
        let v = build_footer_actions("https://oxy.test/threads/abc", None);
        let elements = v["elements"].as_array().expect("elements array");
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0]["action_id"], "slack_view_thread");
        assert_eq!(elements[0]["url"], "https://oxy.test/threads/abc");
    }

    #[test]
    fn footer_emits_reopen_button_when_question_provided() {
        let v = build_footer_actions("https://oxy.test/threads/abc", Some("aGVsbG8="));
        let elements = v["elements"].as_array().expect("elements array");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0]["action_id"], "slack_view_thread");
        assert_eq!(elements[1]["action_id"], "slack_reopen_picker");
        assert_eq!(elements[1]["text"]["text"], "Wrong workspace?");
        assert_eq!(elements[1]["value"], "aGVsbG8=");
        // Reopen button must NOT be styled "primary" — View thread stays the
        // dominant CTA.
        assert!(elements[1].get("style").is_none());
    }

    #[test]
    fn build_view_thread_actions_remains_back_compat_with_no_reopen() {
        let v = build_view_thread_actions("https://oxy.test/threads/abc");
        let elements = v["elements"].as_array().expect("elements array");
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0]["action_id"], "slack_view_thread");
    }
}
