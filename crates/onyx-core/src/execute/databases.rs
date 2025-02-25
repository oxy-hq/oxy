use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use minijinja::value::{Object, ObjectRepr, Value};
use tokio::runtime::Handle;

use crate::{
    config::model::{Config, Database},
    connector::Connector,
};

#[derive(Debug, Clone)]
pub struct DatabasesContext {
    databases: HashMap<String, Database>,
    cache: Arc<Mutex<HashMap<String, Value>>>,
    config: Config,
}

impl DatabasesContext {
    pub fn new(databases: Vec<Database>, config: Config) -> Self {
        let databases = databases.into_iter().map(|w| (w.name.clone(), w)).collect();
        DatabasesContext {
            databases,
            cache: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    pub fn find(&self, name: &str) -> Option<&Database> {
        self.databases.get(name)
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
                match (self.databases.get(database_key), Handle::try_current()) {
                    (Some(database_config), Ok(rt)) => {
                        let database_info = rt.block_on(
                            Connector::new(database_config, &self.config).load_database_info(),
                        );
                        let value = Value::from_serialize(database_info);
                        cache.insert(database_key.to_string(), value.clone());
                        Some(value)
                    }
                    _ => {
                        log::error!("No tokio runtime found or database not found");
                        None
                    }
                }
            }
            _ => None,
        }
    }
}
