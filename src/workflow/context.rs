use minijinja::Value;
use std::collections::{HashMap, VecDeque};

use super::table::J2Table;

#[derive(Debug, Clone)]
pub enum Output {
    Single(String),
    Multi(HashMap<String, Output>),
    Array(Vec<Output>),
    ArrayCursor(Vec<Output>),
    Table(J2Table),
}

impl Output {
    pub fn insert(&mut self, key: String, value: Output) {
        match self {
            Output::Single(_) => {
                let mut m = HashMap::default();
                m.insert(key, value);
                *self = Output::Multi(m);
            }
            Output::Multi(m) => {
                m.insert(key, value);
            }
            Output::Array(a) | Output::ArrayCursor(a) => match a.last_mut() {
                Some(last) => {
                    last.insert(key, value);
                }
                None => {
                    let mut item = Output::Multi(HashMap::default());
                    item.insert(key, value);
                    a.push(item);
                }
            },
            _ => {}
        }
    }

    pub fn escape(&self) -> Output {
        match self {
            Output::ArrayCursor(a) => Output::Array(a.clone()),
            x => x.clone(),
        }
    }
}

impl From<Output> for Value {
    fn from(val: Output) -> Self {
        match val {
            Output::Single(s) => Value::from_safe_string(s),
            Output::Multi(m) => Value::from_object(
                m.into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<HashMap<String, Value>>(),
            ),
            Output::ArrayCursor(a) => <Output as Into<Value>>::into(a.last().unwrap().to_owned()),
            Output::Array(a) => Value::from_iter(
                a.iter()
                    .map(|output| <Output as Into<Value>>::into(output.clone())),
            ),
            Output::Table(t) => Value::from_object(t),
        }
    }
}

impl From<Output> for HashMap<String, Value> {
    fn from(val: Output) -> Self {
        match val {
            Output::Multi(m) => m
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect::<HashMap<String, Value>>(),
            Output::ArrayCursor(a) => {
                <Output as Into<HashMap<String, Value>>>::into(a.last().unwrap().to_owned())
            }
            _ => HashMap::default(),
        }
    }
}

pub struct Scope {
    param: String,
    outputs: Output,
}

impl Scope {
    pub fn new(param: String) -> Self {
        Scope {
            param,
            outputs: Output::Multi(HashMap::default()),
        }
    }

    pub fn new_array() -> Self {
        Scope {
            param: String::new(),
            outputs: Output::ArrayCursor(vec![]),
        }
    }

    fn update_value(&mut self, value: String) {
        self.param = value.clone();
        match self.outputs {
            Output::Array(ref mut a) | Output::ArrayCursor(ref mut a) => {
                a.push(Output::Multi(HashMap::from_iter([(
                    "value".to_string(),
                    Output::Single(value.clone()),
                )])));
            }
            _ => {}
        }
    }

    fn escape(&self) -> Output {
        self.outputs.escape()
    }

    fn update_output(&mut self, key: String, value: Output) {
        self.outputs.insert(key, value);
    }

    fn build_j2_context(&self) -> HashMap<String, Value> {
        <Output as Into<HashMap<String, Value>>>::into(self.outputs.clone())
    }
}

pub struct ContextBuilder {
    stack: VecDeque<String>,
    scopes: HashMap<String, Scope>,
}

const ROOT_SCOPE: &str = "";

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextBuilder {
    pub fn new() -> Self {
        let mut builder = ContextBuilder {
            stack: VecDeque::new(),
            scopes: HashMap::default(),
        };
        builder.enter_scope(ROOT_SCOPE.to_string(), None);
        builder
    }

    fn enter_default_scope(&mut self, name: String, param: Option<String>, is_array: bool) {
        let value = param.unwrap_or_default();
        let scope = if is_array {
            Scope::new_array()
        } else {
            Scope::new(value.clone())
        };
        self.scopes.insert(name.clone(), scope);
        self.stack.push_back(name.clone());
    }

    pub fn enter_scope(&mut self, name: String, param: Option<String>) {
        self.enter_default_scope(name, param, false);
    }

    pub fn enter_loop_scope(&mut self, name: String) {
        self.enter_default_scope(name, None, true);
    }

    pub fn update_value(&mut self, value: String) {
        let scope = self.current_scope_mut();
        scope.update_value(value);
    }

    pub fn escape_scope(&mut self) -> Scope {
        let scope_name = self.stack.pop_back().unwrap();
        let scope = self.scopes.remove(&scope_name).unwrap();
        let parent_scope_name = self.stack.back().unwrap().clone();
        let parent_scope = self.scopes.get_mut(&parent_scope_name).unwrap();
        let outputs = scope.escape();
        parent_scope.update_output(scope_name, outputs);
        scope
    }

    pub fn add_output(&mut self, key: String, value: Output) {
        let scope = self.current_scope_mut();
        scope.update_output(key, value);
    }

    fn current_scope_mut(&mut self) -> &mut Scope {
        let scope_name = self.stack.back().unwrap();
        self.scopes.get_mut(scope_name).unwrap()
    }

    fn current_scope(&self) -> &Scope {
        let scope_name = self.stack.back().unwrap();
        self.scopes.get(scope_name).unwrap()
    }

    pub fn get_outputs(&self) -> &Output {
        &self.current_scope().outputs
    }

    pub fn build_j2_context(&self) -> Value {
        let mut context = HashMap::default();
        let mut idx: usize = 1;
        let stack_depth = self.stack.len();
        for scope_name in &self.stack {
            let scope = self.scopes.get(scope_name).unwrap();
            let mut scope_context = scope.build_j2_context();
            let param_context = HashMap::from([(
                "value".to_string(),
                Value::from_safe_string(scope.param.clone()),
            )]);
            if idx == stack_depth || idx == 1 {
                context.extend(scope_context);
                context.insert(scope_name.clone(), Value::from_object(param_context));
            } else {
                scope_context.extend(param_context);
                context.insert(scope_name.clone(), Value::from_object(scope_context));
            }
            idx += 1;
        }
        Value::from_object(context)
    }
}
