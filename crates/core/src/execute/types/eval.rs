use minijinja::Value;

use crate::{config::constants::AGENT_SOURCE_PROMPT, errors::OxyError};

use super::{Metadata, OutputContainer, ReferenceKind};

#[derive(Clone, Debug)]
pub struct TargetOutput {
    pub output: String,
    pub task_description: Option<String>,
    pub relevant_contexts: Vec<String>,
}

impl TryFrom<&OutputContainer> for TargetOutput {
    type Error = OxyError;

    fn try_from(value: &OutputContainer) -> Result<Self, Self::Error> {
        let (output, task_description, relevant_contexts) = match value {
            OutputContainer::Single(output) => {
                let output = Value::from_object(output.clone()).to_string();
                (output, None, vec![])
            }
            OutputContainer::Metadata { value } | OutputContainer::Consistency { value, .. } => {
                let Metadata {
                    output,
                    metadata,
                    references,
                } = value;
                let output = Value::from_object(output.clone()).to_string();
                let task_description = metadata.get(AGENT_SOURCE_PROMPT).cloned();
                (
                    output,
                    task_description,
                    references
                        .iter()
                        .filter_map(|r| match r {
                            ReferenceKind::Retrieval(r) => Some(r.documents.clone()),
                            _ => None,
                        })
                        .flatten()
                        .collect::<Vec<_>>(),
                )
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
            relevant_contexts,
        })
    }
}
