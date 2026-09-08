use std::fmt::Debug;

use minijinja::Value;
use tokio::sync::mpsc::Sender;

use crate::{
    adapters::{session_filters::SessionFilters, workspace::manager::WorkspaceManager},
    config::model::ConnectionOverrides,
    exec_runtime::renderer::Renderer,
    exec_types::{
        Chunk, Event, EventKind, ProgressType, Source, Usage,
        event::{SandboxAppKind, SandboxInfo, Step},
    },
    metrics::{MetricContext, SharedMetricCtx, SourceType},
};
use oxy_shared::errors::OxyError;

use super::writer::Writer;
use crate::config::WorkingCopy;

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub source: Source,
    pub writer: Sender<Event>,
    pub renderer: Renderer,
    pub workspace: WorkspaceManager<WorkingCopy>,
    /// Filters to apply to all SQL queries in this execution context
    /// Set by API request, transparent to automations/agents
    pub filters: Option<SessionFilters>,
    /// Connection overrides to apply to database connections in this execution context
    /// Set by API request, transparent to automations/agents
    pub connections: Option<ConnectionOverrides>,
    /// Sandbox information from thread (e.g., v0 chat_id and preview_url)
    /// Passed from thread to tools for continuity
    pub sandbox_info: Option<SandboxInfo>,
    /// User ID for the execution context (for run isolation)
    pub user_id: Option<uuid::Uuid>,
    /// Effective workspace role for the run's submitter, when known. Used by
    /// `airhouse_managed` to mint a credential at the user's actual role
    /// (Owner/Admin → write-capable airhouse role, Member/Viewer → reader)
    /// instead of always defaulting to least-privilege Reader. `None` for
    /// background runs and CLI/system paths that have no per-user identity.
    pub effective_role: Option<entity::workspace_members::WorkspaceRole>,
    /// Metric collection context for tracking usage data
    /// Flows through nested agent/automation executions via tokio::spawn
    pub metric_context: Option<SharedMetricCtx>,
    /// Data app file path (for tools that need to read/write data apps) - set by tools/create_data_app and tools/edit_data_app
    pub data_app_file_path: Option<String>,
}

impl ExecutionContext {
    pub fn new(
        source: Source,
        renderer: Renderer,
        workspace: WorkspaceManager<WorkingCopy>,
        writer: Sender<Event>,
        user_id: Option<uuid::Uuid>,
    ) -> Self {
        ExecutionContext {
            source,
            writer,
            renderer,
            workspace,
            filters: None,
            connections: None,
            sandbox_info: None,
            user_id,
            effective_role: None,
            metric_context: None,
            data_app_file_path: None,
        }
    }

    pub fn with_child_source(&self, source_id: String, kind: String) -> Self {
        ExecutionContext {
            source: Source {
                parent_id: Some(self.source.id.clone()),
                id: source_id,
                kind,
            },
            writer: self.writer.clone(),
            renderer: self.renderer.clone(),
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id: self.user_id,
            effective_role: self.effective_role.clone(),
            metric_context: self.metric_context.clone(),
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    pub fn wrap_writer(&self, writer: Sender<Event>) -> ExecutionContext {
        ExecutionContext {
            source: self.source.clone(),
            writer,
            renderer: self.renderer.clone(),
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id: self.user_id,
            effective_role: self.effective_role.clone(),
            metric_context: self.metric_context.clone(),
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    pub fn wrap_renderer(&self, renderer: Renderer) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer,
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id: self.user_id,
            effective_role: self.effective_role.clone(),
            metric_context: self.metric_context.clone(),
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    pub fn wrap_global_context(&self, global_context: Value) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer: self
                .renderer
                .switch_context(global_context, Value::UNDEFINED),
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id: self.user_id,
            effective_role: self.effective_role.clone(),
            metric_context: self.metric_context.clone(),
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    pub fn wrap_render_context(&self, context: &Value) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer: self.renderer.wrap(context),
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id: self.user_id,
            effective_role: self.effective_role.clone(),
            metric_context: self.metric_context.clone(),
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    pub fn with_user_id(&self, user_id: Option<uuid::Uuid>) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer: self.renderer.clone(),
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id,
            effective_role: self.effective_role.clone(),
            metric_context: self.metric_context.clone(),
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    /// Override the effective workspace role for this run. Used when an
    /// authenticated request enters from a handler that has resolved
    /// `EffectiveWorkspaceRole`; the value flows from there into
    /// `Connector::from_db` so airhouse_managed credentials get minted at
    /// the right airhouse role rather than always defaulting to Reader.
    pub fn with_effective_role(
        &self,
        effective_role: Option<entity::workspace_members::WorkspaceRole>,
    ) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer: self.renderer.clone(),
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id: self.user_id,
            effective_role,
            metric_context: self.metric_context.clone(),
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    /// Set the metric context for this execution
    pub fn with_metric_context(&self, metric_context: SharedMetricCtx) -> Self {
        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer: self.renderer.clone(),
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id: self.user_id,
            effective_role: self.effective_role.clone(),
            metric_context: Some(metric_context),
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    /// Create a child metric context for nested execution
    ///
    /// This creates a new MetricContext linked to the parent's trace_id.
    /// Use this when spawning nested agents/automations.
    pub fn with_child_metric_context(&self, source_type: SourceType, source_ref: &str) -> Self {
        let child_ctx = if let Some(parent_ctx) = &self.metric_context {
            if let Ok(guard) = parent_ctx.read() {
                Some(guard.child(source_type, source_ref).shared())
            } else {
                Some(MetricContext::new(source_type, source_ref).shared())
            }
        } else {
            Some(MetricContext::new(source_type, source_ref).shared())
        };

        ExecutionContext {
            source: self.source.clone(),
            writer: self.writer.clone(),
            renderer: self.renderer.clone(),
            workspace: self.workspace.clone(),
            filters: self.filters.clone(),
            connections: self.connections.clone(),
            sandbox_info: self.sandbox_info.clone(),
            user_id: self.user_id,
            effective_role: self.effective_role.clone(),
            metric_context: child_ctx,
            data_app_file_path: self.data_app_file_path.clone(),
        }
    }

    // Metric recording helpers

    /// Record a SQL query in the metric context
    pub fn record_sql(&self, sql: &str) {
        if let Some(ctx) = &self.metric_context
            && let Ok(mut guard) = ctx.write()
        {
            guard.add_sql(sql);
        }
    }

    /// Record explicit metrics from semantic query parameters
    pub fn record_explicit_metrics(
        &self,
        measures: &[String],
        dimensions: &[String],
        topic: Option<&str>,
    ) {
        if let Some(ctx) = &self.metric_context
            && let Ok(mut guard) = ctx.write()
        {
            guard.add_explicit_metrics(measures, dimensions, topic);
        }
    }

    /// Set the question/prompt in the metric context
    pub fn record_question(&self, question: &str) {
        if let Some(ctx) = &self.metric_context
            && let Ok(mut guard) = ctx.write()
        {
            guard.set_question(question);
        }
    }

    /// Set the response in the metric context
    pub fn record_response(&self, response: &str) {
        if let Some(ctx) = &self.metric_context
            && let Ok(mut guard) = ctx.write()
        {
            guard.set_response(response);
        }
    }

    /// Finalize and store the metric context
    ///
    /// This consumes the metric context from this execution context
    /// and triggers async storage of collected metrics.
    pub fn finalize_metrics(&self) {
        if let Some(ctx) = &self.metric_context
            && let Ok(guard) = ctx.read()
        {
            let context = guard.clone();
            drop(guard);
            context.finalize();
        }
    }

    pub async fn write_kind(&self, event: EventKind) -> Result<(), OxyError> {
        self.write(Event {
            source: self.source.clone(),
            kind: event,
        })
        .await
    }

    pub async fn write_chunk(&self, chunk: Chunk) -> Result<(), OxyError> {
        self.write_kind(EventKind::Updated { chunk }).await
    }

    pub async fn write_step_started(&self, step: Step) -> Result<(), OxyError> {
        self.write_kind(EventKind::StepStarted { step }).await
    }

    pub async fn write_step_finished(
        &self,
        step_id: String,
        error: Option<String>,
    ) -> Result<(), OxyError> {
        self.write_kind(EventKind::StepFinished { step_id, error })
            .await
    }

    pub async fn write_create_sandbox_app(
        &self,
        kind: SandboxAppKind,
        preview_url: String,
    ) -> Result<(), OxyError> {
        self.write_kind(EventKind::SandboxAppCreated {
            kind: kind.clone(),
            preview_url: preview_url.clone(),
        })
        .await
    }

    pub async fn write_data_app(
        &self,
        data_app: crate::exec_types::event::DataApp,
    ) -> Result<(), OxyError> {
        self.write_kind(EventKind::DataAppCreated { data_app })
            .await
    }

    pub async fn write_usage(&self, usage: Usage) -> Result<(), OxyError> {
        self.write_kind(EventKind::Usage { usage }).await
    }

    pub async fn write_progress(&self, progress: ProgressType) -> Result<(), OxyError> {
        self.write_kind(EventKind::Progress { progress }).await
    }
}

#[async_trait::async_trait]
impl Writer for ExecutionContext {
    async fn write(&self, event: Event) -> Result<(), OxyError> {
        self.writer
            .send(event)
            .await
            .map_err(|err| OxyError::RuntimeError(format!("Failed to send event:\n{err}")))
    }
}

pub struct ExecutionContextBuilder {
    source: Option<Source>,
    renderer: Option<Renderer>,
    workspace: Option<WorkspaceManager<WorkingCopy>>,
    writer: Option<Sender<Event>>,
    filters: Option<SessionFilters>,
    connections: Option<ConnectionOverrides>,
    sandbox_info: Option<SandboxInfo>,
    user_id: Option<uuid::Uuid>,
    effective_role: Option<entity::workspace_members::WorkspaceRole>,
    metric_context: Option<SharedMetricCtx>,
    data_app_file_path: Option<String>,
}

impl Default for ExecutionContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionContextBuilder {
    pub fn new() -> Self {
        ExecutionContextBuilder {
            source: None,
            renderer: None,
            workspace: None,
            writer: None,
            filters: None,
            connections: None,
            sandbox_info: None,
            user_id: None,
            effective_role: None,
            metric_context: None,
            data_app_file_path: None,
        }
    }

    pub fn with_global_context(mut self, global_context: Value) -> Self {
        self.renderer = Some(Renderer::new(global_context));
        self
    }

    pub fn with_workspace_manager(mut self, workspace: WorkspaceManager<WorkingCopy>) -> Self {
        self.workspace = Some(workspace);
        self
    }

    pub fn with_source(mut self, source: Source) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_writer(mut self, tx: Sender<Event>) -> Self {
        self.writer = Some(tx);
        self
    }

    pub fn with_filters(mut self, filters: impl Into<Option<SessionFilters>>) -> Self {
        self.filters = filters.into();
        self
    }

    pub fn with_connections(mut self, connections: impl Into<Option<ConnectionOverrides>>) -> Self {
        self.connections = connections.into();
        self
    }

    pub fn with_sandbox_info(mut self, sandbox_info: impl Into<Option<SandboxInfo>>) -> Self {
        self.sandbox_info = sandbox_info.into();
        self
    }

    pub fn with_user_id(mut self, user_id: Option<uuid::Uuid>) -> Self {
        self.user_id = user_id;
        self
    }

    /// Effective workspace role for the run's submitter. Threaded into
    /// `Connector::from_db` so airhouse_managed mints carry the right
    /// airhouse role rather than the conservative Reader default.
    pub fn with_effective_role(
        mut self,
        effective_role: Option<entity::workspace_members::WorkspaceRole>,
    ) -> Self {
        self.effective_role = effective_role;
        self
    }

    pub fn with_metric_context(mut self, metric_context: SharedMetricCtx) -> Self {
        self.metric_context = Some(metric_context);
        self
    }

    pub fn with_data_app_file_path(
        mut self,
        data_app_file_path: impl Into<Option<String>>,
    ) -> Self {
        self.data_app_file_path = data_app_file_path.into();
        self
    }

    pub fn build(self) -> Result<ExecutionContext, OxyError> {
        let source = self
            .source
            .ok_or(OxyError::RuntimeError("Source is required".to_string()))?;
        let writer = self
            .writer
            .ok_or(OxyError::RuntimeError("Writer is required".to_string()))?;
        let renderer = self
            .renderer
            .ok_or(OxyError::RuntimeError("Renderer is required".to_string()))?;
        let workspace: WorkspaceManager<WorkingCopy> = self.workspace.ok_or(
            OxyError::RuntimeError("WorkspaceManager is required".to_string()),
        )?;

        Ok(ExecutionContext {
            source,
            writer,
            renderer,
            workspace,
            filters: self.filters,
            connections: self.connections,
            sandbox_info: self.sandbox_info,
            user_id: self.user_id,
            effective_role: self.effective_role.clone(),
            metric_context: self.metric_context,
            data_app_file_path: self.data_app_file_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_execution_context_builder_with_filters() {
        let mut filters = HashMap::new();
        filters.insert("org_id".to_string(), json!(789));
        filters.insert("account_id".to_string(), json!(123));

        let builder = ExecutionContextBuilder::new().with_filters(Some(filters.clone().into()));

        assert!(builder.filters.is_some());
        let builder_filters = builder.filters.unwrap();
        assert_eq!(builder_filters.get("org_id"), Some(&json!(789)));
        assert_eq!(builder_filters.get("account_id"), Some(&json!(123)));
    }

    #[test]
    fn test_execution_context_builder_without_filters() {
        let builder = ExecutionContextBuilder::new();
        assert!(builder.filters.is_none());
    }

    #[test]
    fn test_execution_context_builder_filters_none() {
        let builder = ExecutionContextBuilder::new().with_filters(None);
        assert!(builder.filters.is_none());
    }
}
