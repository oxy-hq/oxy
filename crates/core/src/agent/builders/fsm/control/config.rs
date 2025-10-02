use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    agent::builders::fsm::config::TransitionObjective,
    config::constants::{
        AGENT_CONTINUE_PLAN_TRANSITION, AGENT_END_TRANSITION, AGENT_REVISE_PLAN_TRANSITION,
        AGENT_START_TRANSITION,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum StartMode {
    #[default]
    Default,
    Plan {
        model: Option<String>,
        #[serde(default = "default_plan_instruction")]
        instruction: String,
        #[serde(default = "default_plan_example")]
        example: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Start {
    #[serde(default = "default_start_name")]
    pub name: String,
    #[serde(default = "default_start_description")]
    pub description: String,
    #[serde(flatten)]
    pub mode: StartMode,
}

fn default_start_name() -> String {
    AGENT_START_TRANSITION.to_string()
}

fn default_start_description() -> String {
    "The starting point of the agent's workflow".to_string()
}

impl Start {
    fn revise_tool(&self) -> ChatCompletionTool {
        let schema = serde_json::json!(&schemars::schema_for!(TransitionObjective));
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: AGENT_REVISE_PLAN_TRANSITION.to_string(),
                description: Some(
                    "Decide whether to revise the plan based on the conversation".to_string(),
                ),
                parameters: Some(schema),
                strict: None,
            },
        }
    }

    fn continue_tool(&self) -> ChatCompletionTool {
        ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: AGENT_CONTINUE_PLAN_TRANSITION.to_string(),
                description: Some("Continue with the current plan".to_string()),
                parameters: None,
                strict: None,
            },
        }
    }

    pub fn get_tools(&self) -> Vec<ChatCompletionTool> {
        vec![self.revise_tool(), self.continue_tool()]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum EndMode {
    #[default]
    Default,
    Synthesize {
        model: Option<String>,
        #[serde(default = "default_synthesize_instruction")]
        instruction: String,
    },
}

fn default_synthesize_instruction() -> String {
    "Given the conversation so far, synthesize a final answer that addresses the original objective. Ensure that the answer is clear, concise, and based on the information gathered during the workflow.".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputArtifact {
    #[default]
    None,
    App,
    Query,
    Visualization,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct End {
    #[serde(default = "default_end_name")]
    pub name: String,
    #[serde(default = "default_end_description")]
    pub description: String,
    #[serde(flatten)]
    pub mode: EndMode,
    #[serde(default)]
    pub output_artifact: OutputArtifact,
}

fn default_end_name() -> String {
    AGENT_END_TRANSITION.to_string()
}

fn default_end_description() -> String {
    "The ending point of the agent's workflow".to_string()
}

fn default_plan_instruction() -> String {
    "You are an expert Data Analyst. Your task is to break down the user's query into clear, actionable steps using the available actions.
    Each step should be concise and focused on achieving a specific objective that contributes to the overall goal.
    After outlining the steps, select the next action to take from the list of available actions.".to_string()
}

fn default_plan_example() -> String {
    "Example:

    Given the user query: Build a data app to analyze sales data in last quarter.
    And Available Actions:
    ```
    ### Available Actions
    - { \"type\": \"query\", \"description\": \"Use this action to query the database with a specific question. The question should be clear and concise, focusing on the information you need from the database.\" }
    - { \"type\": \"build_table\", \"description\": \"Use this action to create a table that summarizes or organizes data in a structured format. Specify the columns and the data to be included in the table.\" }
    - { \"type\": \"end\", \"description\": \"Use this action to conclude the workflow and provide a final answer or summary based on the previous steps.\" }
    ```

    You should respond in the following format:
    ```
    ### Plan to build a data app to analyze sales data
    - **Query** the database to get the total sales for the last quarter.
    - **Build table** to summarize the sales data by region and product category.
    - **End** by building a data app to visualize the sales trends over the last year. And summarize the findings.

    Lets take the next action: <action>
    ```
    ".to_string()
}
