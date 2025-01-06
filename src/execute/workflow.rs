use std::path::PathBuf;

use minijinja::Value;

use crate::{
    config::{
        load_config,
        model::{LoopValues, Step, StepType, Workflow},
    },
    errors::OnyxError,
    workflow::{executor::WorkflowExecutor, WorkflowResult},
};

use super::{
    core::{value::ContextValue, Executable, ExecutionContext, OutputCollector},
    renderer::{Renderer, TemplateRegister},
};

impl TemplateRegister for Workflow {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        renderer.register(&self.steps)
    }
}

impl TemplateRegister for &Step {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        let mut register = renderer.struct_register();
        match &self.step_type {
            StepType::Agent(agent) => {
                register.field(&agent.prompt.as_str())?;
            }
            StepType::ExecuteSQL(execute_sql) => {
                register.field(&execute_sql.sql_file.as_str())?;
                match &execute_sql.variables {
                    Some(variables) => {
                        register.fields(
                            variables
                                .iter()
                                .map(|(_key, value)| value.as_str())
                                .collect::<Vec<&str>>(),
                        )?;
                    }
                    None => {}
                }
            }
            StepType::Formatter(formatter) => {
                register.field(&formatter.template.as_str())?;
            }
            StepType::LoopSequential(loop_sequential) => {
                match &loop_sequential.values {
                    LoopValues::Template(template) => {
                        register.field(&template.as_str())?;
                    }
                    _ => {}
                }
                register.field(&loop_sequential.steps)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl TemplateRegister for Vec<Step> {
    fn register_template(&self, renderer: &mut Renderer) -> Result<(), OnyxError> {
        let mut list_register = renderer.list_register();
        list_register.items(self)?;
        Ok(())
    }
}

pub async fn run_workflow(workflow_path: &PathBuf) -> Result<WorkflowResult, OnyxError> {
    let config = load_config()?;
    let workflow = config.load_workflow(workflow_path)?;
    config.validate_workflow(&workflow).map_err(|e| {
        OnyxError::ConfigurationError(format!("Invalid workflow configuration: {}", e))
    })?;

    let mut renderer = Renderer::new();
    renderer.register(&workflow)?;
    let mut output_collector = OutputCollector::default();
    let mut execution_context = ExecutionContext::new(
        Value::UNDEFINED,
        &mut renderer,
        &Value::UNDEFINED,
        &mut output_collector,
    );
    let executor = WorkflowExecutor::new(workflow);
    executor.execute(&mut execution_context).await?;
    let output = output_collector.output.unwrap_or_default();
    let result = ContextValue::Map(
        [
            ("output".to_string(), output.clone()),
            (
                "steps".to_string(),
                ContextValue::Array(
                    [ContextValue::Map(
                        [
                            ("name".to_string(), ContextValue::Text("".to_string())),
                            ("output".to_string(), ContextValue::Text("".to_string())),
                        ]
                        .iter()
                        .collect(),
                    )]
                    .iter()
                    .collect(),
                ),
            ),
        ]
        .iter()
        .collect(),
    );
    Ok(result.into())
}
