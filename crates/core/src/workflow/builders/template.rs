use crate::{
    config::model::{Condition, LoopValues, SQL, Task, TaskType, Workflow},
    errors::OxyError,
    execute::renderer::{Renderer, TemplateRegister},
};

impl TemplateRegister for Workflow {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        renderer.register(&self.tasks)
    }
}

impl TemplateRegister for &Task {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        let mut register = renderer.child_register();

        if let Some(cache) = &self.cache {
            register.entry(&cache.path.as_str())?;
        }

        match &self.task_type {
            TaskType::Workflow(workflow) => {
                if let Some(variables) = workflow.variables.clone() {
                    register.entries(
                        variables
                            .iter()
                            .filter_map(|(_key, value)| value.as_str())
                            .collect::<Vec<&str>>(),
                    )?;
                }
            }
            TaskType::Agent(agent) => {
                register.entry(&agent.prompt.as_str())?;
                if let Some(export) = &agent.export {
                    register.entry(&export.path.as_str())?;
                }
            }
            TaskType::ExecuteSQL(execute_sql) => {
                let sql = match &execute_sql.sql {
                    SQL::Query { sql_query } => sql_query,
                    SQL::File { sql_file } => sql_file,
                };
                register.entry(&sql.as_str())?;
                if let Some(variables) = &execute_sql.variables {
                    register.entries(
                        variables
                            .iter()
                            .map(|(_key, value)| value.as_str())
                            .collect::<Vec<&str>>(),
                    )?;
                }
                if let Some(export) = &execute_sql.export {
                    register.entry(&export.path.as_str())?;
                }
            }
            TaskType::Formatter(formatter) => {
                register.entry(&formatter.template.as_str())?;
                if let Some(export) = &formatter.export {
                    register.entry(&export.path.as_str())?;
                }
            }
            TaskType::LoopSequential(loop_sequential) => {
                if let LoopValues::Template(template) = &loop_sequential.values {
                    register.entry(&template.as_str())?;
                }
                register.entry(&loop_sequential.tasks)?;
            }
            TaskType::Conditional(conditional) => {
                register.entry(&conditional.conditions)?;
                if let Some(else_tasks) = &conditional.else_tasks {
                    register.entry(else_tasks)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl TemplateRegister for &Condition {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        let child_register = renderer.child_register();
        child_register.entry(&self.if_expr.as_str())?;
        child_register.entry(&self.tasks)?;
        Ok(())
    }
}

impl TemplateRegister for Vec<Condition> {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        let mut child_register = renderer.child_register();
        child_register.entries(self)?;
        Ok(())
    }
}

impl TemplateRegister for Vec<Task> {
    fn register_template(&self, renderer: &Renderer) -> Result<(), OxyError> {
        let mut child_register = renderer.child_register();
        child_register.entries(self)?;
        Ok(())
    }
}
