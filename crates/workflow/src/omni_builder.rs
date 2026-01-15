use oxy::{
    config::model::OmniQueryTask,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::Output,
    },
    observability::events::workflow as workflow_events,
    tools::omni::{executable::OmniQueryExecutable, types::OmniQueryInput},
    types::tool_params::OmniQueryParams,
};
use oxy_shared::errors::OxyError;

#[derive(Clone)]
struct OmniQueryTaskMapper;

#[async_trait::async_trait]
impl ParamMapper<OmniQueryTask, OmniQueryInput> for OmniQueryTaskMapper {
    #[tracing::instrument(skip_all, err, fields(
        otel.name = workflow_events::task::omni_query::NAME_MAP,
        oxy.span_type = workflow_events::task::omni_query::TYPE,
        oxy.omni_query.integration = %input.integration,
        oxy.omni_query.topic = %input.topic,
    ))]
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: OmniQueryTask,
    ) -> Result<(OmniQueryInput, Option<ExecutionContext>), OxyError> {
        workflow_events::task::omni_query::map_input(&input);

        let mut rendered_fields = vec![];
        for raw_field in &input.query.fields {
            let rendered_field = execution_context.renderer.render_str(raw_field)?;
            rendered_fields.push(rendered_field);
        }

        workflow_events::task::omni_query::map_output(
            &input.integration,
            &input.topic,
            rendered_fields.len(),
        );

        return Ok((
            OmniQueryInput {
                integration: input.integration,
                topic: input.topic,
                params: OmniQueryParams {
                    fields: rendered_fields,
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
