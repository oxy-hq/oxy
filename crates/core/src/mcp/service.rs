use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    RoleServer, ServerHandler,
    model::{
        CallToolResult, Content, ListToolsResult, PaginatedRequestParam, ServerCapabilities,
        ServerInfo, Tool,
    },
    schemars,
    service::RequestContext,
};
use serde_json::{Map, Value, json};

use crate::{
    execute::{
        core::value::ContextValue,
        workflow::{NoopLogger, run_workflow},
    },
    service::{
        agent::{ask_adhoc, get_agent_config, list_agents},
        workflow::{get_workflow, list_workflows},
    },
};

#[derive(Debug, Clone)]
pub struct OxyMcpServer {
    pub project_path: PathBuf,
}

#[derive(Debug, Clone, schemars::JsonSchema, serde::Serialize)]
pub struct AgentResponse {
    pub answer: String,
}

#[derive(Debug, Clone, schemars::JsonSchema, serde::Serialize, serde::Deserialize)]
pub struct RunWorkflowInput {
    pub variables: Option<HashMap<String, String>>,
}

impl ServerHandler for OxyMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("A simple calculator".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _: PaginatedRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::Error> {
        let mut tools = self.list_agent_tools().await?;
        tools.extend(self.list_workflow_tools().await?);

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let tool_name = request.name.clone().to_string();
        if is_agent_tool(tool_name.as_str()) {
            let agent_name = get_agent_name_from_tool_name(tool_name.as_str());
            return self
                .run_agent_tool(agent_name.to_string(), request.arguments)
                .await;
        }
        if is_workflow_tool(tool_name.as_str()) {
            let workflow_name = get_workflow_name_from_tool_name(tool_name.as_str());
            return self
                .run_workflow_tool(workflow_name.to_string(), request.arguments)
                .await;
        }
        Err(rmcp::Error::invalid_request(
            format!("Tool {} not found", tool_name),
            None,
        ))
    }
}

impl OxyMcpServer {
    async fn list_agent_tools(&self) -> Result<Vec<Tool>, rmcp::Error> {
        let mut tools = Vec::new();
        for agent in list_agents(self.project_path.clone()).await.map_err(|e| {
            rmcp::Error::internal_error(format!("Failed to list agents: {}", e), None)
        })? {
            let agent_config = get_agent_config(self.project_path.clone(), agent.clone())
                .await
                .map_err(|e| {
                    rmcp::Error::internal_error(format!("Failed to get agent config: {}", e), None)
                })?;
            let schema = serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "question to ask the agent"
                    }
                }
            }))
            .map_err(|e| {
                rmcp::Error::internal_error(format!("Failed to parse schema: {}", e), None)
            })?;
            let tool = Tool::new(
                get_agent_tool_name(agent_config.name.as_str()),
                agent_config.description,
                Arc::new(schema),
            );
            tools.push(tool);
        }
        Ok(tools)
    }

    async fn list_workflow_tools(&self) -> Result<Vec<Tool>, rmcp::Error> {
        let mut tools = Vec::new();
        let workflows = list_workflows(Some(self.project_path.clone()))
            .await
            .map_err(|e| {
                rmcp::Error::internal_error(format!("Failed to list workflows: {}", e), None)
            })?;

        for workflow in workflows {
            let workflow_config = get_workflow(
                PathBuf::from(workflow.path),
                Some(self.project_path.clone()),
            )
            .await
            .map_err(|e| {
                rmcp::Error::internal_error(format!("Failed to get workflow config: {}", e), None)
            })?;
            let tool = Tool::new(
                get_workflow_tool_name(workflow.name.as_str()),
                workflow_config.description,
                Arc::new(generate_workflow_run_schema(
                    workflow_config.variables.clone(),
                )),
            );
            tools.push(tool);
        }
        Ok(tools)
    }

    async fn run_agent_tool(
        &self,
        agent_name: String,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, rmcp::Error> {
        match arguments {
            None => Err(rmcp::Error::invalid_request(
                "Missing 'arguments' parameter".to_string(),
                None,
            )),
            Some(args) => {
                let question = args.get("question").and_then(|v| v.as_str()).ok_or(
                    rmcp::Error::invalid_request("Missing 'question' parameter".to_string(), None),
                )?;

                let output = ask_adhoc(question.to_string(), self.project_path.clone(), agent_name)
                    .await
                    .map_err(|e| {
                        rmcp::Error::internal_error(format!("Failed to ask agent: {}", e), None)
                    })?;
                Ok(CallToolResult {
                    content: vec![Content::text(output)],
                    is_error: Some(false),
                })
            }
        }
    }

    async fn run_workflow_tool(
        &self,
        workflow_name: String,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, rmcp::Error> {
        let variables = match arguments {
            None => None,
            Some(args) => args
                .get("variables")
                .and_then(|v| v.as_object())
                .map(|v| json_to_hashmap(v.to_owned())),
        };

        let workflows = list_workflows(Some(self.project_path.clone()))
            .await
            .map_err(|e| {
                rmcp::Error::internal_error(format!("Failed to list workflows: {}", e), None)
            })?;

        let workflow_info = workflows.iter().find(|w| w.name == workflow_name).ok_or(
            rmcp::Error::invalid_request(format!("Workflow {} not found", workflow_name), None),
        )?;

        let workflow_config = get_workflow(
            PathBuf::from(workflow_info.path.clone()),
            Some(self.project_path.clone()),
        )
        .await
        .map_err(|e| {
            rmcp::Error::internal_error(format!("Failed to get workflow config: {}", e), None)
        })?;

        let output = run_workflow(
            &PathBuf::from(workflow_info.path.clone()),
            Some(self.project_path.clone()),
            variables,
            Some(Box::new(NoopLogger {})),
        )
        .await
        .map_err(|e| rmcp::Error::internal_error(format!("Failed to run workflow: {}", e), None))?;

        let last_task = workflow_config
            .tasks
            .last()
            .ok_or(rmcp::Error::internal_error(
                "Workflow has no tasks".to_string(),
                None,
            ))?;

        Ok(CallToolResult {
            content: vec![Content::text(
                get_final_result(output.output, last_task.name.clone()).unwrap(),
            )],
            is_error: Some(false),
        })
    }
}

fn json_to_hashmap(json: serde_json::Map<String, serde_json::Value>) -> HashMap<String, String> {
    let mut lookup = json.clone();
    let mut map = HashMap::new();
    for key in json.keys() {
        let (k, v) = lookup.remove_entry(key).unwrap();
        map.insert(k, v.as_str().unwrap().to_string());
    }
    map
}

fn get_final_result(value: ContextValue, last_task_name: String) -> Result<String, rmcp::Error> {
    match value {
        ContextValue::Map(map) => {
            let last_task_result = map.get_value(last_task_name.as_str());
            match last_task_result {
                Some(value) => get_final_result(value.to_owned(), last_task_name),
                None => Ok(serde_json::to_string(&map).unwrap()),
            }
        }
        ContextValue::Array(array) => Ok(serde_json::to_string(&array).unwrap()),
        ContextValue::Text(string) => Ok(string),
        ContextValue::None => Ok("None".to_owned()),
        ContextValue::Table(arrow_table) => Ok(serde_json::to_string(&arrow_table).unwrap()),
        ContextValue::Agent(agent_output) => {
            get_final_result(agent_output.output.as_ref().to_owned(), last_task_name)
        }
        ContextValue::Consistency(consistency_output) => {
            get_final_result(consistency_output.value.as_ref().to_owned(), last_task_name)
        }
    }
}

fn generate_workflow_run_schema(
    variables: Option<HashMap<String, String>>,
) -> serde_json::Map<String, Value> {
    if variables.is_none() {
        let mut schema = serde_json::Map::new();
        schema.insert("type".to_string(), Value::String("object".to_string()));

        return schema;
    }
    let mut schema = serde_json::Map::new();
    let mut variable_schema = serde_json::Map::new();
    let mut properties = serde_json::Map::new();
    let variables = variables.unwrap();

    for (key, _) in variables.iter() {
        properties.insert(
            key.clone(),
            json!(
                {
                    "type": "string",
                }
            ),
        );
    }
    variable_schema.insert("type".to_string(), Value::String("object".to_string()));
    variable_schema.insert("properties".to_string(), Value::Object(properties));

    schema.insert(
        "properties".to_string(),
        json!({
            "variables": variable_schema,
        }),
    );
    schema.insert("type".to_string(), Value::String("object".to_string()));

    schema
}

const AGENT_TOOL_PREFIX: &str = "agent-";
const WORKFLOW_TOOL_PREFIX: &str = "workflow-";

fn is_agent_tool(tool_name: &str) -> bool {
    tool_name.starts_with(AGENT_TOOL_PREFIX)
}
fn is_workflow_tool(tool_name: &str) -> bool {
    tool_name.starts_with(WORKFLOW_TOOL_PREFIX)
}
fn get_agent_tool_name(agent_name: &str) -> String {
    format!("{}{}", AGENT_TOOL_PREFIX, agent_name)
}
fn get_workflow_tool_name(workflow_name: &str) -> String {
    format!("{}{}", WORKFLOW_TOOL_PREFIX, workflow_name)
}
fn get_agent_name_from_tool_name(tool_name: &str) -> String {
    tool_name.split_at(AGENT_TOOL_PREFIX.len()).1.to_string()
}
fn get_workflow_name_from_tool_name(tool_name: &str) -> String {
    tool_name.split_at(WORKFLOW_TOOL_PREFIX.len()).1.to_string()
}
