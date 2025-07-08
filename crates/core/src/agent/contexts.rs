use crate::{
    StyledText,
    config::{
        ConfigManager,
        model::{AgentContext, AgentContextType, SemanticModels},
    },
};
use minijinja::value::{Object, ObjectRepr, Value};
use std::{collections::HashMap, fs, sync::Arc};
use tokio::runtime::Handle;

#[derive(Debug, Clone)]
pub struct Contexts {
    contexts: HashMap<String, AgentContext>,
    config: ConfigManager,
}

impl Contexts {
    pub fn new(contexts: Vec<AgentContext>, config: ConfigManager) -> Self {
        let contexts = contexts.into_iter().map(|c| (c.name.clone(), c)).collect();
        Contexts { contexts, config }
    }
}

impl Object for Contexts {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str();
        match key {
            Some(key) => match self.contexts.get(key) {
                Some(context) => match &context.context_type {
                    AgentContextType::File(file_context) => {
                        let rt = Handle::try_current().ok()?;
                        match rt.block_on(self.config.resolve_glob(&file_context.src)) {
                            Ok(paths) => {
                                let mut contents = vec![];
                                for path in paths {
                                    match fs::read_to_string(&path) {
                                        Ok(content) => {
                                            contents.push(content);
                                        }
                                        Err(e) => {
                                            println!(
                                                "{} {} {:?}",
                                                "Error reading file context: ".warning(),
                                                path.as_str(),
                                                e
                                            );
                                        }
                                    }
                                }
                                Some(Value::from(contents))
                            }
                            Err(e) => {
                                println!("{} {:?}", "Error expanding globs".warning(), e);
                                None
                            }
                        }
                    }
                    AgentContextType::SemanticModel(semantic_model_context) => {
                        let rt = Handle::try_current().ok()?;
                        let semantic_model_path = rt
                            .block_on(self.config.resolve_file(&semantic_model_context.src))
                            .ok()?;
                        match fs::read_to_string(semantic_model_path) {
                            Ok(content) => match serde_yaml::from_str::<SemanticModels>(&content) {
                                Ok(semantic_model) => Some(Value::from_serialize(semantic_model)),
                                Err(e) => {
                                    println!(
                                        "{} {:?}",
                                        "Error deserializing semantic model".warning(),
                                        e
                                    );
                                    None
                                }
                            },
                            Err(e) => {
                                println!("{} {:?}", "Error reading semantic model".warning(), e);
                                None
                            }
                        }
                    }
                },
                _ => None,
            },
            None => None,
        }
    }
}
