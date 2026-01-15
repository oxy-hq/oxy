use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use minijinja::value::{Object, ObjectRepr, Value};
use tokio::runtime::Handle;

use oxy::adapters::secrets::SecretsManager;
use oxy::config::ConfigManager;
use oxy::semantic::SemanticManager;
use oxy::theme::StyledText;

#[derive(Debug, Clone)]
pub struct DatabasesContext {
    cache: Arc<Mutex<HashMap<String, Value>>>,
    database_keys: Vec<String>,
    config: ConfigManager,
    secrets_manager: SecretsManager,
}

impl DatabasesContext {
    pub fn new(config: ConfigManager, secrets_manager: SecretsManager) -> Self {
        DatabasesContext {
            cache: Arc::new(Mutex::new(HashMap::new())),
            database_keys: config
                .list_databases()
                .iter()
                .map(|db| db.name.clone())
                .collect(),
            config,
            secrets_manager,
        }
    }
}

impl Object for DatabasesContext {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let database_key = key.as_str();
        if database_key.is_none()
            || !self
                .database_keys
                .contains(&database_key.unwrap().to_string())
        {
            return None;
        }

        match database_key {
            Some(database_key) => {
                let mut cache = self.cache.lock().unwrap();
                if let Some(value) = cache.get(database_key) {
                    return Some(value.clone());
                }
                match Handle::try_current() {
                    Ok(rt) => {
                        let semantic_manager = rt
                            .block_on(SemanticManager::from_config(
                                self.config.clone(),
                                self.secrets_manager.clone(),
                                false,
                            ))
                            .ok()?;
                        let database_info =
                            match rt.block_on(semantic_manager.load_database_info(database_key)) {
                                Ok(info) => info,
                                Err(e) => {
                                    println!(
                                        "{}",
                                        format!("Failed to get database info: \n{e}\n").error()
                                    );
                                    return None;
                                }
                            };
                        let value = Value::from_serialize(database_info);
                        cache.insert(database_key.to_string(), value.clone());
                        Some(value)
                    }
                    _ => {
                        tracing::error!("No tokio runtime found");
                        None
                    }
                }
            }
            _ => None,
        }
    }
}
