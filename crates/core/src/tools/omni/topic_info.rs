use crate::{
    config::model::OmniField,
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::Output},
    tools::types::OmniTopicInfoInput,
};

#[derive(Debug, Clone)]
pub struct OmniTopicInfoExecutable;

#[async_trait::async_trait]
impl Executable<OmniTopicInfoInput> for OmniTopicInfoExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        _execution_context: &ExecutionContext,
        input: OmniTopicInfoInput,
    ) -> Result<Self::Response, OxyError> {
        tracing::debug!(
            "{}",
            format!("Executing Omni topic tool with input: {:?}", input.topic)
        );
        let topic_fields = input.semantic_model.get_topic_fields(&input.topic)?;
        let mut detail = "Fields in topic: \n".to_string();

        for (field_name, field) in topic_fields {
            match field {
                OmniField::Dimension(dimension) => {
                    if let Some(description) = &dimension.description {
                        detail.push_str(format!("  - {} : {}\n", field_name, description).as_str());
                    } else {
                        detail.push_str(format!("  - {}", field_name).as_str());
                    }
                }
                OmniField::Measure(measure) => {
                    if let Some(description) = &measure.description {
                        detail.push_str(format!("  - {} : {}\n", field_name, description).as_str());
                    } else {
                        detail.push_str(format!("  - {}", field_name).as_str());
                    }
                }
            }
        }

        Ok(Output::Text(detail))
    }
}
