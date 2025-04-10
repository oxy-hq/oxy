use std::{collections::HashMap, hash::Hash};

use itertools::Itertools;
use minijinja::Value;
use serde::{Deserialize, Serialize};

use crate::{errors::OxyError, execute::types::Output};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputContainer {
    List(Vec<OutputContainer>),
    Map(HashMap<String, OutputContainer>),
    Single(Output),
    Consistency {
        value: Output,
        score: f32,
        metadata: HashMap<String, String>,
    },
    Metadata {
        output: Output,
        metadata: HashMap<String, String>,
    },
}

impl Default for OutputContainer {
    fn default() -> Self {
        OutputContainer::Map(HashMap::new())
    }
}

impl OutputContainer {
    pub fn merge(self, other: OutputContainer) -> OutputContainer {
        match (self, other) {
            (OutputContainer::List(mut list1), OutputContainer::List(list2)) => {
                list1.extend(list2);
                OutputContainer::List(list1)
            }
            (OutputContainer::Map(mut map1), OutputContainer::Map(map2)) => {
                map1.extend(map2);
                OutputContainer::Map(map1)
            }
            _ => panic!("Cannot merge different output type"),
        }
    }

    pub fn project_ref(&self, task_ref: &str) -> Result<Vec<&OutputContainer>, OxyError> {
        let mut containers = vec![self];
        for part in task_ref.split('.') {
            containers = containers
                .iter()
                .map(|container| container.find_ref(part))
                .try_collect::<Vec<&OutputContainer>, Vec<_>, OxyError>()
                .map(|item| item.into_iter().flatten().collect())?;
        }
        Ok(containers)
    }

    pub fn find_ref(&self, task_ref: &str) -> Result<Vec<&OutputContainer>, OxyError> {
        match self {
            OutputContainer::List(list) => list
                .iter()
                .map(|item| item.find_ref(task_ref))
                .try_collect::<Vec<&OutputContainer>, Vec<_>, OxyError>()
                .map(|item| item.into_iter().flatten().collect()),
            OutputContainer::Map(map) => {
                map.get(task_ref)
                    .map(|item| vec![item])
                    .ok_or(OxyError::RuntimeError(format!(
                        "Task ref `{}` not found",
                        task_ref
                    )))
            }
            _ => Err(OxyError::RuntimeError(format!(
                "Cannot find `{}` in {:?}",
                task_ref, self
            ))),
        }
    }
}

impl From<&OutputContainer> for Value {
    fn from(value: &OutputContainer) -> Self {
        match value {
            OutputContainer::List(list) => Value::from_iter(list.iter().map(Value::from)),
            OutputContainer::Map(map) => Value::from_iter(
                map.iter()
                    .map(|(k, v)| (k, Into::<Value>::into(v)))
                    .collect::<Vec<_>>(),
            ),
            OutputContainer::Single(output) => Value::from_object(output.clone()),
            OutputContainer::Metadata {
                output,
                metadata: _,
            } => Value::from_object(output.clone()),
            OutputContainer::Consistency {
                value,
                score,
                metadata: _,
            } => {
                let mut map = HashMap::new();
                map.insert("value".to_string(), Value::from_object(value.clone()));
                map.insert("score".to_string(), Value::from(*score));
                Value::from_iter(map)
            }
        }
    }
}

impl From<Output> for OutputContainer {
    fn from(val: Output) -> Self {
        OutputContainer::Single(val)
    }
}

impl From<HashMap<String, OutputContainer>> for OutputContainer {
    fn from(val: HashMap<String, OutputContainer>) -> Self {
        OutputContainer::Map(val)
    }
}

impl Hash for OutputContainer {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            OutputContainer::List(list) => {
                list.hash(state);
            }
            OutputContainer::Map(map) => {
                map.iter().for_each(|(key, value)| {
                    key.hash(state);
                    value.hash(state);
                });
            }
            OutputContainer::Single(output) => {
                output.hash(state);
            }
            OutputContainer::Metadata {
                output,
                metadata: _,
            } => {
                output.hash(state);
            }
            OutputContainer::Consistency {
                value,
                score,
                metadata: _,
            } => {
                value.hash(state);
                score.to_bits().hash(state);
            }
        }
    }
}
