//! Client-renderer abstraction for agent output streams.
//!
//! Agents emit structured `AnswerContent`; each surface (CLI, web, Slack)
//! implements [`ClientRenderer`]. [`render_stream`] dispatches each variant
//! to the matching callback. Default callbacks are no-ops, so renderers
//! override only what they display.

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::execute::types::Usage;
use crate::execute::types::event::{ArtifactKind, Step};
use crate::types::{AnswerContent, AnswerStream, ArtifactValue};

/// Renders an agent stream into surface-specific output. All callbacks
/// default to no-ops; only `finalize` is required.
#[async_trait]
pub trait ClientRenderer: Send {
    /// Surface-specific terminal output (markdown, blocks, `()`, …).
    type Output: Send;

    async fn on_text(&mut self, _content: &str) {}

    async fn on_reasoning_started(&mut self, _id: &str) {}
    async fn on_reasoning_chunk(&mut self, _id: &str, _delta: &str) {}
    async fn on_reasoning_done(&mut self, _id: &str) {}

    /// Chart artifact from the visualize tool. `chart_src` is the JSON
    /// filename; renderer decides how to surface it.
    async fn on_chart(&mut self, _chart_src: &str) {}

    async fn on_artifact_started(
        &mut self,
        _id: &str,
        _title: &str,
        _kind: &ArtifactKind,
        _is_verified: bool,
    ) {
    }
    async fn on_artifact_value(&mut self, _id: &str, _value: &ArtifactValue) {}
    async fn on_artifact_done(&mut self, _id: &str, _error: Option<&str>) {}

    async fn on_step_started(&mut self, _step: &Step) {}
    async fn on_step_finished(&mut self, _step_id: &str, _error: Option<&str>) {}

    async fn on_error(&mut self, _message: &str) {}
    async fn on_usage(&mut self, _usage: &Usage) {}

    /// Catch-all for events without a structured callback (e.g. `DataApp`).
    async fn on_data_app(&mut self, _file_path: &str) {}

    async fn finalize(self) -> Self::Output;
}

/// Drain `rx` and dispatch each event onto the matching renderer callback.
pub async fn render_stream<R: ClientRenderer>(
    mut rx: mpsc::Receiver<AnswerStream>,
    mut renderer: R,
) -> R::Output {
    while let Some(event) = rx.recv().await {
        match event.content {
            AnswerContent::Text { content } => renderer.on_text(&content).await,
            AnswerContent::ReasoningStarted { id } => renderer.on_reasoning_started(&id).await,
            AnswerContent::ReasoningChunk { id, delta } => {
                renderer.on_reasoning_chunk(&id, &delta).await
            }
            AnswerContent::ReasoningDone { id } => renderer.on_reasoning_done(&id).await,
            AnswerContent::Chart { chart_src } => renderer.on_chart(&chart_src).await,
            AnswerContent::ArtifactStarted {
                id,
                title,
                is_verified,
                kind,
            } => {
                renderer
                    .on_artifact_started(&id, &title, &kind, is_verified)
                    .await
            }
            AnswerContent::ArtifactValue { id, value } => {
                renderer.on_artifact_value(&id, &value).await
            }
            AnswerContent::ArtifactDone { id, error } => {
                renderer.on_artifact_done(&id, error.as_deref()).await
            }
            AnswerContent::StepStarted { step } => renderer.on_step_started(&step).await,
            AnswerContent::StepFinished { step_id, error } => {
                renderer.on_step_finished(&step_id, error.as_deref()).await
            }
            AnswerContent::Error { message } => renderer.on_error(&message).await,
            AnswerContent::Usage { usage } => renderer.on_usage(&usage).await,
            AnswerContent::DataApp { file_path } => renderer.on_data_app(&file_path).await,
        }
    }
    renderer.finalize().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute::types::event::StepKind;
    use crate::types::{AnswerContent, AnswerStream};

    /// Records each callback invocation so tests can assert dispatch.
    #[derive(Debug, PartialEq)]
    enum Captured {
        Text(String),
        ReasoningStarted(String),
        ReasoningChunk(String, String),
        ReasoningDone(String),
        Chart(String),
        ArtifactStarted(String, String, bool),
        ArtifactDone(String, Option<String>),
        StepStarted(String),
        StepFinished(String, Option<String>),
        Error(String),
    }

    #[derive(Default)]
    struct Recorder {
        events: Vec<Captured>,
    }

    #[async_trait]
    impl ClientRenderer for Recorder {
        type Output = Vec<Captured>;

        async fn on_text(&mut self, content: &str) {
            self.events.push(Captured::Text(content.to_string()));
        }
        async fn on_reasoning_started(&mut self, id: &str) {
            self.events.push(Captured::ReasoningStarted(id.to_string()));
        }
        async fn on_reasoning_chunk(&mut self, id: &str, delta: &str) {
            self.events
                .push(Captured::ReasoningChunk(id.to_string(), delta.to_string()));
        }
        async fn on_reasoning_done(&mut self, id: &str) {
            self.events.push(Captured::ReasoningDone(id.to_string()));
        }
        async fn on_chart(&mut self, chart_src: &str) {
            self.events.push(Captured::Chart(chart_src.to_string()));
        }
        async fn on_artifact_started(
            &mut self,
            id: &str,
            title: &str,
            _kind: &ArtifactKind,
            is_verified: bool,
        ) {
            self.events.push(Captured::ArtifactStarted(
                id.to_string(),
                title.to_string(),
                is_verified,
            ));
        }
        async fn on_artifact_done(&mut self, id: &str, error: Option<&str>) {
            self.events.push(Captured::ArtifactDone(
                id.to_string(),
                error.map(str::to_string),
            ));
        }
        async fn on_step_started(&mut self, step: &Step) {
            self.events.push(Captured::StepStarted(step.id.clone()));
        }
        async fn on_step_finished(&mut self, step_id: &str, error: Option<&str>) {
            self.events.push(Captured::StepFinished(
                step_id.to_string(),
                error.map(str::to_string),
            ));
        }
        async fn on_error(&mut self, message: &str) {
            self.events.push(Captured::Error(message.to_string()));
        }
        async fn finalize(self) -> Self::Output {
            self.events
        }
    }

    fn stream(content: AnswerContent) -> AnswerStream {
        AnswerStream {
            content,
            references: vec![],
            is_error: false,
            step: String::new(),
        }
    }

    #[tokio::test]
    async fn dispatches_each_variant_to_its_callback() {
        let (tx, rx) = mpsc::channel::<AnswerStream>(32);
        // Send representative events across all dispatcher arms.
        tx.send(stream(AnswerContent::Text {
            content: "hello".into(),
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::ReasoningStarted { id: "r1".into() }))
            .await
            .unwrap();
        tx.send(stream(AnswerContent::ReasoningChunk {
            id: "r1".into(),
            delta: "thinking".into(),
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::ReasoningDone { id: "r1".into() }))
            .await
            .unwrap();
        tx.send(stream(AnswerContent::Chart {
            chart_src: "abc.json".into(),
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::ArtifactStarted {
            id: "a1".into(),
            title: "Sales".into(),
            is_verified: true,
            kind: ArtifactKind::SemanticQuery {},
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::ArtifactDone {
            id: "a1".into(),
            error: None,
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::StepStarted {
            step: Step {
                id: "s1".into(),
                kind: StepKind::Plan,
                objective: None,
            },
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::StepFinished {
            step_id: "s1".into(),
            error: None,
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::Error {
            message: "boom".into(),
        }))
        .await
        .unwrap();
        drop(tx);

        let captured = render_stream(rx, Recorder::default()).await;
        assert_eq!(
            captured,
            vec![
                Captured::Text("hello".into()),
                Captured::ReasoningStarted("r1".into()),
                Captured::ReasoningChunk("r1".into(), "thinking".into()),
                Captured::ReasoningDone("r1".into()),
                Captured::Chart("abc.json".into()),
                Captured::ArtifactStarted("a1".into(), "Sales".into(), true),
                Captured::ArtifactDone("a1".into(), None),
                Captured::StepStarted("s1".into()),
                Captured::StepFinished("s1".into(), None),
                Captured::Error("boom".into()),
            ]
        );
    }

    #[tokio::test]
    async fn finalize_runs_when_channel_closes_with_no_events() {
        let (_tx, rx) = mpsc::channel::<AnswerStream>(1);
        drop(_tx);
        let captured = render_stream(rx, Recorder::default()).await;
        assert!(captured.is_empty());
    }

    /// Uninterested events must not panic or block the driver.
    #[tokio::test]
    async fn default_noop_methods_swallow_events_silently() {
        struct OnlyText {
            buf: String,
        }
        #[async_trait]
        impl ClientRenderer for OnlyText {
            type Output = String;
            async fn on_text(&mut self, content: &str) {
                self.buf.push_str(content);
            }
            async fn finalize(self) -> Self::Output {
                self.buf
            }
        }

        let (tx, rx) = mpsc::channel::<AnswerStream>(8);
        tx.send(stream(AnswerContent::Text {
            content: "before ".into(),
        }))
        .await
        .unwrap();
        // These should be silently ignored by `OnlyText`.
        tx.send(stream(AnswerContent::ReasoningChunk {
            id: "r".into(),
            delta: "ignored".into(),
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::Chart {
            chart_src: "ignored.json".into(),
        }))
        .await
        .unwrap();
        tx.send(stream(AnswerContent::Text {
            content: "after".into(),
        }))
        .await
        .unwrap();
        drop(tx);

        let result = render_stream(rx, OnlyText { buf: String::new() }).await;
        assert_eq!(result, "before after");
    }
}
