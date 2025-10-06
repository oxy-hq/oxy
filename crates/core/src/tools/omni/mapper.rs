use crate::{
    errors::OxyError,
    execute::{ExecutionContext, builders::map::ParamMapper},
    tools::{
        omni::types::{OmniQueryInput, OmniQueryToolInput},
        types::OmniQueryParams,
    },
};

#[derive(Clone)]
pub struct OmniQueryToolMapper;

#[async_trait::async_trait]
impl ParamMapper<OmniQueryToolInput, OmniQueryInput> for OmniQueryToolMapper {
    async fn map(
        &self,
        _execution_context: &ExecutionContext,
        input: OmniQueryToolInput,
    ) -> Result<(OmniQueryInput, Option<ExecutionContext>), OxyError> {
        let OmniQueryToolInput {
            param,
            topic,
            integration,
        } = input;
        let query = serde_json::from_str::<OmniQueryParams>(&param)?;
        Ok((
            OmniQueryInput {
                params: query,
                topic,
                integration,
            },
            None,
        ))
    }
}
