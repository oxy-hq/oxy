use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use minijinja::value::{Object, ObjectRepr, Value};
use tokio::runtime::Handle;

use crate::{adapters::connector::Connector, config::ConfigManager};

#[derive(Debug, Clone)]
pub struct DatabasesContext {
    cache: Arc<Mutex<HashMap<String, Value>>>,
    config: ConfigManager,
}

impl DatabasesContext {
    pub fn new(config: ConfigManager) -> Self {
        DatabasesContext {
            cache: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }
}

impl Object for DatabasesContext {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let database_key = key.as_str();
        match database_key {
            Some(database_key) => {
                let mut cache = self.cache.lock().unwrap();
                if let Some(value) = cache.get(database_key) {
                    return Some(value.clone());
                }
                match Handle::try_current() {
                    Ok(rt) => {
                        let connector = rt
                            .block_on(Connector::from_database(database_key, &self.config))
                            .ok()?;
                        let database_info = rt.block_on(connector.database_info()).ok()?;
                        let value = Value::from_serialize(database_info);
                        cache.insert(database_key.to_string(), value.clone());
                        Some(value)
                    }
                    _ => {
                        log::error!("No tokio runtime found");
                        None
                    }
                }
            }
            _ => None,
        }
    }
}
