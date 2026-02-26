use std::path::PathBuf;

use async_openai::types::chat::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionToolChoiceOption, ToolChoiceOptions,
};
use futures::StreamExt;

use crate::fsm::{
    config::{
        AgenticConfig, ErrorTriage, RevisePlan, Transition, TransitionMode, TransitionObjective,
        TriggerType,
    },
    control::{
        Idle, Plan, Synthesize, TriggerBuilder,
        config::{EndMode, OutputArtifact, StartMode},
        ensure_ends_with_user_message,
    },
    data_app::{BuildDataApp, GenerateInsight, config::Insight},
    machine::Agent,
    query::{AutoSQL, config::Query},
    trigger::StepTrigger,
    types::{Artifact, Message, ToolReq, ToolRes},
};
use oxy::adapters::{openai::OpenAIAdapter, project::manager::ProjectManager};
use oxy::config::constants::{
    AGENT_CONTINUE_PLAN_TRANSITION, AGENT_END_TRANSITION, AGENT_FIX_ERROR_TRANSITION,
    AGENT_REVISE_PLAN_TRANSITION,
};
use oxy::execute::{
    ExecutionContext,
    builders::fsm::{State, Trigger},
    types::{
        Chunk, Output, OutputContainer, Table, VizParams,
        event::{Step, StepKind},
    },
};
use oxy_shared::errors::OxyError;

#[derive(Debug)]
pub struct MachineContext {
    state_dir: PathBuf,
    context_state_path: String,
    messages: Vec<Message>,
    artifacts: Vec<Artifact>,
    max_iterations: usize,
    iteration: usize,
    transition_name: String,
    user_query: String,
    plan: Option<String>,
    synthesized_output: Option<String>,
}

impl MachineContext {
    pub async fn from_conversation(
        project: ProjectManager,
        context_id: String,
        user_query: String,
        history: Vec<ChatCompletionRequestMessage>,
        transition_name: String,
        max_iterations: usize,
    ) -> Result<Self, OxyError> {
        Ok(Self {
            state_dir: project.config_manager.resolve_state_dir().await?,
            context_state_path: format!("contexts/{}", context_id),
            messages: history.into_iter().map(|m| m.into()).collect(),
            artifacts: vec![],
            max_iterations,
            iteration: 0,
            transition_name,
            user_query,
            plan: None,
            synthesized_output: None,
        })
    }

    pub fn transition_name(&self) -> &str {
        &self.transition_name
    }

    pub fn set_transition_name(&mut self, name: &str) {
        self.transition_name = name.to_string();
    }

    pub fn user_query(&self) -> &str {
        &self.user_query
    }

    pub fn set_plan(&mut self, plan: Option<String>) {
        self.plan = plan;
    }

    pub fn get_plan(&self) -> &Option<String> {
        &self.plan
    }

    pub fn set_content(&mut self, content: Option<String>) {
        self.synthesized_output = content;
    }

    pub fn increase_iteration(&mut self) {
        self.iteration += 1;
    }

    pub fn max_iterations_reached(&self) -> bool {
        self.iteration >= self.max_iterations
    }

    pub fn list_messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn list_artifacts(&self) -> &[Artifact] {
        &self.artifacts
    }

    pub fn add_artifact(&mut self, tool_req: ToolReq, artifact: Artifact) {
        self.messages.push(Message::ToolReq(tool_req.clone()));
        self.messages.push(Message::ToolRes(ToolRes::new(
            tool_req.call_id().to_string(),
            artifact.describe(),
        )));
        self.messages.push(Message::Assistant {
            content: format!(
                "Generated artifact: {}. You can use this to support your next steps.",
                match &artifact {
                    Artifact::Viz { viz_name, .. } => viz_name,
                    Artifact::Table { table_name, .. } => table_name,
                    Artifact::Insight { .. } => "insight",
                    Artifact::DataApp { app_name, .. } => app_name,
                }
            ),
        });
        self.artifacts.push(artifact);
    }

    pub fn add_viz(
        &mut self,
        objective: String,
        tool_req: ToolReq,
        viz_params: VizParams,
    ) -> Result<VizParams, OxyError> {
        let viz_artifact = Artifact::Viz {
            viz_name: viz_params.name.to_string(),
            description: format!("Successful visualize for objective: {}", objective),
            params: viz_params.clone(),
        };
        self.add_artifact(tool_req, viz_artifact);
        let file_path = self.save_viz_table(&viz_params)?;
        Ok(viz_params.with_data_path(&file_path))
    }

    pub fn add_table(&mut self, _objective: String, tool_req: ToolReq, table: Table) {
        let table_artifact = Artifact::Table {
            table_name: table.name.clone(),
            description: table.summary(),
            table,
        };
        self.add_artifact(tool_req, table_artifact);
    }

    pub fn add_insight(&mut self, content: String) {
        let insight_artifact = Artifact::Insight {
            content: content.clone(),
        };
        self.messages.push(Message::Thinking {
            content: format!("Generated insight: {}", content),
        });
        self.artifacts.push(insight_artifact);
    }

    pub fn add_message(&mut self, content: String) {
        self.messages.push(Message::Assistant { content });
    }

    pub fn think(&mut self, content: &str) {
        self.messages.push(Message::Thinking {
            content: content.to_string(),
        });
    }

    pub fn plan(&mut self, content: &str) {
        self.messages.push(Message::Planning {
            content: content.to_string(),
        });
    }

    pub fn artifacts_context(&self) -> String {
        self.artifacts
            .iter()
            .map(|a| a.describe())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn list_tables(&self) -> Vec<&Table> {
        self.artifacts
            .iter()
            .filter_map(|a| {
                if let Artifact::Table { table, .. } = a {
                    Some(table)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    pub fn list_viz(&self) -> Vec<&VizParams> {
        self.artifacts
            .iter()
            .filter_map(|a| {
                if let Artifact::Viz { params, .. } = a {
                    Some(params)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    pub fn list_insights(&self) -> Vec<&String> {
        self.artifacts
            .iter()
            .filter_map(|a| {
                if let Artifact::Insight { content } = a {
                    Some(content)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    fn get_content(&self) -> &str {
        self.synthesized_output.as_deref().unwrap_or("")
    }

    fn save_viz_table(&self, params: &VizParams) -> Result<String, OxyError> {
        let table_name = params.data_slug();
        let table = self
            .list_tables()
            .into_iter()
            .find(|t| t.slug() == table_name)
            .ok_or(OxyError::RuntimeError(format!(
                "Table {} not found for viz artifact.",
                table_name
            )))?;
        let file_path = PathBuf::from(&self.context_state_path)
            .join("tables")
            .join(format!("{}.parquet", table.name));
        table.save_data(self.state_dir.join(&file_path))?;
        Ok(file_path.to_string_lossy().to_string())
    }

    fn is_transition_available(&self, transition: &Transition) -> bool {
        match &transition.trigger {
            TriggerType::Visualize(_) => !self.list_tables().is_empty(),
            // Insight can only run once per workflow to prevent infinite re-analysis loops.
            // Once an insight exists, Claude must act on it (visualize, query, or end) rather
            // than generating another identical insight.
            TriggerType::Insight(_) => {
                !self.list_tables().is_empty() && self.list_insights().is_empty()
            }
            _ => true,
        }
    }

    async fn select_transition(
        &self,
        execution_context: &ExecutionContext,
        items: &[String],
        messages: Vec<ChatCompletionRequestMessage>,
        machine: &Agent<MachineContext>,
    ) -> Result<(Transition, TransitionObjective), OxyError> {
        let tools = machine
            .config
            .list_transitions(items)?
            .iter()
            .filter(|t| self.is_transition_available(t))
            .map(|t| t.get_tool())
            .collect::<Vec<_>>();
        if tools.is_empty() {
            return Err(OxyError::RuntimeError(
                "No available transitions for the current context.".to_string(),
            ));
        }
        tracing::info!(
            "Requesting transition selection with context: {:?}",
            messages
        );
        let artifacts_summary = if self.artifacts.is_empty() {
            "No artifacts created yet.".to_string()
        } else {
            format!(
                "## Available Artifacts (Already Created)\n{}\n\n**Important**: Do NOT recreate existing artifacts. Reference them by name if needed.",
                self.artifacts_context()
            )
        };

        let mut messages = [vec![ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(format!(
                    "You are the workflow orchestrator. Select the next action based on current context and progress.

{}

## Decision Process

1. **Review Current State**
   - Check existing artifacts above - do NOT duplicate them
   - What is the next logical step to achieve the goal?
   - Are there any unresolved errors or issues?

2. **Evaluate Available Actions**
   - Which action directly addresses the next needed step?
   - Does the action have required dependencies? (e.g., visualize needs query data first)
   - Is this action available given current artifacts?
   - Skip actions that would recreate existing artifacts

3. **Consider Special Cases**
   - If troubleshooting info is present: prioritize actions that address the identified issues
   - If no data exists yet: start with query action
   - If data exists but no visualization: consider visualize action
   - If goal is achieved: select end action

4. **Write Clear Objective**
   - Be specific about what needs to be done (not vague like 'analyze data')
   - Include relevant context (table names, columns, conditions)
   - If addressing an error, reference the fix from troubleshooting
   - Reference existing artifacts by name if building on previous work

Select the action that makes the most progress toward the goal while respecting dependencies and avoiding duplication.",
                    artifacts_summary
                )),
                ..Default::default()
            }
            .into(),],
            messages].concat();
        // Claude requires the last message to be a user message (no assistant prefill).
        ensure_ends_with_user_message(&mut messages, "What is the next action to take?");
        let (_response, tool_calls) = machine
            .adapter
            .request_tool_call_with_usage(
                execution_context,
                messages,
                tools,
                Some(ChatCompletionToolChoiceOption::Mode(
                    ToolChoiceOptions::Required,
                )),
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
        machine
            .config
            .find_transition(transition_name)
            .map(|t| (t, transition_objective))
    }

    async fn troubleshoot_error(
        &mut self,
        execution_context: &ExecutionContext,
        error: &OxyError,
        machine: &Agent<MachineContext>,
    ) -> Result<(), OxyError> {
        let mut messages = vec![
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(format!(
                    "Troubleshoot this workflow error: {}

Available artifacts: {}

Provide a concise analysis in this format:
1. ERROR: What failed and why. If there is a data issue, specify the problematic data. Pay attention to the columns, types, and values especially temporal types.
2. FIX: Specific action to resolve it (e.g., adjust parameters, retry with different approach). Consider which rules to follow to avoid repeating the same error.

Keep response under 200 words.",
                    error,
                    self.artifacts_context()
                )),
                ..Default::default()
            }
            .into(),
        ];
        // Claude requires the conversation to end with a user message.
        ensure_ends_with_user_message(&mut messages, "Please troubleshoot this error.");
        let mut stream = machine.adapter.stream_text(messages).await?;
        let mut content = String::new();
        let streaming_context = execution_context
            .with_child_source(uuid::Uuid::new_v4().to_string(), "text".to_string());
        while let Some(chunk) = stream.next().await.transpose()?.flatten() {
            content.push_str(&chunk);
            streaming_context
                .write_chunk(Chunk {
                    key: None,
                    delta: Output::Text(chunk),
                    finished: false,
                })
                .await?;
        }
        streaming_context
            .write_chunk(Chunk {
                key: None,
                delta: Output::Text("".to_string()),
                finished: true,
            })
            .await?;
        self.think(&content);
        Ok(())
    }

    pub async fn should_revise_plan(
        &mut self,
        execution_context: &ExecutionContext,
        messages: Vec<ChatCompletionRequestMessage>,
        machine: &Agent<MachineContext>,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError> {
        let mut messages = [
            vec![ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(
                    format!("Based on the previous execution, decide if you need to revise your plan.
                        If you do, select the '{AGENT_REVISE_PLAN_TRANSITION}' action,
                        otherwise select '{AGENT_CONTINUE_PLAN_TRANSITION}' to proceed with your current plan.
                        If you only change the next step of your plan, you can continue with '{AGENT_CONTINUE_PLAN_TRANSITION}'.
                        If there is Error troubleshooting information available, select the {AGENT_FIX_ERROR_TRANSITION} action to continue executing the plan to addressing the error.")
                ),
                ..Default::default()
            }
            .into()],
            messages,
        ]
        .concat();
        // Claude requires the last message to be a user message (no assistant prefill).
        ensure_ends_with_user_message(&mut messages, "Should the plan be revised?");
        let (_response, tool_calls) = machine
            .adapter
            .request_tool_call_with_usage(
                execution_context,
                messages,
                machine.config.start.start.get_tools(),
                Some(ChatCompletionToolChoiceOption::Mode(
                    ToolChoiceOptions::Required,
                )),
                None,
            )
            .await?;

        let tool_call = tool_calls.first().ok_or(OxyError::RuntimeError(
            "No tool calls returned from the model".to_string(),
        ))?;
        match tool_call.function.name.as_str() {
            AGENT_CONTINUE_PLAN_TRANSITION => Ok(Box::new(Idle::<Self>::new())),
            AGENT_REVISE_PLAN_TRANSITION => {
                let revise_plan: RevisePlan = serde_json::from_str(&tool_call.function.arguments)
                    .map_err(|e| {
                    OxyError::SerializerError(format!(
                        "Failed to parse transition objective from arguments: {e}"
                    ))
                })?;
                self.plan(&revise_plan.revision);
                Ok(Box::new(Idle::<Self>::new()))
            }
            AGENT_FIX_ERROR_TRANSITION => {
                let error_triage: ErrorTriage = serde_json::from_str(&tool_call.function.arguments)
                    .map_err(|e| {
                        OxyError::SerializerError(format!(
                            "Failed to parse error triage from arguments: {e}"
                        ))
                    })?;
                Ok(Box::new(Plan::<Self>::new(
                    machine.adapter.clone(),
                    format!(
                        "Fix your plan based on the latest troubleshooting information. Error triage: {}",
                        error_triage.error_triage
                    ),
                    "".to_string(),
                    machine.config.list_transitions(&[
                        self.transition_name().to_string(),
                        AGENT_END_TRANSITION.to_string(),
                    ])?,
                )))
            }
            _ => Err(OxyError::RuntimeError(format!(
                "Unexpected tool call name: {}",
                tool_call.function.name
            ))),
        }
    }
}

#[async_trait::async_trait]
impl TriggerBuilder for MachineContext {
    async fn build_query_trigger(
        &self,
        execution_context: &ExecutionContext,
        agentic_config: &AgenticConfig,
        query_config: &Query,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError> {
        let model_ref = query_config
            .model
            .as_deref()
            .unwrap_or(&agentic_config.model);
        let openai_adapter =
            OpenAIAdapter::from_config(execution_context.project.clone(), model_ref).await?;
        Ok(Box::new(AutoSQL::<MachineContext>::new(
            openai_adapter,
            query_config.clone(),
            objective,
        )))
    }

    async fn build_viz_trigger(
        &self,
        execution_context: &ExecutionContext,
        agentic_config: &AgenticConfig,
        viz_config: &crate::fsm::viz_config::Visualize,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError> {
        let model_ref = viz_config.model.as_deref().unwrap_or(&agentic_config.model);
        let openai_adapter =
            OpenAIAdapter::from_config(execution_context.project.clone(), model_ref).await?;
        Ok(Box::new(
            crate::fsm::viz::GenerateViz::<MachineContext>::new(
                objective,
                openai_adapter,
                viz_config.clone(),
            ),
        ))
    }

    async fn build_insight_trigger(
        &self,
        execution_context: &ExecutionContext,
        agentic_config: &AgenticConfig,
        insight_config: &Insight,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError> {
        let model_ref = insight_config
            .model
            .as_deref()
            .unwrap_or(&agentic_config.model);
        let openai_adapter =
            OpenAIAdapter::from_config(execution_context.project.clone(), model_ref).await?;
        Ok(Box::new(GenerateInsight::<MachineContext>::new(
            openai_adapter,
            objective,
            insight_config.clone(),
        )))
    }

    async fn build_data_app_trigger(
        &self,
        _execution_context: &ExecutionContext,
        _agentic_config: &AgenticConfig,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError>
    where
        Self: std::fmt::Debug,
    {
        Ok(Box::new(BuildDataApp::<MachineContext>::new(objective)))
    }

    async fn build(
        &self,
        execution_context: &ExecutionContext,
        agentic_config: &AgenticConfig,
        transition: Transition,
        objective: String,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError> {
        tracing::info!(
            "Building trigger '{}' of type {:?}",
            transition.trigger.get_name(),
            transition.trigger
        );
        match &transition.trigger {
            TriggerType::Start(start_config) => match &start_config.mode {
                StartMode::Default => Ok(Box::new(Idle::<Self>::new())),
                StartMode::Plan {
                    model,
                    instruction,
                    example,
                } => {
                    let openai_adapter = OpenAIAdapter::from_config(
                        execution_context.project.clone(),
                        model.as_deref().unwrap_or(&agentic_config.model),
                    )
                    .await?;
                    let transitions = [
                        transition.get_transition_names(),
                        vec![AGENT_END_TRANSITION.to_string()],
                    ]
                    .concat();

                    Ok(Box::new(Plan::<Self>::new(
                        openai_adapter,
                        instruction.to_string(),
                        example.to_string(),
                        agentic_config.list_transitions(&transitions)?,
                    )))
                }
            },
            TriggerType::End(end_config) => {
                let finalizer = match end_config.output_artifact {
                    OutputArtifact::App => Some(
                        self.build_data_app_trigger(execution_context, agentic_config, objective)
                            .await?,
                    ),
                    _ => None,
                };
                match &end_config.mode {
                    EndMode::Default => match finalizer {
                        Some(finalizer) => Ok(finalizer),
                        None => Ok(Box::new(Idle::new())),
                    },
                    EndMode::Synthesize { model, instruction } => {
                        let model_ref = model.as_deref().unwrap_or(&agentic_config.model);
                        let openai_adapter = OpenAIAdapter::from_config(
                            execution_context.project.clone(),
                            model_ref,
                        )
                        .await?;
                        Ok(Box::new(Synthesize::<Self>::new(
                            openai_adapter,
                            instruction.to_string(),
                            finalizer,
                        )))
                    }
                }
            }
            TriggerType::Query(query_config) => {
                tracing::info!("Building Query Trigger {query_config:?}");
                self.build_query_trigger(execution_context, agentic_config, query_config, objective)
                    .await
            }
            TriggerType::Visualize(viz_config) => {
                self.build_viz_trigger(execution_context, agentic_config, viz_config, objective)
                    .await
            }
            TriggerType::Insight(insight_config) => {
                self.build_insight_trigger(
                    execution_context,
                    agentic_config,
                    insight_config,
                    objective,
                )
                .await
            }
            TriggerType::Subflow(subflow_config) => {
                self.build_subflow_trigger(
                    execution_context,
                    agentic_config,
                    subflow_config,
                    objective,
                )
                .await
            }
        }
    }
}

#[async_trait::async_trait]
impl State for MachineContext {
    type Machine = Agent<Self>;

    async fn first_trigger(
        &mut self,
        execution_context: &ExecutionContext,
        machine: &mut Self::Machine,
    ) -> Result<Box<dyn Trigger<State = Self>>, OxyError> {
        let start_transition = machine.config.start_transition();
        self.set_transition_name(start_transition.trigger.get_name());
        Ok(StepTrigger::boxed(
            Step::new(
                uuid::Uuid::new_v4().to_string(),
                start_transition.trigger.get_step_kind(),
                None,
            ),
            self.build(
                execution_context,
                &machine.config,
                start_transition,
                self.user_query().to_string(),
            )
            .await?,
        ))
    }

    async fn next_trigger(
        &mut self,
        execution_context: &ExecutionContext,
        machine: &mut Self::Machine,
    ) -> Result<Option<Box<dyn Trigger<State = Self>>>, OxyError> {
        // Check if max iterations reached
        if self.max_iterations_reached() {
            return Err(OxyError::RuntimeError(format!(
                "Max iterations of {} reached",
                machine.config.max_iterations
            )));
        }

        let current_transition = machine.config.find_transition(self.transition_name())?;
        if current_transition.trigger.is_end() {
            return Ok(None);
        }

        let mut objective = self.user_query().to_string();
        let mut get_transition = async || match &current_transition.next {
            TransitionMode::Always(next) => {
                let (transition, transition_objective) = self
                    .select_transition(
                        execution_context,
                        &[next.clone()],
                        self.list_messages()
                            .iter()
                            .map(|m| m.clone().into())
                            .collect::<Vec<_>>(),
                        machine,
                    )
                    .await?;
                objective = transition_objective.objective;
                Result::Ok::<Option<Transition>, OxyError>(Some(transition))
            }
            TransitionMode::Auto(items) => {
                if items.is_empty() {
                    return Ok(None);
                }

                let (transition, transition_objective) = self
                    .select_transition(
                        execution_context,
                        items,
                        self.list_messages()
                            .iter()
                            .map(|m| m.clone().into())
                            .collect::<Vec<_>>(),
                        machine,
                    )
                    .await?;
                objective = transition_objective.objective;
                Result::Ok::<Option<Transition>, OxyError>(Some(transition))
            }
            TransitionMode::Plan => match &self.get_plan().is_some() {
                true => Result::Ok::<Option<Transition>, OxyError>(Some(
                    machine.config.start_transition(),
                )),
                false => Result::Ok::<Option<Transition>, OxyError>(None),
            },
        };
        let transition = get_transition().await?;

        match transition {
            Some(t) => {
                self.think(&format!(
                    "Transitioning to '{}' agent to achieve objective: {}",
                    t.trigger.get_name(),
                    objective
                ));
                self.set_transition_name(t.trigger.get_name());

                // If the next transition is a start transition, we need to check if we should revise the plan
                if t.trigger.is_start() && self.get_plan().is_some() {
                    return self
                        .should_revise_plan(
                            execution_context,
                            self.list_messages()
                                .iter()
                                .map(|m| m.clone().into())
                                .collect::<Vec<_>>(),
                            machine,
                        )
                        .await
                        .map(Some);
                }

                self.increase_iteration();
                Ok(Some(StepTrigger::boxed(
                    Step::new(
                        uuid::Uuid::new_v4().to_string(),
                        t.trigger.get_step_kind(),
                        Some(objective.clone()),
                    ),
                    self.build(
                        execution_context,
                        &machine.config,
                        t.clone(),
                        objective.clone(),
                    )
                    .await?,
                )))
            }
            None => Ok(None),
        }
    }

    async fn handle_error(
        mut self,
        execution_context: &ExecutionContext,
        machine: &mut Self::Machine,
        error: OxyError,
    ) -> Result<Self, OxyError> {
        // Log the error in the context
        self.add_message(format!("Encountered an error: {}", error));

        // Provide error troubleshooting
        let step_id = uuid::Uuid::new_v4().to_string();
        let error_context =
            execution_context.with_child_source(step_id.clone(), "troubleshoot".to_string());
        let step = Step::new(step_id.clone(), StepKind::Troubleshoot, None);
        error_context.write_step_started(step).await?;
        let err = self
            .troubleshoot_error(&error_context, &error, machine)
            .await
            .err()
            .map(|e| e.to_string());
        error_context.write_step_finished(step_id, err).await?;
        Ok(self)
    }
}

impl From<MachineContext> for OutputContainer {
    fn from(ctx: MachineContext) -> Self {
        Output::Text(ctx.get_content().to_string()).into()
    }
}
