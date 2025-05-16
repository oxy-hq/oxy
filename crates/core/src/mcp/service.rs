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
    errors::OxyError,
    service::{
        agent::{ask_adhoc, get_agent_config, list_agents},
        workflow::{get_workflow, list_workflows, run_workflow},
    },
    workflow::loggers::NoopLogger,
};

#[derive(Debug, Clone)]
pub enum ToolType {
    Agent,
    Workflow,
}

#[derive(Debug, Clone)]
pub struct OxyTool {
    pub tool: Tool,
    pub tool_type: ToolType,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct OxyMcpServer {
    pub project_path: PathBuf,
    pub tools: HashMap<String, OxyTool>,
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
            instructions: Some("Oxy is the Data Agent Platform that brings intelligence to your structured enterprise data. Answer, build, and automate anything.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _: std::option::Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::Error> {
        let tools = self
            .tools
            .values()
            .map(|oxy_tool| oxy_tool.tool.clone())
            .collect::<Vec<_>>();
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
        let oxy_tool = self
            .tools
            .get(tool_name.as_str())
            .ok_or(rmcp::Error::invalid_request(
                format!("Tool {} not found", tool_name),
                None,
            ))?;
        match oxy_tool.tool_type {
            ToolType::Agent => {
                return self
                    .run_agent_tool(oxy_tool.name.to_owned(), request.arguments)
                    .await;
            }
            ToolType::Workflow => {
                return self
                    .run_workflow_tool(oxy_tool.name.to_owned(), request.arguments)
                    .await;
            }
        }
    }
}

impl OxyMcpServer {
    pub async fn new(project_path: PathBuf) -> Result<Self, OxyError> {
        let tools = get_oxy_tools(project_path.clone()).await?;
        Ok(Self {
            project_path,
            tools,
        })
    }

    async fn run_agent_tool(
        &self,
        agent_name: String,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, rmcp::Error> {
        std::env::set_current_dir(&self.project_path).map_err(|e| {
            rmcp::Error::internal_error(format!("Failed to set current directory: {}", e), None)
        })?;

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
        std::env::set_current_dir(&self.project_path).map_err(|e| {
            rmcp::Error::internal_error(format!("Failed to set current directory: {}", e), None)
        })?;

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

        let output = run_workflow(
            &PathBuf::from(workflow_info.path.clone()),
            NoopLogger {},
            false,
            variables,
        )
        .await
        .map_err(|e| rmcp::Error::internal_error(format!("Failed to run workflow: {}", e), None))?;

        Ok(CallToolResult {
            content: vec![output.try_into().map_err(|_err| {
                rmcp::Error::internal_error(
                    "Failed to convert from workflow output into mcp output".to_string(),
                    None,
                )
            })?],
            is_error: Some(false),
        })
    }
}

fn json_to_hashmap(
    json: serde_json::Map<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    let mut lookup = json.clone();
    let mut map = HashMap::new();
    for key in json.keys() {
        let (k, v) = lookup.remove_entry(key).unwrap();
        map.insert(k, v);
    }
    map
}

async fn get_oxy_tools(project_path: PathBuf) -> Result<HashMap<String, OxyTool>, OxyError> {
    let mut tools_map = get_agent_tools(project_path.clone()).await?;
    tools_map.extend(get_workflow_tools(project_path.clone()).await?);
    Ok(tools_map)
}

async fn get_agent_tools(project_path: PathBuf) -> Result<HashMap<String, OxyTool>, OxyError> {
    let mut tools_map = HashMap::new();
    for agent in list_agents(project_path.clone()).await? {
        let agent_config = get_agent_config(project_path.clone(), agent.clone()).await?;
        let schema = serde_json::from_value(json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "question to ask the agent"
                }
            }
        }))?;
        let tool_name = get_agent_tool_name(agent_config.name.as_str());
        let tool = Tool::new(
            get_agent_tool_name(agent_config.name.as_str()),
            agent_config.description,
            Arc::new(schema),
        );
        let oxy_tool = OxyTool {
            tool: tool.clone(),
            tool_type: ToolType::Agent,
            name: agent_config.name.to_owned(),
        };
        tools_map.insert(tool_name, oxy_tool);
    }
    Ok(tools_map)
}

async fn get_workflow_tools(project_path: PathBuf) -> Result<HashMap<String, OxyTool>, OxyError> {
    let mut tools_map = HashMap::new();
    let workflows = list_workflows(Some(project_path.clone())).await?;

    for workflow in workflows {
        let workflow_config = get_workflow(
            PathBuf::from(workflow.path.clone()),
            Some(project_path.clone()),
        )
        .await?;

        let tool_name = get_workflow_tool_name(workflow.name.as_str());
        let tool = Tool::new(
            tool_name.clone(),
            workflow_config.description,
            Arc::new(workflow_config.variables.unwrap_or_default().into()),
        );

        let oxy_tool = OxyTool {
            tool: tool.clone(),
            tool_type: ToolType::Workflow,
            name: workflow.name,
        };
        tools_map.insert(tool_name, oxy_tool);
    }
    Ok(tools_map)
}

const AGENT_TOOL_PREFIX: &str = "agent-";
const WORKFLOW_TOOL_PREFIX: &str = "workflow-";

fn get_agent_tool_name(agent_name: &str) -> String {
    format!("{}{}", AGENT_TOOL_PREFIX, agent_name)
}
fn get_workflow_tool_name(workflow_name: &str) -> String {
    format!("{}{}", WORKFLOW_TOOL_PREFIX, workflow_name)
}
