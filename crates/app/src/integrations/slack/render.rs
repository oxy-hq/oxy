//! `SlackRenderer` — [`oxy::ClientRenderer`] impl for Slack.
//!
//! Prose is buffered, not streamed: live token reveal looks frozen during
//! tool calls (the bubble shows "Let me check…" while SQL runs for 10s).
//! The `loading_messages` rotation via `setStatus` carries the "alive"
//! signal; `chat.postMessage` reveals the full prose at the end.
//!
//! Charts: render PNG bytes during streaming and queue them. The actual
//! upload happens *after* `chat.postMessage` posts the prose, so the text
//! answer always reaches the thread first and the auto-shared chart files
//! land as thread replies underneath. Embedding charts as `slack_file`
//! blocks inside the prose message looked tidier but turned out to be
//! brittle — Slack rejects the message when the file isn't yet visible
//! to the channel — so we use the same "post text, then auto-share" shape
//! Datadog and Grafana use. See `events::execution::run_for_slack` for
//! the orchestration. When `OXY_SLACK_CHART_UPLOAD` is unset (the dev
//! default), uploads are skipped and PNGs surface as on-disk paths in a
//! footer context block — Slack can't fetch localhost so there's no
//! inline preview, but devs can `open` the file.
//!
//! Reasoning events are dropped — Slack has no collapsible-reasoning UI.

use std::path::PathBuf;

use async_trait::async_trait;
use oxy::ClientRenderer;
use oxy::execute::types::Usage;
use oxy::execute::types::event::{ArtifactKind, Step};
use oxy::types::ArtifactValue;
use uuid::Uuid;

use crate::integrations::slack::chart_render::{ensure_chart_png_cached, get_or_render_chart_png};
use crate::integrations::slack::client::SlackClient;
use crate::integrations::slack::events::artifact_filter::ArtifactFilter;

/// Status messages Slack rotates as the AI-assistant indicator. Pair with
/// a non-empty `status` string in `setStatus` — an empty string clears
/// the indicator. We keep 5 entries: half the original 10-message list,
/// enough motion to read as "alive" without the shuffly carousel.
///
/// Verbs are deliberately neutral and product-flavored — they describe
/// what an Oxy agent generally does (query, read, analyze, compile,
/// draft) rather than committing to specific outputs (e.g. "Visualizing…"
/// would be misleading on runs that don't produce charts). Slack rotates
/// these on its own timer with no relation to actual agent state, so
/// they're ambient motion only — the plan-mode task block carries the
/// detailed progress signal. "Thinking…" is deliberately absent (Slack
/// uses it as the fallback word).
pub const LOADING_MESSAGES: &[&str] = &[
    "Querying data…",
    "Reading results…",
    "Analyzing…",
    "Compiling insights…",
    "Drafting answer…",
];

/// PNG bytes for a chart, queued during streaming and uploaded to Slack
/// after the prose `chat.postMessage` has landed. `chart_src` is the
/// source JSON filename — used to derive the Slack file display name.
pub struct QueuedChart {
    pub chart_src: String,
    pub png_bytes: Vec<u8>,
}

/// Output of a Slack render run.
///
/// * `body` — accumulated prose markdown.
/// * `queued_charts` — PNG bytes ready to upload. The caller (in
///   `events::execution`) drives the upload after the prose message
///   posts, so the text appears first in the thread.
/// * `chart_local_paths` — on-disk PNGs surfaced for dev inspection
///   when `OXY_SLACK_CHART_UPLOAD` is unset.
/// * `failed_chart_count` — render-side failures only (Chromium crash,
///   missing JSON). Upload-side failures live in a separate counter on
///   the orchestration side in `events::execution` so each surface gets
///   its own warning footer.
pub struct SlackRenderResult {
    pub body: String,
    pub queued_charts: Vec<QueuedChart>,
    pub chart_local_paths: Vec<PathBuf>,
    pub failed_chart_count: usize,
}

pub struct SlackRenderer<'a> {
    client: &'a SlackClient,
    bot_token: &'a str,
    channel: &'a str,
    thread_ts: &'a str,
    workspace_id: Uuid,
    /// When `true`, queued PNG bytes are uploaded to Slack after the
    /// prose message lands. When `false`, PNGs render to disk only and
    /// the path is surfaced as a context-block breadcrumb — the dev
    /// path that doesn't depend on a real Slack workspace.
    upload_charts: bool,

    body: String,
    artifact_filter: ArtifactFilter,
    queued_charts: Vec<QueuedChart>,
    chart_local_paths: Vec<PathBuf>,
    /// Charts that never made it past the render step (Chromium crash,
    /// missing chart JSON). Upload-side failures live on the
    /// orchestration side in `events::execution`.
    failed_chart_count: usize,
}

impl<'a> SlackRenderer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: &'a SlackClient,
        bot_token: &'a str,
        channel: &'a str,
        thread_ts: &'a str,
        thread_url: Option<String>,
        workspace_id: Uuid,
        upload_charts: bool,
    ) -> Self {
        let artifact_filter = match thread_url {
            Some(url) => ArtifactFilter::with_thread_url(url),
            None => ArtifactFilter::new(),
        };
        Self {
            client,
            bot_token,
            channel,
            thread_ts,
            workspace_id,
            upload_charts,
            body: String::new(),
            artifact_filter,
            queued_charts: Vec::new(),
            chart_local_paths: Vec::new(),
            failed_chart_count: 0,
        }
    }
}

#[async_trait]
impl<'a> ClientRenderer for SlackRenderer<'a> {
    type Output = SlackRenderResult;

    async fn on_text(&mut self, content: &str) {
        // Filter strips `:::artifact{…}` directives and emits inline
        // subtext; everything else passes through. Accumulate only —
        // see file-level doc for why we don't live-stream prose.
        let filtered = self.artifact_filter.feed(content);
        if filtered.is_empty() {
            return;
        }
        self.body.push_str(&filtered);
    }

    // Reasoning dropped: Slack has no collapsible UI, and toggling the
    // status indicator on every span caused visible flicker.
    async fn on_reasoning_started(&mut self, _id: &str) {}
    async fn on_reasoning_chunk(&mut self, _id: &str, _delta: &str) {}
    async fn on_reasoning_done(&mut self, _id: &str) {}

    async fn on_chart(&mut self, chart_src: &str) {
        // Render failures are rare (missing JSON file, Chromium crash) and
        // a single chart that fails to render shouldn't kill the run —
        // the agent's text answer still posts.
        if self.upload_charts {
            // Upload path: render to bytes and queue for upload after the
            // prose message lands. The bytes also sit on disk in the
            // cache dir — useful for debugging if a Slack upload fails.
            match get_or_render_chart_png(self.workspace_id, chart_src).await {
                Ok(bytes) => self.queued_charts.push(QueuedChart {
                    chart_src: chart_src.to_string(),
                    png_bytes: bytes,
                }),
                Err(e) => {
                    tracing::warn!(chart_src, "chart render failed: {e}");
                    self.failed_chart_count += 1;
                }
            }
            return;
        }

        // Dev path: render to disk only, surface the on-disk path so the
        // developer can `open` the file. Slack won't fetch localhost so
        // this never produces an inline preview — it's a breadcrumb, not
        // a real image block. Avoids loading the PNG bytes into memory
        // since we don't need them.
        match ensure_chart_png_cached(self.workspace_id, chart_src).await {
            Ok(path) => {
                tracing::info!(
                    chart_src,
                    png_path = %path.display(),
                    "rendered chart PNG locally for validation"
                );
                self.chart_local_paths.push(path);
            }
            Err(e) => {
                tracing::warn!(chart_src, "chart render failed: {e}");
                self.failed_chart_count += 1;
            }
        }
    }

    async fn on_artifact_started(
        &mut self,
        _id: &str,
        _title: &str,
        _kind: &ArtifactKind,
        _is_verified: bool,
    ) {
        // No-op: the directive path (`on_text` → `ArtifactFilter`) already
        // renders inline subtext attribution; emitting here would double
        // up. Once BlockHandler stops emitting the directive, move the
        // inline placeholder here and delete `ArtifactFilter`.
    }

    async fn on_artifact_value(&mut self, _id: &str, _value: &ArtifactValue) {}
    async fn on_artifact_done(&mut self, _id: &str, _error: Option<&str>) {}

    // Step events are dropped: the run is presented as a single opaque
    // "thinking" period driven by the setStatus loading_messages rotation.
    // The final blocks land via chat.postMessage.
    async fn on_step_started(&mut self, _step: &Step) {}
    async fn on_step_finished(&mut self, _step_id: &str, _error: Option<&str>) {}

    async fn on_error(&mut self, _message: &str) {}

    async fn on_usage(&mut self, _usage: &Usage) {}

    async fn finalize(mut self) -> Self::Output {
        // Drain trailing artifact-filter state (partial fences, split
        // colons across chunks) into body before postMessage lands.
        let trailing = self.artifact_filter.finish();
        if !trailing.is_empty() {
            self.body.push_str(&trailing);
        }
        // Clear the loading indicator. Logged-and-swallowed; UX decoration.
        if let Err(e) = self
            .client
            .assistant_threads_set_status(self.bot_token, self.channel, self.thread_ts, "", None)
            .await
        {
            tracing::warn!("clear setStatus failed: {e}");
        }

        SlackRenderResult {
            body: self.body,
            queued_charts: std::mem::take(&mut self.queued_charts),
            chart_local_paths: self.chart_local_paths,
            failed_chart_count: self.failed_chart_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::slack::client::SlackClient;

    fn make_renderer<'a>(client: &'a SlackClient, upload_charts: bool) -> SlackRenderer<'a> {
        SlackRenderer::new(
            client,
            "token",
            "C123",
            "12345.6789",
            None,
            Uuid::nil(),
            upload_charts,
        )
    }

    #[tokio::test]
    async fn finalize_returns_empty_state_with_no_events() {
        // We can't call `on_chart` here — it needs a real workspace + Chromium.
        let client = SlackClient::new();
        let r = make_renderer(&client, false);
        let result = r.finalize().await;
        assert!(result.body.is_empty());
        assert!(result.queued_charts.is_empty());
        assert!(result.chart_local_paths.is_empty());
        assert_eq!(result.failed_chart_count, 0);
    }
}
