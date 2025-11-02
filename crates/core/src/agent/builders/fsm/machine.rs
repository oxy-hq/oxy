use std::path::Path;

use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, ChatCompletionToolChoiceOption,
};
use minijinja::context;

use crate::{
    adapters::{openai::OpenAIAdapter, project::manager::ProjectManager},
    agent::{
        builders::fsm::{
            config::{AgenticConfig, AgenticInput, Transition, TransitionObjective},
            control::{TransitionContext, config::OutputArtifact},
            states::{data_app::DataAppState, qa::QAState, query::QueryState},
        },
        databases::DatabasesContext,
    },
    config::constants::{AGENT_CONTINUE_PLAN_TRANSITION, AGENT_REVISE_PLAN_TRANSITION},
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

impl<S: TransitionContext + Send + Sync> Agent<S> {
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

    pub async fn select_transition(
        &self,
        execution_context: &ExecutionContext,
        items: &[String],
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<(Transition, String), OxyError> {
        let tools = self
            .config
            .list_transitions(items)?
            .iter()
            .map(|t| t.get_tool())
            .collect::<Vec<_>>();
        let tool_calls = self
            .adapter
            .request_tool_call_with_usage(
                execution_context,
                messages,
                tools,
                Some(ChatCompletionToolChoiceOption::Required),
                None,
            )
            .await?;
        let tool_call = tool_calls.first().ok_or(OxyError::RuntimeError(
            "No tool calls returned from the model".to_string(),
        ))?;
        let transition_name = &tool_call.function.name;
        let transition_objective: TransitionObjective =
            serde_json::from_str(&tool_call.function.arguments).map_err(|e| {
                OxyError::SerializerError(format!(
                    "Failed to parse transition objective from arguments: {e}"
                ))
            })?;
        self.config
            .find_transition(transition_name)
            .map(|t| (t, transition_objective.objective))
    }

    pub async fn should_revise_plan(
        &self,
        execution_context: &ExecutionContext,
        messages: Vec<ChatCompletionRequestMessage>,
    ) -> Result<bool, OxyError> {
        let tool_calls = self
            .adapter
            .request_tool_call_with_usage(
                execution_context,
                [messages,
                    vec![
                    ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(
                        format!("Based on the previous execution, decide if you need to revise your plan.
                        If you do, select the '{AGENT_REVISE_PLAN_TRANSITION}' action,
                        otherwise select '{AGENT_CONTINUE_PLAN_TRANSITION}' to proceed with your current plan.
                        If you only change the next step of your plan, you can continue with '{AGENT_CONTINUE_PLAN_TRANSITION}'.")
                    ),
                    ..Default::default()
                }
                .into(),
                ]]
                .concat(),
                self.config.start.start.get_tools(),
                Some(ChatCompletionToolChoiceOption::Required),
                None,
            )
            .await?;
        let tool_call = tool_calls.first().ok_or(OxyError::RuntimeError(
            "No tool calls returned from the model".to_string(),
        ))?;
        let should_revise = tool_call.function.name.as_str() == AGENT_REVISE_PLAN_TRANSITION;
        Ok(should_revise)
    }
}

#[async_trait::async_trait]
impl Machine<AgenticInput> for Agent<QueryState> {
    type State = QueryState;
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
        let instruction = execution_context
            .renderer
            .render_async(&self.config.instruction)
            .await?;
        Ok(Self::State::from_input(instruction, &input))
    }

    async fn end(
        &mut self,
        _execution_context: &ExecutionContext,
        final_state: Self::State,
    ) -> Result<Self::State, OxyError> {
        Ok(final_state)
    }
}

#[async_trait::async_trait]
impl Machine<AgenticInput> for Agent<QAState> {
    type State = QAState;
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
        let instruction = execution_context
            .renderer
            .render_async(&self.config.instruction)
            .await?;
        Ok(Self::State::from_input(instruction, &input))
    }

    async fn end(
        &mut self,
        _execution_context: &ExecutionContext,
        final_state: Self::State,
    ) -> Result<Self::State, OxyError> {
        Ok(final_state)
    }
}

#[async_trait::async_trait]
impl Machine<AgenticInput> for Agent<DataAppState> {
    type State = DataAppState;

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
        let instruction = execution_context
            .renderer
            .render_async(&self.config.instruction)
            .await?;
        Ok(Self::State::from_input(instruction, &input))
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
    match agentic_config.end.end.output_artifact {
        OutputArtifact::None => {
            let machine =
                Agent::<QAState>::new(execution_context.project.clone(), agentic_config).await?;
            let output = FSM::new(machine).execute(execution_context, input).await?;
            Ok(output.into())
        }
        OutputArtifact::Query => {
            let machine =
                Agent::<QueryState>::new(execution_context.project.clone(), agentic_config).await?;
            let output = FSM::new(machine).execute(execution_context, input).await?;
            Ok(output.into())
        }
        OutputArtifact::App => {
            let machine =
                Agent::<DataAppState>::new(execution_context.project.clone(), agentic_config)
                    .await?;
            let output = FSM::new(machine).execute(execution_context, input).await?;
            Ok(output.into())
        }
        _ => todo!(),
    }
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
    Ok(context! {
        databases => minijinja::Value::from_object(databases),
        models => minijinja::Value::from_object(semantic_contexts),
        dimensions => minijinja::Value::from_object(semantic_dimensions_contexts),
    })
}
