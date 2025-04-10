use minijinja::Value;

use crate::{config::constants::AGENT_SOURCE_PROMPT, errors::OxyError};

use super::OutputContainer;

#[derive(Clone, Debug)]
pub struct TargetOutput {
    pub output: String,
    pub task_description: Option<String>,
}

impl TryFrom<&OutputContainer> for TargetOutput {
    type Error = OxyError;

    fn try_from(value: &OutputContainer) -> Result<Self, Self::Error> {
        let (output, task_description) = match value {
            OutputContainer::Single(output) => {
                let output = Value::from_object(output.clone()).to_string();
                (output, None)
            }
            OutputContainer::Metadata { output, metadata } => {
                let output = Value::from_object(output.clone()).to_string();
                let task_description = metadata.get(AGENT_SOURCE_PROMPT).cloned();
                (output, task_description)
            }
            _ => {
                return Err(OxyError::RuntimeError(format!(
                    "Failed to convert OutputContainer to TargetOutput: {:?}",
                    value
                )));
            }
        };
        Ok(Self {
            output,
            task_description,
        })
    }
}
