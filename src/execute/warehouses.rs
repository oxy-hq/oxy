use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use minijinja::value::{Object, ObjectRepr, Value};
use tokio::runtime::Handle;

use crate::{config::model::Warehouse, connector::Connector};

#[derive(Debug, Clone)]
pub struct WarehousesContext {
    warehouses: HashMap<String, Warehouse>,
    cache: Arc<Mutex<HashMap<String, Value>>>,
}

impl WarehousesContext {
    pub fn new(warehouses: Vec<Warehouse>) -> Self {
        let warehouses = warehouses
            .into_iter()
            .map(|w| (w.name.clone(), w))
            .collect();
        WarehousesContext {
            warehouses,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn find(&self, name: &str) -> Option<&Warehouse> {
        self.warehouses.get(name)
    }
}

impl Object for WarehousesContext {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let warehouse_key = key.as_str();
        match warehouse_key {
            Some(warehouse_key) => {
                let mut cache = self.cache.lock().unwrap();
                match cache.get(warehouse_key) {
                    Some(value) => return Some(value.clone()),
                    None => {}
                }
                match (self.warehouses.get(warehouse_key), Handle::try_current()) {
                    (Some(warehouse_config), Ok(rt)) => {
                        let warehouse_info =
                            rt.block_on(Connector::new(warehouse_config).load_warehouse_info());
                        let value = Value::from_serialize(warehouse_info);
                        cache.insert(warehouse_key.to_string(), value.clone());
                        Some(value)
                    }
                    _ => {
                        log::error!("No tokio runtime found or warehouse not found");
                        None
                    }
                }
            }
            _ => None,
        }
    }
}
