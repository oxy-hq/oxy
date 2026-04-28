//! `SlackRenderer` — [`oxy::ClientRenderer`] impl for Slack.
//!
//! Prose is buffered, not streamed: live token reveal looks frozen during
//! tool calls (the bubble shows "Let me check…" while SQL runs for 10s).
//! The `loading_messages` rotation via `setStatus` carries the "alive"
//! signal; `chat.postMessage` reveals the full prose at the end.
//!
//! Charts: prefer the `chart_image_publisher` (S3 → public URL → Slack CDN).
//! Fall back to an eager local PNG render so devs can `open` it; no image
//! block on that path because Slack can't fetch from localhost.
//!
//! Reasoning events are dropped — Slack has no collapsible-reasoning UI.

use std::path::PathBuf;

use async_trait::async_trait;
use oxy::ClientRenderer;
use oxy::execute::types::Usage;
use oxy::execute::types::event::{ArtifactKind, Step};
use oxy::storage::SharedChartImagePublisher;
use oxy::types::ArtifactValue;
use uuid::Uuid;

use crate::integrations::slack::chart_render::get_or_render_chart_png;
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

pub struct SlackRenderer<'a> {
    client: &'a SlackClient,
    bot_token: &'a str,
    channel: &'a str,
    thread_ts: &'a str,
    workspace_id: Uuid,
    chart_image_publisher: Option<SharedChartImagePublisher>,
    charts_dir: Option<PathBuf>,

    body: String,
    artifact_filter: ArtifactFilter,
    /// Public image URLs from successful chart publishes (one image block each).
    chart_image_urls: Vec<String>,
    /// On-disk PNG paths from the eager local-render fallback. Surfaced
    /// in a footer context block so devs can `open` and verify; Slack
    /// can't fetch from localhost so no image block is emitted.
    chart_local_paths: Vec<PathBuf>,
    /// Total `Chart` events seen, even when publishing failed. Drives the
    /// "View chart in Oxygen" fallback footer when no public URL is available.
    attempted_chart_count: usize,
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
        chart_image_publisher: Option<SharedChartImagePublisher>,
        charts_dir: Option<PathBuf>,
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
            chart_image_publisher,
            charts_dir,
            body: String::new(),
            artifact_filter,
            chart_image_urls: Vec::new(),
            chart_local_paths: Vec::new(),
            attempted_chart_count: 0,
        }
    }

    pub fn chart_image_urls(&self) -> &[String] {
        &self.chart_image_urls
    }

    pub fn attempted_chart_count(&self) -> usize {
        self.attempted_chart_count
    }

    pub fn chart_local_paths(&self) -> &[PathBuf] {
        &self.chart_local_paths
    }
}

#[async_trait]
impl<'a> ClientRenderer for SlackRenderer<'a> {
    /// `(body_markdown, chart_image_urls, chart_local_paths, attempted_chart_count)`
    type Output = (String, Vec<String>, Vec<PathBuf>, usize);

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
        self.attempted_chart_count += 1;

        if let Some(charts_dir) = &self.charts_dir {
            let chart_file = charts_dir.join(chart_src);
            tracing::info!(
                chart_src,
                path = %chart_file.display(),
                "chart spec written to disk"
            );
        }

        // Preferred path: publisher → public URL → Slack CDN (no proxy hop).
        if let Some(publisher) = self.chart_image_publisher.clone()
            && let Some(charts_dir) = self.charts_dir.clone()
        {
            let chart_file = charts_dir.join(chart_src);
            match tokio::fs::read_to_string(&chart_file).await {
                Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(config) => {
                        let key = format!("slack/{chart_src}");
                        match publisher.publish(&key, &config).await {
                            Ok(Some(url)) => {
                                self.chart_image_urls.push(url);
                                return;
                            }
                            Ok(None) => {
                                // Backend produced no public URL — fall
                                // through to the eager-local-render path.
                            }
                            Err(e) => {
                                tracing::warn!(
                                    chart_src,
                                    "publisher.publish failed (falling back to local render): {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            chart_src,
                            "failed to parse chart JSON (falling back to local render): {e}"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        chart_src,
                        "failed to read chart file (falling back to local render): {e}"
                    );
                }
            }
        }

        // Fallback: eager local PNG render for dev visual validation.
        // No public URL → no Slack image block. Chromium failures are
        // logged-and-swallowed so a chart event never kills the run.
        match get_or_render_chart_png(self.workspace_id, chart_src).await {
            Ok(_bytes) => {
                match crate::integrations::slack::chart_render::cached_chart_png_path(
                    self.workspace_id,
                    chart_src,
                )
                .await
                {
                    Ok(path) => {
                        tracing::info!(
                            chart_src,
                            png_path = %path.display(),
                            "rendered chart PNG locally for validation"
                        );
                        self.chart_local_paths.push(path);
                    }
                    Err(e) => {
                        tracing::warn!(chart_src, "could not resolve PNG cache path: {e}");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    chart_src,
                    "local chart render failed (open the JSON spec by hand to validate): {e}"
                );
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
        (
            self.body,
            self.chart_image_urls,
            self.chart_local_paths,
            self.attempted_chart_count,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::slack::client::SlackClient;

    /// No-op renderer for chart bookkeeping tests.
    fn make_renderer<'a>(
        client: &'a SlackClient,
        publisher: Option<SharedChartImagePublisher>,
        charts_dir: Option<PathBuf>,
    ) -> SlackRenderer<'a> {
        SlackRenderer::new(
            client,
            "token",
            "C123",
            "12345.6789",
            None,
            Uuid::nil(),
            publisher,
            charts_dir,
        )
    }

    #[tokio::test]
    async fn finalize_returns_empty_state_with_no_events() {
        // We can't call `on_chart` here — it needs a real workspace + Chromium.
        let client = SlackClient::new();
        let r = make_renderer(&client, None, None);
        let (body, urls, paths, attempted) = r.finalize().await;
        assert!(body.is_empty());
        assert!(urls.is_empty());
        assert!(paths.is_empty());
        assert_eq!(attempted, 0);
    }
}
