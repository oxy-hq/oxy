use crate::{
    agent::builders::fsm::{
        control::config::{End, EndMode, OutputArtifact, Start, StartMode},
        data_app::config::Insight,
        query::config::Query,
        subflow::config::Subflow,
        viz::config::Visualize,
    },
    config::constants::{AGENT_END_TRANSITION, AGENT_START_TRANSITION},
    errors::OxyError,
    execute::{renderer::TemplateRegister, types::event::StepKind},
};

use async_openai::types::chat::{ChatCompletionRequestMessage, ChatCompletionTool, FunctionObject};
use itertools::Itertools;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgenticConfig {
    #[serde(skip)]
    pub name: String,
    #[serde(default = "default_public")]
    pub public: bool,
    pub model: String,
    #[serde(default)]
    pub instruction: String,
    #[serde(default = "default_auto_transition_prompt")]
    pub auto_transition_prompt: String,
    pub start: StartConfig,
    pub end: EndConfig,
    pub transitions: Vec<Transition>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_public() -> bool {
    true
}

fn default_auto_transition_prompt() -> String {
    "Based on the conversation messages, select the next action to take from the list of available actions.".to_string()
}

fn default_max_iterations() -> usize {
    15
}

impl AgenticConfig {
    pub fn start_transition(&self) -> Transition {
        self.start.clone().into()
    }

    pub fn find_transition(&self, name: &str) -> Result<Transition, OxyError> {
        if name == AGENT_END_TRANSITION {
            return Ok(self.end.clone().into());
        }

        if name == AGENT_START_TRANSITION {
            return Ok(self.start.clone().into());
        }

        let transition = self
            .transitions
            .iter()
            .find(|t| t.trigger.get_name() == name)
            .cloned();
        transition.ok_or(OxyError::RuntimeError(format!(
            "Transition '{}' not found in the configuration",
            name
        )))
    }

    pub fn list_transitions(&self, names: &[String]) -> Result<Vec<Transition>, OxyError> {
        names
            .iter()
            .map(|name| self.find_transition(name))
            .try_collect::<Transition, Vec<_>, OxyError>()
    }
}

impl TemplateRegister for AgenticConfig {
    fn register_template(
        &self,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<(), OxyError> {
        let mut child_register = renderer.child_register();
        child_register
            .entry(&self.instruction.as_str())?
            .entry(&self.start.start)?
            .entry(&self.end.end)?;
        child_register.entries(
            self.transitions
                .iter()
                .map(|t| t.trigger.clone())
                .collect::<Vec<_>>(),
        )?;
        Ok(())
    }
}

impl TemplateRegister for Start {
    fn register_template(
        &self,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<(), OxyError> {
        if let StartMode::Plan {
            instruction,
            example,
            ..
        } = &self.mode
        {
            renderer.register_template(instruction)?;
            renderer.register_template(example)?;
        }
        Ok(())
    }
}

impl TemplateRegister for End {
    fn register_template(
        &self,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<(), OxyError> {
        if let EndMode::Synthesize { instruction, .. } = &self.mode {
            renderer.register_template(instruction)?;
        }
        Ok(())
    }
}

impl TemplateRegister for TriggerType {
    fn register_template(
        &self,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<(), OxyError> {
        match self {
            TriggerType::Start(s) => s.register_template(renderer),
            TriggerType::End(e) => e.register_template(renderer),
            TriggerType::Query(q) => q.register_template(renderer),
            TriggerType::Visualize(v) => v.register_template(renderer),
            TriggerType::Insight(i) => i.register_template(renderer),
            _ => Ok(()),
        }
    }
}

impl TemplateRegister for Query {
    fn register_template(
        &self,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<(), OxyError> {
        renderer.register_template(&self.instruction)?;
        Ok(())
    }
}

impl TemplateRegister for Visualize {
    fn register_template(
        &self,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<(), OxyError> {
        renderer.register_template(&self.instruction)?;
        Ok(())
    }
}

impl TemplateRegister for Insight {
    fn register_template(
        &self,
        renderer: &crate::execute::renderer::Renderer,
    ) -> Result<(), OxyError> {
        renderer.register_template(&self.instruction)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct StartConfig {
    #[serde(flatten)]
    pub start: Start,
    pub next: TransitionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct EndConfig {
    #[serde(flatten)]
    pub end: End,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(untagged)]
pub enum TransitionMode {
    Always(String),
    Auto(Vec<String>),
    #[default]
    Plan,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Transition {
    #[serde(flatten)]
    pub trigger: TriggerType,
    #[serde(default = "TransitionMode::default")]
    pub next: TransitionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransitionObjective {
    #[schemars(
        description = "Give a detailed explanation of why this action is being taken. Format as follows:
        ## Query Examples:
        - Get all customers from California who joined after 2023
        - Count total orders per product category this quarter
        - Find employees with salary above $75,000 in the engineering department
        - List customer names with their total purchase amounts, only for customers who have made at least 3 orders
        - Show top 10 best-selling products by revenue in the last 30 days
        - Calculate month-over-month growth in user signups for 2024
        ## Visualize Example:
        - Show market share distribution by company as a pie chart
        - Display percentage breakdown of expenses by category (rent, utilities, salaries, other)
        - Plot monthly revenue trend for the past 12 months as a line chart
        - Show daily active users over the last 30 days
        - Compare total sales by region using a bar chart
        - How top 5 products by units sold as a horizontal bar chart
        ## Troubleshoot Example:
        ### Viz Issue:
        - Fix bar chart showing no data - empty result set from query. Error: \"No data available for the selected parameters.\"
        - Resolve pie chart with incorrect percentages - data aggregation error. Error: \"Sum of values does not equal total.\"
        ### Query Issue:
        - Correct SQL syntax error in query - missing WHERE clause. Error: \"column reference 'id' is ambiguous\"
        - Fix ambiguous column reference when joining users and orders tables. Error: \"column reference 'id' is ambiguous\"
        "
    )]
    pub objective: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisePlan {
    #[schemars(description = "Review your plan based on what just happened.

Analyze:
- What worked as expected vs what didn't?
- What new information did you learn?
- Are there missing steps or unnecessary ones?
- Do any steps need reordering or clarification?

Then output your revision in this format:

CHANGES:
- [Describe specific changes to the plan, e.g., 'Add step to validate data before visualization', 'Remove redundant analysis step', 'Reorder steps 3 and 4']

If no changes are needed, respond with: 'No changes needed - plan is on track.'")]
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ErrorTriage {
    #[schemars(
        description = "Give a detailed explanation of why this action is being taken based on the error encountered."
    )]
    pub error_triage: String,
}

impl Transition {
    pub fn get_tool(&self) -> ChatCompletionTool {
        ChatCompletionTool {
            function: FunctionObject {
                name: self.trigger.get_name().to_string(),
                description: Some(self.trigger.get_description().to_string()),
                parameters: Some(serde_json::json!(&schemars::schema_for!(
                    TransitionObjective
                ))),
                strict: Some(true),
            },
        }
    }

    pub fn get_transition_names(&self) -> Vec<String> {
        match &self.next {
            TransitionMode::Always(name) => vec![name.clone()],
            TransitionMode::Auto(names) => names.clone(),
            TransitionMode::Plan => vec![],
        }
    }
}

impl From<EndConfig> for Transition {
    fn from(end: EndConfig) -> Transition {
        Transition {
            next: TransitionMode::Plan,
            trigger: TriggerType::End(end.end),
        }
    }
}

impl From<StartConfig> for Transition {
    fn from(start: StartConfig) -> Transition {
        let mut names = match start.next {
            TransitionMode::Always(name) => vec![name],
            TransitionMode::Auto(names) => names,
            TransitionMode::Plan => vec![],
        };
        if !names.contains(&AGENT_END_TRANSITION.to_string()) {
            names.push(AGENT_END_TRANSITION.to_string());
        }

        Transition {
            next: TransitionMode::Auto(names),
            trigger: TriggerType::Start(start.start),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerType {
    Start(Start),
    End(End),
    Query(Query),
    Visualize(Visualize),
    Insight(Insight),
    Subflow(Subflow),
}

impl TriggerType {
    pub fn is_end(&self) -> bool {
        matches!(self, TriggerType::End(_))
    }

    pub fn is_start(&self) -> bool {
        matches!(self, TriggerType::Start(_))
    }

    pub fn get_name(&self) -> &str {
        match self {
            TriggerType::Start(s) => &s.name,
            TriggerType::End(e) => &e.name,
            TriggerType::Query(q) => &q.name,
            TriggerType::Visualize(v) => &v.name,
            TriggerType::Insight(i) => &i.name,
            TriggerType::Subflow(s) => &s.name,
        }
    }

    pub fn get_step_kind(&self) -> StepKind {
        match self {
            TriggerType::Start(s) => match s.mode {
                StartMode::Plan { .. } => StepKind::Plan,
                _ => StepKind::Idle,
            },
            TriggerType::End(e) => match e.output_artifact {
                OutputArtifact::App => StepKind::BuildApp,
                _ => StepKind::End,
            },
            TriggerType::Query(_) => StepKind::Query,
            TriggerType::Visualize(_) => StepKind::Visualize,
            TriggerType::Insight(_) => StepKind::Insight,
            TriggerType::Subflow(_) => StepKind::Subflow,
        }
    }

    pub fn get_description(&self) -> &str {
        match self {
            TriggerType::Start(s) => &s.description,
            TriggerType::End(e) => &e.description,
            TriggerType::Query(q) => &q.description,
            TriggerType::Visualize(v) => &v.description,
            TriggerType::Insight(i) => &i.description,
            TriggerType::Subflow(s) => &s.description,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticInput {
    pub context_id: String,
    pub prompt: String,
    pub trace: Vec<ChatCompletionRequestMessage>,
}
