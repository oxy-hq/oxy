use super::renderer::{Renderer, TemplateRegister};
use crate::config::model::{Condition, ConditionalTask, SQL, Task, TaskType, Workflow};
use oxy_shared::errors::OxyError;

impl TemplateRegister for Workflow {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        self.tasks.register_template(renderer)?;
        Ok(())
    }
}

impl TemplateRegister for &Task {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        match &self.task_type {
            TaskType::Conditional(conditional_task) => {
                conditional_task.register_template(renderer)?;
            }
            TaskType::OmniQuery(_omni_task) => {
                // OmniQueryTask doesn't have file templates to register
            }
            TaskType::Agent(_agent_task) => {
                // AgentTask doesn't have file templates to register
            }
            TaskType::SemanticQuery(_semantic_task) => {
                // SemanticQueryTask doesn't have file templates to register
            }
            TaskType::ExecuteSQL(sql_task) => {
                // Register SQL file template if it's a file reference
                if let SQL::File { sql_file } = &sql_task.sql {
                    renderer.register_template(sql_file)?;
                }
            }
            TaskType::Workflow(_workflow_task) => {
                // WorkflowTask doesn't have file templates to register
            }
            TaskType::Formatter(_formatter_task) => {
                // FormatterTask doesn't have file templates to register
            }
            TaskType::LoopSequential(loop_task) => {
                loop_task.tasks.register_template(renderer)?;
            }
            TaskType::Visualize(_visualize_task) => {
                // VisualizeTask doesn't have file templates to register
            }
            TaskType::Unknown => {
                // Unknown task type, skip
            }
        }
        Ok(())
    }
}

impl TemplateRegister for &ConditionalTask {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        for condition in &self.conditions {
            condition.register_template(renderer)?;
        }
        if let Some(ref else_tasks) = self.else_tasks {
            else_tasks.register_template(renderer)?;
        }
        Ok(())
    }
}

impl TemplateRegister for &Condition {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        // Don't register the if_expr - it's an inline template, not a file path
        // Just register templates in the tasks
        self.tasks.register_template(renderer)?;
        Ok(())
    }
}

impl TemplateRegister for Vec<Task> {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        for task in self {
            task.register_template(renderer)?;
        }
        Ok(())
    }
}
