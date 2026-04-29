use async_trait::async_trait;

use oxy::{
    config::model::ToolType,
    execute::{ExecutionContext, types::Output, types::OutputContainer},
    tools::{ToolExecutor, types::ToolRawInput},
};
use oxy_shared::errors::OxyError;

use crate::service::AirformService;

pub struct DbtToolExecutor;

#[async_trait]
impl ToolExecutor for DbtToolExecutor {
    async fn execute(
        &self,
        execution_context: &ExecutionContext,
        tool_type: &ToolType,
        input: &ToolRawInput,
    ) -> Result<OutputContainer, OxyError> {
        let project_dir = execution_context
            .workspace
            .config_manager
            .workspace_path()
            .to_path_buf();
        let service = AirformService::new(project_dir);

        match tool_type {
            ToolType::DbtCompile(config) => {
                let model = config
                    .model
                    .as_deref()
                    .or(if input.param.trim().is_empty() {
                        None
                    } else {
                        Some(input.param.as_str())
                    });

                let result = if let Some(model_name) = model {
                    service
                        .compile_model(model_name)
                        .map_err(|e| OxyError::RuntimeError(e.to_string()))?
                } else {
                    let output = service
                        .compile_project()
                        .map_err(|e| OxyError::RuntimeError(e.to_string()))?;
                    serde_json::to_string_pretty(&output)
                        .unwrap_or_else(|_| format!("{} models compiled", output.models_compiled))
                };

                Ok(OutputContainer::Single(Output::Text(result)))
            }
            ToolType::DbtRun(config) => {
                let selector: Option<String> =
                    config
                        .selector
                        .clone()
                        .or(if input.param.trim().is_empty() {
                            None
                        } else {
                            Some(input.param.clone())
                        });

                let output = service
                    .run(selector.as_deref())
                    .await
                    .map_err(|e| OxyError::RuntimeError(e.to_string()))?;

                let summary = format!(
                    "Run completed: {} results\n{}",
                    output.results.len(),
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );

                Ok(OutputContainer::Single(Output::Text(summary)))
            }
            _ => Err(OxyError::RuntimeError(
                "DbtToolExecutor can only handle DbtRun and DbtCompile tools".to_string(),
            )),
        }
    }

    fn can_handle(&self, tool_type: &ToolType) -> bool {
        matches!(tool_type, ToolType::DbtRun(_) | ToolType::DbtCompile(_))
    }

    fn name(&self) -> &'static str {
        "DbtToolExecutor"
    }
}

pub async fn register_dbt_executor() -> Result<(), OxyError> {
    use std::sync::Arc;
    oxy::tools::register_tool_executor(Arc::new(DbtToolExecutor)).await?;
    tracing::info!("Registered dbt tool executor");
    Ok(())
}
