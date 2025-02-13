use std::{collections::HashMap, fmt::Display, sync::Arc};

use super::arrow_table::ArrowTable;
use minijinja::{
    value::{Enumerator, Object, ObjectRepr},
    Value,
};
use pyo3::{
    prelude::*,
    types::{PyDict, PyList, PyNone, PyString},
};
use pyo3_arrow::PyRecordBatch;
use serde::{Deserialize, Serialize};

pub trait ContextLookup {
    fn find(&self, key: &str) -> Option<ContextValue>;
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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
            map.set_value(key, value.clone());
        }
        map
    }
}

impl ContextLookup for Map {
    fn find(&self, key: &str) -> Option<ContextValue> {
        self.0.get(key).cloned()
    }
}

impl Object for Map {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str()?;
        self.0.get(key).map(|value| value.to_owned().into())
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Array(pub Vec<ContextValue>);

impl Array {
    pub fn project(&self, key: &str) -> Array {
        Array(
            self.0
                .iter()
                .map(|v| v.find(key))
                .filter(|v| v.is_some())
                .flat_map(|v| {
                    let output = v.unwrap();
                    match output {
                        ContextValue::Array(a) => a.0,
                        _ => vec![output],
                    }
                })
                .collect(),
        )
    }

    pub fn nested_project(&self, keys: &str) -> Vec<ContextValue> {
        let mut keys_iter = keys.split('.');
        let first_key = keys_iter.next().unwrap();
        let mut output = self.project(first_key);
        log::info!(
            "Array.nested_project: {:?} -> {:?}, key: {}",
            self,
            output,
            first_key
        );

        for key in keys_iter {
            output = output.project(key);
        }
        output.0
    }
}

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
    fn find(&self, key: &str) -> Option<ContextValue> {
        let values = self.project(key);
        if values.0.is_empty() {
            return None;
        }
        Some(ContextValue::Array(values))
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
            None => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Values(self.0.iter().map(|v| v.clone().into()).collect())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentOutput {
    pub prompt: String,
    pub output: Box<ContextValue>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ContextValue {
    None,
    Text(String),
    Map(Map),
    Array(Array),
    Table(ArrowTable),
    Agent(AgentOutput),
}

impl Display for ContextValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextValue::None => write!(f, ""),
            ContextValue::Text(s) => write!(f, "{}", s),
            ContextValue::Map(m) => write!(f, "{:?}", m),
            ContextValue::Array(a) => write!(f, "{:?}", a),
            ContextValue::Table(t) => write!(f, "{:?}", t),
            ContextValue::Agent(a) => write!(f, "{}", a.output),
        }
    }
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
            ContextValue::Agent(a) => Value::from_object(*a.output),
        }
    }
}

impl Object for ContextValue {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str()?;
        self.find(key).map(|value| value.clone().into())
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
            ContextValue::Agent(a) => Arc::new(*a.output).render(f),
        }
    }
}

impl ContextLookup for ContextValue {
    fn find(&self, key: &str) -> Option<ContextValue> {
        match self {
            ContextValue::Map(m) => m.find(key),
            ContextValue::Array(a) => a.find(key),
            _ => None,
        }
    }
}

pub fn convert_output_to_python<'py>(py: Python<'py>, output: &ContextValue) -> Bound<'py, PyAny> {
    match output {
        ContextValue::Text(s) => PyString::new(py, s).into_any(),
        ContextValue::Map(m) => {
            let dict = PyDict::new(py);
            for (k, v) in &m.0 {
                dict.set_item(k, convert_output_to_python(py, v)).unwrap();
            }
            dict.into_any()
        }
        ContextValue::Array(a) => {
            let elements = a.0.iter().map(|v| convert_output_to_python(py, v));
            let list = PyList::new(py, elements).unwrap();
            list.into_any()
        }
        ContextValue::Table(table) => {
            let mut record_batchs = vec![];
            let iterator = table.0.clone().into_iter();
            for batch in iterator {
                let rb = PyRecordBatch::new(batch);
                record_batchs.push(rb.to_pyarrow(py).unwrap());
            }
            PyList::new(py, record_batchs).unwrap().into_any()
        }
        _ => <pyo3::Bound<'_, PyNone> as Clone>::clone(&PyNone::get(py)).into_any(),
    }
}

impl<'py> IntoPyObject<'py> for ContextValue {
    type Target = PyAny;

    type Output = Bound<'py, Self::Target>;

    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let output = convert_output_to_python(py, &self);
        Ok(output)
    }
}
