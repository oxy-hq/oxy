//! Host-side adapter implementing
//! [`agentic_pipeline::workflow_run::InlineAgentRunner`].
//!
//! `agentic-pipeline` doesn't know how to run an agent — that's a host
//! concern. This adapter bridges the two: it builds an oxy-side
//! `ExecutionContext` and dispatches through `AgentLauncherExecutable`,
//! returning the final answer text. The pipeline calls this synchronously
//! when a workflow's agent fan-out (consistency runs) hits the inline
//! runner.

use std::sync::Arc;

use agentic_pipeline::workflow_run::InlineAgentRunner;
use async_trait::async_trait;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::execute::{
    Executable, ExecutionContext,
    renderer::Renderer,
    types::{Event, Output, OutputContainer, Source},
};
use oxy_agent::{AgentLauncherExecutable, types::AgentInput};

/// Channel buffer for the throwaway event stream we open to satisfy
/// `ExecutionContext`'s required writer. The agent's events aren't
/// consumed from inline; if the agent stalls the buffer just back-pressures.
const EVENT_CHANNEL_SIZE: usize = 64;

pub struct OxyInlineAgentRunner {
    workspace: WorkspaceManager,
}

impl OxyInlineAgentRunner {
    pub fn new(workspace: WorkspaceManager) -> Self {
        Self { workspace }
    }

    /// Build a vanilla `ExecutionContext` suitable for a single agent
    /// invocation — no filters, no connection overrides, throwaway event
    /// channel. Returns the receiver alongside so the caller can drain it
    /// (or, more typically, drop it once the run finishes).
    fn make_execution_context(
        &self,
        kind: &str,
    ) -> (ExecutionContext, tokio::sync::mpsc::Receiver<Event>) {
        let (tx, rx) = tokio::sync::mpsc::channel::<Event>(EVENT_CHANNEL_SIZE);
        let source = Source {
            parent_id: None,
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.to_string(),
        };
        let renderer = Renderer::new(minijinja::context! {});
        let ctx = ExecutionContext::new(source, renderer, self.workspace.clone(), tx, None, None);
        (ctx, rx)
    }
}

#[async_trait]
impl InlineAgentRunner for OxyInlineAgentRunner {
    async fn run_agent(&self, agent_ref: &str, prompt: &str) -> Result<String, String> {
        let (ctx, mut rx) = self.make_execution_context("inline_agent");

        // Drain events into the void so the channel doesn't block the
        // launcher. We don't surface them in inline mode — any consumer
        // that wants live agent output should use the queue path.
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });

        let input = AgentInput {
            agent_ref: agent_ref.to_string(),
            prompt: prompt.to_string(),
            memory: vec![],
            variables: None,
            a2a_task_id: None,
            a2a_thread_id: None,
            a2a_context_id: None,
            sandbox_info: None,
        };

        let result = AgentLauncherExecutable
            .execute(&ctx, input)
            .await
            .map_err(|e| format!("agent launcher: {e}"))?;
        drop(ctx);
        drain.await.ok();

        Ok(output_container_to_text(&result))
    }
}

/// Project an `OutputContainer` onto a single text answer.
///
/// Agents normally produce `Output::Text`; for richer outputs (Table,
/// Metadata wrappers, etc.) we serialize the whole container as JSON so
/// the consistency picker downstream still has something well-formed
/// to compare. Final text shape is what the queue path's
/// `aggregate_child_results` returns.
fn output_container_to_text(output: &OutputContainer) -> String {
    match output {
        OutputContainer::Single(Output::Text(text)) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Convenience wrapper for callers that need an `Arc<dyn InlineAgentRunner>`
/// (e.g. to pass through a builder).
pub fn shared(workspace: WorkspaceManager) -> Arc<dyn InlineAgentRunner> {
    Arc::new(OxyInlineAgentRunner::new(workspace))
}
