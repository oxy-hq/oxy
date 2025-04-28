use std::{path::PathBuf, str::FromStr, sync::Arc};

use minijinja::{
    Value,
    value::{Enumerator, Object, ObjectRepr},
};
use serde::{Deserialize, Serialize};

use crate::errors::OxyError;

use super::{
    Prompt, SQL, Table,
    output_container::{Data, DataContainer},
    table::TableReference,
};

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub enum Output {
    Bool(bool),
    Text(String),
    SQL(SQL),
    Table(Table),
    Prompt(Prompt),
}

impl Output {
    pub fn table(file_path: String) -> Self {
        Output::Table(Table::new(file_path))
    }

    pub fn table_with_reference(file_path: String, reference: TableReference) -> Self {
        Output::Table(Table::with_reference(file_path, reference))
    }

    pub fn sql(sql: String) -> Self {
        Output::SQL(SQL::new(sql))
    }

    pub fn prompt(prompt: String) -> Self {
        Output::Prompt(Prompt::new(prompt))
    }

    pub fn merge(&self, other: &Self) -> Self {
        match (self, other) {
            (Output::Text(text1), Output::Text(text2)) => {
                Output::Text(format!("{}{}", text1, text2))
            }
            _ => other.clone(),
        }
    }

    pub fn replace(&mut self, text: String) {
        match self {
            Output::Text(t) => *t = text,
            Output::SQL(t) => t.0 = text,
            Output::Prompt(t) => t.0 = text,
            Output::Table(t) => t.file_path = text,
            _ => {}
        }
    }

    pub fn to_data(&self, file_path: &PathBuf) -> Result<Data, OxyError> {
        match self {
            Output::Text(text) => Ok(Data::Text(text.to_owned())),
            Output::SQL(sql) => Ok(Data::Text(sql.to_string())),
            Output::Table(table) => Ok(Data::Table(table.to_data(file_path)?)),
            Output::Bool(b) => Ok(Data::Bool(*b)),
            Output::Prompt(prompt) => Ok(Data::Text(prompt.to_string())),
        }
    }
}

impl From<SQL> for Output {
    fn from(sql: SQL) -> Self {
        Output::SQL(sql)
    }
}

impl From<Table> for Output {
    fn from(table: Table) -> Self {
        Output::Table(table)
    }
}

impl From<Prompt> for Output {
    fn from(prompt: Prompt) -> Self {
        Output::Prompt(prompt)
    }
}

impl FromStr for Output {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Output::Text(s.to_string()))
    }
}

impl From<bool> for Output {
    fn from(b: bool) -> Self {
        Output::Bool(b)
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Output::Text(text) => write!(f, "{}", text),
            Output::SQL(sql) => write!(f, "{}", sql),
            Output::Table(table) => write!(f, "{}", table),
            Output::Prompt(prompt) => write!(f, "{}", prompt),
            Output::Bool(b) => write!(f, "{}", b),
        }
    }
}

impl Object for Output {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        match self.as_ref() {
            Output::Table(table) => Object::repr(&Arc::new(table.clone())),
            _ => ObjectRepr::Plain,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        match self.repr() {
            ObjectRepr::Plain => Enumerator::NonEnumerable,
            ObjectRepr::Iterable | ObjectRepr::Map | ObjectRepr::Seq => match self.as_ref() {
                Output::Table(table) => Arc::new(table.clone()).enumerate(),
                _ => Enumerator::Empty,
            },
            _ => Enumerator::Empty,
        }
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match self.as_ref() {
            Output::Table(table) => Arc::new(table.clone()).get_value(key),
            _ => None,
        }
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    where
        Self: Sized + 'static,
    {
        match self.as_ref() {
            Output::Text(text) => write!(f, "{}", text),
            Output::SQL(sql) => write!(f, "{:?}", sql),
            Output::Table(table) => Arc::new(table.clone()).render(f),
            Output::Prompt(prompt) => write!(f, "{:?}", prompt),
            Output::Bool(b) => write!(f, "{}", b),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Chunk {
    pub key: Option<String>,
    pub delta: Output,
    pub finished: bool,
}
