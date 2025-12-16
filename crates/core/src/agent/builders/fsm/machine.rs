use std::path::Path;

use minijinja::context;

use crate::{
    adapters::{openai::OpenAIAdapter, project::manager::ProjectManager},
    agent::{
        builders::fsm::{
            config::{AgenticConfig, AgenticInput},
            state::MachineContext,
        },
        databases::DatabasesContext,
    },
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::fsm::{FSM, Machine},
        renderer::Renderer,
        types::OutputContainer,
    },
    semantic::SemanticManager,
};

pub struct Agent<S> {
    pub config: AgenticConfig,
    pub adapter: OpenAIAdapter,
    _state: std::marker::PhantomData<S>,
}

impl<S> Agent<S> {
    pub async fn new(
        project_manager: ProjectManager,
        config: AgenticConfig,
    ) -> Result<Self, OxyError> {
        let adapter = OpenAIAdapter::from_config(project_manager, &config.model).await?;
        Ok(Self {
            config,
            adapter,
            _state: std::marker::PhantomData,
        })
    }

    pub async fn init(
        &self,
        execution_context: &ExecutionContext,
    ) -> Result<ExecutionContext, OxyError> {
        let global_context = build_global_context(execution_context).await?;
        let renderer = Renderer::from_template(global_context, &self.config).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create renderer from template: {e}"))
        })?;
        let execution_context = execution_context.wrap_renderer(renderer);
        Ok(execution_context)
    }
}

#[async_trait::async_trait]
impl Machine<AgenticInput> for Agent<MachineContext> {
    type State = MachineContext;

    async fn wrap_context(
        &self,
        execution_context: &ExecutionContext,
    ) -> Result<ExecutionContext, OxyError> {
        self.init(execution_context).await
    }

    async fn start(
        &mut self,
        execution_context: &ExecutionContext,
        input: AgenticInput,
    ) -> Result<Self::State, OxyError> {
        Ok(Self::State::from_conversation(
            execution_context.project.clone(),
            input.context_id.to_string(),
            input.prompt.to_string(),
            input.trace.into_iter().map(|m| m.into()).collect(),
            self.config.start.start.name.clone(),
        )
        .await?)
    }

    async fn end(
        &mut self,
        _execution_context: &ExecutionContext,
        final_state: Self::State,
    ) -> Result<Self::State, OxyError> {
        Ok(final_state)
    }
}

pub async fn launch_agentic_workflow<P: AsRef<Path>>(
    execution_context: &ExecutionContext,
    agent_ref: P,
    input: AgenticInput,
) -> Result<OutputContainer, OxyError> {
    let agentic_config = execution_context
        .project
        .config_manager
        .resolve_agentic_workflow(agent_ref)
        .await?;
    let machine =
        Agent::<MachineContext>::new(execution_context.project.clone(), agentic_config).await?;
    let output = FSM::new(machine).execute(execution_context, input).await?;
    Ok(output.into())
}

async fn build_global_context(
    execution_context: &ExecutionContext,
) -> Result<minijinja::Value, OxyError> {
    let config = execution_context.project.config_manager.clone();
    let secrets_manager = execution_context.project.secrets_manager.clone();
    let databases = DatabasesContext::new(config.clone(), secrets_manager.clone());
    let semantic_manager = SemanticManager::from_config(config, secrets_manager, false).await?;
    let semantic_contexts = semantic_manager.get_semantic_variables_contexts().await?;
    let semantic_dimensions_contexts = semantic_manager
        .get_semantic_dimensions_contexts(&semantic_contexts)
        .await?;

    // Get globals from the semantic manager
    let globals_value = semantic_manager.get_globals_value()?;

    // Convert serde_yaml::Value to minijinja::Value
    let globals = minijinja::Value::from_serialize(&globals_value);

    Ok(context! {
        databases => minijinja::Value::from_object(databases),
        models => minijinja::Value::from_object(semantic_contexts),
        dimensions => minijinja::Value::from_object(semantic_dimensions_contexts),
        globals => globals,
    })
}
