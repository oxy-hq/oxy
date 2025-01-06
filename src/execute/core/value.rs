use std::{collections::HashMap, sync::Arc};

use minijinja::{
    value::{Object, ObjectRepr},
    Value,
};

use super::arrow_table::ArrowTable;

pub trait ContextLookup {
    fn find(&self, key: &str) -> Option<&ContextValue>;
}

#[derive(Debug, Clone, Default)]
pub struct Map(pub HashMap<String, ContextValue>);

impl Map {
    pub fn set_value(&mut self, key: &str, value: ContextValue) {
        self.0.insert(key.to_string(), value);
    }

    pub fn get_value(&self, key: &str) -> Option<&ContextValue> {
        self.0.get(key)
    }
}

impl<'a> FromIterator<&'a (String, ContextValue)> for Map {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = &'a (String, ContextValue)>,
    {
        let mut map = Map::default();
        for (key, value) in iter {
            map.set_value(&key, value.clone());
        }
        map
    }
}

impl ContextLookup for Map {
    fn find(&self, key: &str) -> Option<&ContextValue> {
        self.0.get(key)
    }
}

impl Object for Map {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str()?;
        match self.0.get(key) {
            Some(value) => Some(value.to_owned().into()),
            None => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Array(pub Vec<ContextValue>);

impl<'a> FromIterator<&'a ContextValue> for Array {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = &'a ContextValue>,
    {
        let mut array = Array::default();
        for value in iter {
            array.0.push(value.clone());
        }
        array
    }
}

impl ContextLookup for Array {
    fn find(&self, key: &str) -> Option<&ContextValue> {
        match self.0.last() {
            Some(last) => last.find(key),
            None => None,
        }
    }
}

impl Object for Array {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Seq
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        if self.0.is_empty() {
            return None;
        }
        match self.0.last() {
            Some(last) => {
                let value: Value = last.clone().into();
                match value.get_item(key) {
                    Ok(value) => Some(value),
                    Err(_) => None,
                }
            }
            None => return None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ContextValue {
    None,
    Text(String),
    Map(Map),
    Array(Array),
    Table(ArrowTable),
}

#[derive(Debug, Clone)]
pub enum Mutation {
    NewItem { key: String, value: ContextValue },
    Upsert { key: String, value: ContextValue },
}

impl Default for ContextValue {
    fn default() -> Self {
        ContextValue::Map(Map::default())
    }
}

impl From<ContextValue> for Value {
    fn from(value: ContextValue) -> Self {
        match value {
            ContextValue::None => Value::default(),
            ContextValue::Text(s) => Value::from(s.clone()),
            ContextValue::Map(m) => Value::from_object(m),
            ContextValue::Array(a) => Value::from_object(a),
            ContextValue::Table(t) => Value::from_object(t),
        }
    }
}

impl Object for ContextValue {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str()?;
        match self.find(key) {
            Some(value) => Some(value.clone().into()),
            None => None,
        }
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    where
        Self: Sized + 'static,
    {
        match self.as_ref().clone() {
            ContextValue::None => write!(f, ""),
            ContextValue::Text(s) => write!(f, "{}", s),
            ContextValue::Map(m) => Arc::new(m).render(f),
            ContextValue::Array(a) => Arc::new(a).render(f),
            ContextValue::Table(t) => Arc::new(t).render(f),
        }
    }
}

impl ContextLookup for ContextValue {
    fn find(&self, key: &str) -> Option<&ContextValue> {
        match self {
            ContextValue::Map(m) => m.find(key),
            ContextValue::Array(a) => a.find(key),
            _ => None,
        }
    }
}
