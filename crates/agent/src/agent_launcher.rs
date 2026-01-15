use tracing::Instrument;

use crate::{
    agent::builder::AgentExecutable,
    fsm::{config::AgenticInput, machine::launch_agentic_workflow},
    types::AgentInput,
};

use oxy::{
    adapters::{
        project::manager::ProjectManager, secrets::SecretsManager, session_filters::SessionFilters,
    },
    config::{ConfigManager, constants::AGENT_SOURCE, model::ConnectionOverrides},
    execute::{
        Executable, ExecutionContext, ExecutionContextBuilder,
        types::{EventKind, OutputContainer, Source, event::SandboxInfo},
        writer::{BufWriter, EventHandler},
    },
    observability::events,
    semantic::SemanticManager,
};
use oxy_shared::errors::OxyError;

use minijinja::{Value, context};

pub struct AgentLauncher {
    execution_context: Option<ExecutionContext>,
    buf_writer: BufWriter,
    filters: Option<SessionFilters>,
    connections: Option<ConnectionOverrides>,
    globals: Option<indexmap::IndexMap<String, serde_json::Value>>,
    /// A2A task ID for tracking (optional, only used in A2A context)
    a2a_task_id: Option<String>,
    /// A2A thread ID for conversation continuity (optional, only used in A2A context)
    a2a_thread_id: Option<String>,
    /// A2A context ID for grouping related tasks (optional, only used in A2A context)
    a2a_context_id: Option<String>,
    /// Sandbox information from thread (e.g., v0 chat_id and preview_url)
    sandbox_info: Option<SandboxInfo>,
}

impl Default for AgentLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentLauncher {
    pub fn new() -> Self {
        Self {
            execution_context: None,
            buf_writer: BufWriter::new(),
            filters: None,
            connections: None,
            globals: None,
            a2a_task_id: None,
            a2a_thread_id: None,
            a2a_context_id: None,
            sandbox_info: None,
        }
    }

    pub fn with_filters(mut self, filters: impl Into<Option<SessionFilters>>) -> Self {
        self.filters = filters.into();
        self
    }

    pub fn with_connections(mut self, connections: impl Into<Option<ConnectionOverrides>>) -> Self {
        self.connections = connections.into();
        self
    }

    pub fn with_globals(
        mut self,
        globals: impl Into<Option<indexmap::IndexMap<String, serde_json::Value>>>,
    ) -> Self {
        self.globals = globals.into();
        self
    }

    /// Set the A2A task ID for tracking (used in A2A execution context)
    pub fn with_a2a_task_id(mut self, task_id: impl Into<Option<String>>) -> Self {
        self.a2a_task_id = task_id.into();
        self
    }

    /// Set the A2A thread ID for conversation continuity (used in A2A execution context)
    pub fn with_a2a_thread_id(mut self, thread_id: impl Into<Option<String>>) -> Self {
        self.a2a_thread_id = thread_id.into();
        self
    }

    /// Set the A2A context ID for grouping related tasks (used in A2A execution context)
    pub fn with_a2a_context_id(mut self, context_id: impl Into<Option<String>>) -> Self {
        self.a2a_context_id = context_id.into();
        self
    }

    /// Set the sandbox information from thread (e.g., v0 chat_id and preview_url)
    pub fn with_sandbox_info(mut self, sandbox_info: impl Into<Option<SandboxInfo>>) -> Self {
        self.sandbox_info = sandbox_info.into();
        self
    }

    #[tracing::instrument(skip_all, err, fields(
        otel.name = events::agent::get_global_context::NAME,
        oxy.span_type = events::agent::get_global_context::TYPE,
    ))]
    async fn get_global_context(
        &self,
        config: ConfigManager,
        secrets_manager: SecretsManager,
    ) -> Result<Value, OxyError> {
        events::agent::get_global_context::input(config.get_config());
        let mut semantic_manager =
            SemanticManager::from_config(config, secrets_manager, false).await?;

        // Apply global overrides to the GlobalRegistry before loading semantics
        if let Some(globals) = &self.globals {
            semantic_manager.set_global_overrides(globals.clone())?;
        }

        let semantic_variables_contexts =
            semantic_manager.get_semantic_variables_contexts().await?;

        let semantic_dimensions_contexts = semantic_manager
            .get_semantic_dimensions_contexts(&semantic_variables_contexts)
            .await?;

        // Get globals from the semantic manager
        let globals_value = semantic_manager.get_globals_value()?;

        // Convert serde_yaml::Value to minijinja::Value
        let globals = Value::from_serialize(&globals_value);

        let global_context = context! {
            models => Value::from_object(semantic_variables_contexts),
            dimensions => Value::from_object(semantic_dimensions_contexts),
            globals => globals,
        };

        events::agent::get_global_context::output(&global_context);

        Ok(global_context)
    }

    pub async fn with_external_context(
        mut self,
        execution_context: &ExecutionContext,
    ) -> Result<Self, OxyError> {
        let config_manager = execution_context.project.config_manager.clone();
        let secrets_manager = execution_context.project.secrets_manager.clone();
        let tx = self.buf_writer.create_writer(None)?;
        let global_context = self
            .get_global_context(config_manager, secrets_manager)
            .await?;
        self.execution_context = Some(
            execution_context
                .wrap_writer(tx)
                .wrap_global_context(global_context),
        );
        Ok(self)
    }

    #[tracing::instrument(skip_all, err, fields(
        otel.name = "agent.launcher.with_project",
        oxy.span_type = "agent",
    ))]
    pub async fn with_project(mut self, project_manager: ProjectManager) -> Result<Self, OxyError> {
        let tx = self.buf_writer.create_writer(None)?;

        let mut execution_context = ExecutionContextBuilder::new()
            .with_project_manager(project_manager)
            .with_global_context(Value::UNDEFINED)
            .with_writer(tx)
            .with_source(Source {
                parent_id: None,
                id: AGENT_SOURCE.to_string(),
                kind: AGENT_SOURCE.to_string(),
            })
            .with_filters(self.filters.clone())
            .with_connections(self.connections.clone())
            .with_sandbox_info(self.sandbox_info.clone())
            .build()?;

        let config_manager = execution_context.project.config_manager.clone();
        let secrets_manager = execution_context.project.secrets_manager.clone();

        let global_context = self
            .get_global_context(config_manager, secrets_manager)
            .await?;
        execution_context = execution_context.wrap_global_context(global_context);
        self.execution_context = Some(execution_context);
        Ok(self)
    }

    #[tracing::instrument(skip_all, err, fields(
        otel.name = "agent.launcher.launch",
        oxy.span_type = "agent",
        oxy.agent.ref = %agent_input.agent_ref,
    ))]
    pub async fn launch<H: EventHandler + Send + 'static>(
        self,
        mut agent_input: AgentInput,
        event_handler: H,
    ) -> Result<OutputContainer, OxyError> {
        // Pass A2A context from launcher to agent input if not already set
        if agent_input.a2a_task_id.is_none() {
            agent_input.a2a_task_id = self.a2a_task_id.clone();
        }
        if agent_input.a2a_thread_id.is_none() {
            agent_input.a2a_thread_id = self.a2a_thread_id.clone();
        }
        if agent_input.a2a_context_id.is_none() {
            agent_input.a2a_context_id = self.a2a_context_id.clone();
        }

        let execution_context = self
            .execution_context
            .ok_or(OxyError::RuntimeError(
                "ExecutionContext is required".to_string(),
            ))?
            .with_child_source(agent_input.agent_ref.to_string(), AGENT_SOURCE.to_string());

        // Capture the current span to propagate trace context to the spawned task
        let current_span = tracing::Span::current();

        let handle = tokio::spawn(
            async move {
                execution_context
                    .write_kind(EventKind::Started {
                        name: agent_input.agent_ref.to_string(),
                        attributes: Default::default(),
                    })
                    .await?;

                let response = AgentExecutable
                    .execute(&execution_context, agent_input)
                    .await;

                execution_context
                    .write_kind(EventKind::Finished {
                        attributes: Default::default(),
                        message: Default::default(),
                        error: response.as_ref().err().map(|e| e.to_string()),
                    })
                    .await?;
                response
            }
            .instrument(current_span),
        );

        let buf_writer = self.buf_writer;
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let response = handle.await?;
        event_handle.await??;
        response
    }

    #[tracing::instrument(skip_all, err, fields(
        otel.name = "agent.launcher.launch_agentic_workflow",
        oxy.span_type = "agent",
        oxy.agent.ref = %agent_ref,
    ))]
    pub async fn launch_agentic_workflow<H: EventHandler + Send + 'static>(
        self,
        agent_ref: &str,
        agent_input: AgenticInput,
        event_handler: H,
        run_id: Option<String>,
    ) -> Result<OutputContainer, OxyError> {
        let execution_context = self
            .execution_context
            .ok_or(OxyError::RuntimeError(
                "ExecutionContext is required".to_string(),
            ))?
            .with_child_source(agent_ref.to_string(), AGENT_SOURCE.to_string());
        let agent_ref = agent_ref.to_string();
        let agent_config = execution_context
            .project
            .config_manager
            .resolve_agentic_workflow(&agent_ref)
            .await?;

        let handle = tokio::spawn(async move {
            execution_context
                .write_kind(EventKind::AgenticStarted {
                    agent_id: agent_ref.to_string(),
                    run_id: run_id.clone().unwrap_or_default(),
                    agent_config: serde_json::to_value(&agent_config)?,
                })
                .await?;
            let response =
                launch_agentic_workflow(&execution_context, &agent_ref, agent_input).await;
            execution_context
                .write_kind(EventKind::AgenticFinished {
                    agent_id: agent_ref.to_string(),
                    run_id: run_id.clone().unwrap_or_default(),
                    error: response.as_ref().err().map(|e| e.to_string()),
                })
                .await?;
            response
        });

        let buf_writer = self.buf_writer;
        let event_handle =
            tokio::spawn(async move { buf_writer.write_to_handler(event_handler).await });
        let response = handle.await?;
        event_handle.await??;
        response
    }
}

#[derive(Clone, Debug)]
pub struct AgentLauncherExecutable;

#[async_trait::async_trait]
impl Executable<AgentInput> for AgentLauncherExecutable {
    type Response = OutputContainer;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: AgentInput,
    ) -> Result<Self::Response, OxyError> {
        AgentLauncher::new()
            .with_external_context(execution_context)
            .await?
            .launch(input, execution_context.writer.clone())
            .await
    }
}
