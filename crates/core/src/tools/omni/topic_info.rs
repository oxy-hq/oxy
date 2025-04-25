use crate::{
    config::model::OmniField,
    errors::OxyError,
    execute::{Executable, ExecutionContext, types::Output},
    tools::{
        tool::Tool,
        types::{OmniTopicInfoInput, OmniTopicInfoParams},
    },
};

#[derive(Debug, Clone)]
pub struct OmniTopicInfoExecutable;

impl Tool for OmniTopicInfoExecutable {
    type Param = OmniTopicInfoParams;
    type Output = String;

    fn serialize_output(&self, output: &Self::Output) -> Result<String, OxyError> {
        Ok(output.to_string())
    }
}

#[async_trait::async_trait]
impl Executable<OmniTopicInfoInput> for OmniTopicInfoExecutable {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: OmniTopicInfoInput,
    ) -> Result<Self::Response, OxyError> {
        log::debug!(
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
