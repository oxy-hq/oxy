use oxy::{
    config::model::LookerQueryTask,
    execute::{
        Executable, ExecutionContext,
        builders::{ExecutableBuilder, map::ParamMapper},
        types::Output,
    },
    observability::events::workflow as workflow_events,
    tools::looker::{executable::LookerQueryExecutable, types::LookerQueryInput},
};
use oxy_shared::errors::OxyError;

#[derive(Clone)]
struct LookerQueryTaskMapper;

#[async_trait::async_trait]
impl ParamMapper<LookerQueryTask, LookerQueryInput> for LookerQueryTaskMapper {
    #[tracing::instrument(skip_all, err, fields(
        oxy.name = workflow_events::task::looker_query::NAME_MAP,
        oxy.span_type = workflow_events::task::looker_query::TYPE,
        oxy.looker_query.integration = %input.integration,
        oxy.looker_query.model = %input.model,
        oxy.looker_query.explore = %input.explore,
    ))]
    async fn map(
        &self,
        execution_context: &ExecutionContext,
        input: LookerQueryTask,
    ) -> Result<(LookerQueryInput, Option<ExecutionContext>), OxyError> {
        workflow_events::task::looker_query::map_input(&input);

        let mut rendered_fields = vec![];
        for raw_field in &input.query.fields {
            let rendered_field = execution_context.renderer.render_str(raw_field)?;
            rendered_fields.push(rendered_field);
        }

        workflow_events::task::looker_query::map_output(
            &input.integration,
            &input.model,
            &input.explore,
            rendered_fields.len(),
        );

        Ok((
            LookerQueryInput {
                integration: input.integration,
                model: input.model,
                explore: input.explore,
                params: oxy::config::model::LookerQueryParams {
                    fields: rendered_fields,
                    filters: input.query.filters,
                    filter_expression: input.query.filter_expression,
                    sorts: input.query.sorts,
                    limit: input.query.limit,
                },
            },
            None,
        ))
    }
}

pub fn build_looker_query_task_executable() -> impl Executable<LookerQueryTask, Response = Output> {
    ExecutableBuilder::new()
        .map(LookerQueryTaskMapper)
        .executable(LookerQueryExecutable::new())
}
