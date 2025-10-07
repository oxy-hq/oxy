use crate::{
    config::model::OmniQueryTask,
    errors::OxyError,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::Output,
    },
    tools::{
        omni::{executable::OmniQueryExecutable, types::OmniQueryInput},
        types::OmniQueryParams,
    },
};

#[derive(Clone)]
struct OmniQueryTaskMapper;

#[async_trait::async_trait]
impl ParamMapper<OmniQueryTask, OmniQueryInput> for OmniQueryTaskMapper {
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: OmniQueryTask,
    ) -> Result<(OmniQueryInput, Option<ExecutionContext>), OxyError> {
        let mut fields = vec![];
        for raw_field in input.query.fields {
            let rendered_field = execution_context.renderer.render(&raw_field)?;
            fields.push(rendered_field);
        }
        return Ok((
            OmniQueryInput {
                integration: input.integration,
                topic: input.topic,
                params: OmniQueryParams {
                    fields,
                    limit: input.query.limit,
                    sorts: input.query.sorts,
                },
            },
            None,
        ));
    }
}

pub fn build_omni_query_task_executable() -> impl Executable<OmniQueryTask, Response = Output> {
    ExecutableBuilder::new()
        .map(OmniQueryTaskMapper)
        .executable(OmniQueryExecutable::new())
}
