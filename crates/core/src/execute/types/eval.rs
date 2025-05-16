use crate::{config::constants::AGENT_SOURCE_PROMPT, errors::OxyError};

use super::{Document, Metadata, OutputContainer, ReferenceKind};

#[derive(Clone, Debug)]
pub enum RelevantContextGetter {
    Id,
    Content,
}

impl RelevantContextGetter {
    pub fn get(&self, document: &Document) -> String {
        match self {
            RelevantContextGetter::Id => document.id.clone(),
            RelevantContextGetter::Content => document.content.clone(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct TargetOutput {
    pub output: String,
    pub task_description: Option<String>,
    pub relevant_contexts: Vec<String>,
}

#[derive(Debug)]
pub struct OutputGetter<'a> {
    pub value: &'a OutputContainer,
    pub relevant_context_getter: &'a RelevantContextGetter,
}

impl<'a> TryFrom<OutputGetter<'a>> for TargetOutput {
    type Error = OxyError;

    fn try_from(getter: OutputGetter) -> Result<Self, Self::Error> {
        let (output, task_description, relevant_contexts) = match getter.value {
            OutputContainer::Single(output) => {
                let output = output.to_string();
                (output, None, vec![])
            }
            OutputContainer::Metadata { value } | OutputContainer::Consistency { value, .. } => {
                let Metadata {
                    output,
                    metadata,
                    references,
                } = value;
                let output = output.to_string();
                let task_description = metadata.get(AGENT_SOURCE_PROMPT).cloned();
                (
                    output,
                    task_description,
                    references
                        .iter()
                        .filter_map(|r| match r {
                            ReferenceKind::Retrieval(r) => Some(
                                r.documents
                                    .iter()
                                    .map(|doc| getter.relevant_context_getter.get(doc))
                                    .collect::<Vec<_>>(),
                            ),
                            _ => None,
                        })
                        .flatten()
                        .collect::<Vec<_>>(),
                )
            }
            _ => {
                return Err(OxyError::RuntimeError(format!(
                    "Failed to convert OutputContainer to TargetOutput: {:?}",
                    getter
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
