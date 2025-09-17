use std::collections::HashMap;

use schemars::{
    JsonSchema,
    schema::{InstanceType, Metadata, RootSchema, SchemaObject, SingleOrVec},
};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};
use serde_json::Value;

use crate::errors::OxyError;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Variables {
    #[serde(deserialize_with = "deserialize_hash_map_value")]
    pub variables: HashMap<String, SchemaObject>,
}

impl Variables {
    pub fn extract_enum_variables(&self) -> (Vec<(String, Vec<serde_json::Value>)>, HashMap<String, serde_json::Value>) {
        let mut enum_vars = Vec::new();
        let mut non_enum_vars = HashMap::new();
        
        for (name, schema) in &self.variables {
            if let Some(enum_values) = &schema.enum_values {
                let values: Vec<serde_json::Value> = enum_values.iter().cloned().collect();
                if !values.is_empty() {
                    enum_vars.push((name.clone(), values));
                    continue;
                }
            }
            
            let default_value = schema.metadata
                .as_ref()
                .and_then(|m| m.default.clone())
                .unwrap_or(serde_json::Value::Null);
            non_enum_vars.insert(name.clone(), default_value);
        }
        
        (enum_vars, non_enum_vars)
    }

    pub fn resolve_params(
        &self,
        params: Option<HashMap<String, Value>>,
    ) -> Result<HashMap<String, Value>, OxyError> {
        match params {
            Some(params) => self.convert_params(params),
            None => Ok(self.into()),
        }
    }

    fn convert_value_to_schema_type(
        &self,
        value: &Value,
        schema: &SchemaObject,
    ) -> Result<Value, OxyError> {
        if let Some(instance_type) = &schema.instance_type {
            match instance_type {
                SingleOrVec::Single(instance_type) => {
                    Self::convert_to_single_type(value, instance_type.as_ref())
                }
                SingleOrVec::Vec(types) => {
                    // Try each type in order until one succeeds
                    for instance_type in types {
                        if let Ok(converted) = Self::convert_to_single_type(value, instance_type) {
                            return Ok(converted);
                        }
                    }
                    Err(OxyError::ArgumentError(format!(
                        "Cannot convert {value:?} to any of the allowed types: {types:?}"
                    )))
                }
            }
        } else {
            Ok(value.clone())
        }
    }

    fn convert_to_single_type(
        value: &Value,
        target_type: &InstanceType,
    ) -> Result<Value, OxyError> {
        match target_type {
            InstanceType::Integer => Self::to_integer(value),
            InstanceType::Number => Self::to_number(value),
            InstanceType::Boolean => Self::to_boolean(value),
            InstanceType::String => Self::to_string(value),
            InstanceType::Array => match value {
                Value::Array(_) => Ok(value.clone()),
                _ => Self::type_error("array", value),
            },
            InstanceType::Object => match value {
                Value::Object(_) => Ok(value.clone()),
                _ => Self::type_error("object", value),
            },
            InstanceType::Null => match value {
                Value::Null => Ok(value.clone()),
                _ => Self::type_error("null", value),
            },
        }
    }

    fn to_integer(value: &Value) -> Result<Value, OxyError> {
        match value {
            Value::String(s) => {
                let trimmed = s.trim();

                // Try parsing as integer first
                if let Ok(i) = trimmed.parse::<i64>() {
                    return Ok(Value::Number(serde_json::Number::from(i)));
                }

                // Try parsing as float and converting to int
                if let Ok(f) = trimmed.parse::<f64>() {
                    if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                        return Ok(Value::Number(serde_json::Number::from(f as i64)));
                    }
                }

                Err(OxyError::ArgumentError(format!(
                    "Cannot convert '{s}' to integer"
                )))
            }
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    Ok(value.clone())
                } else if let Some(f) = n.as_f64() {
                    if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                        Ok(Value::Number(serde_json::Number::from(f as i64)))
                    } else {
                        Err(OxyError::ArgumentError(format!(
                            "Cannot convert float {f} to integer without precision loss"
                        )))
                    }
                } else {
                    Err(OxyError::ArgumentError(format!(
                        "Invalid number format: {n:?}"
                    )))
                }
            }
            _ => Self::type_error("integer", value),
        }
    }

    fn to_number(value: &Value) -> Result<Value, OxyError> {
        match value {
            Value::String(s) => {
                let trimmed = s.trim();
                trimmed
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .ok_or_else(|| {
                        OxyError::ArgumentError(format!("Cannot convert '{s}' to number"))
                    })
            }
            Value::Number(_) => Ok(value.clone()),
            _ => Self::type_error("number", value),
        }
    }

    fn to_boolean(value: &Value) -> Result<Value, OxyError> {
        match value {
            Value::String(s) => match s.to_lowercase().trim() {
                "true" | "yes" | "1" | "on" | "y" => Ok(Value::Bool(true)),
                "false" | "no" | "0" | "off" | "n" => Ok(Value::Bool(false)),
                _ => Err(OxyError::ArgumentError(format!(
                    "Cannot convert '{s}' to boolean"
                ))),
            },
            Value::Bool(_) => Ok(value.clone()),
            Value::Number(n) => {
                let is_truthy = n
                    .as_i64()
                    .map(|i| i != 0)
                    .or_else(|| n.as_f64().map(|f| f != 0.0))
                    .ok_or_else(|| {
                        OxyError::ArgumentError(format!("Cannot convert number {n:?} to boolean"))
                    })?;
                Ok(Value::Bool(is_truthy))
            }
            _ => Self::type_error("boolean", value),
        }
    }

    fn to_string(value: &Value) -> Result<Value, OxyError> {
        let string_value = match value {
            Value::String(_) => value.clone(),
            Value::Number(n) => Value::String(n.to_string()),
            Value::Bool(b) => Value::String(b.to_string()),
            Value::Null => Value::String("null".to_string()),
            Value::Array(_) | Value::Object(_) => Value::String(
                serde_json::to_string(value)
                    .map_err(|e| OxyError::ArgumentError(format!("Serialization error: {e}")))?,
            ),
        };
        Ok(string_value)
    }

    fn type_error(expected_type: &str, actual_value: &Value) -> Result<Value, OxyError> {
        Err(OxyError::ArgumentError(format!(
            "Expected {expected_type} value, got {actual_value:?}"
        )))
    }

    // Parse YAML and directly convert to JSON Value without intermediate string conversion
    pub fn parse_yaml_to_value(yaml_str: &str) -> Result<Value, OxyError> {
        serde_yaml::from_str(yaml_str)
            .map_err(|e| OxyError::ArgumentError(format!("YAML parsing error: {e}")))
    }

    /// Process YAML document with schema validation and type conversion
    pub fn process_yaml_with_schema(
        yaml_str: &str,
        variables: &Variables,
    ) -> Result<HashMap<String, Value>, OxyError> {
        let value = Self::parse_yaml_to_value(yaml_str)?;

        let Value::Object(obj) = value else {
            return Err(OxyError::ArgumentError(
                "Expected YAML object at root level".to_string(),
            ));
        };

        variables.convert_params(obj.into_iter().collect())
    }

    /// Convert parameters using schema definitions
    fn convert_params(
        &self,
        params: HashMap<String, Value>,
    ) -> Result<HashMap<String, Value>, OxyError> {
        let mut result = HashMap::new();

        for (key, schema) in &self.variables {
            if let Some(param_value) = params.get(key) {
                let converted_value = self.convert_value_to_schema_type(param_value, schema)?;
                result.insert(key.clone(), converted_value);
            } else if let Some(metadata) = &schema.metadata {
                if let Some(default_value) = &metadata.default {
                    result.insert(key.clone(), default_value.clone());
                } else {
                    return Err(OxyError::ArgumentError(format!(
                        "Missing required variable: {key}"
                    )));
                }
            } else {
                return Err(OxyError::ArgumentError(format!(
                    "Missing required variable: {key}"
                )));
            }
        }

        Ok(result)
    }
}

pub struct Variable(SchemaObject);

// Convert Variables to default value for workflow run
impl From<&Variables> for HashMap<String, Value> {
    fn from(val: &Variables) -> Self {
        val.variables
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    v.metadata
                        .clone()
                        .unwrap_or_default()
                        .default
                        .unwrap_or(Value::Null),
                )
            })
            .collect()
    }
}

// Schema generation for Variables
impl From<Variables> for serde_json::Map<String, Value> {
    fn from(val: Variables) -> Self {
        val.variables
            .into_iter()
            .map(|(k, v)| (k, serde_json::json!(&v)))
            .collect()
    }
}

impl From<&Variables> for RootSchema {
    fn from(val: &Variables) -> Self {
        val.variables.iter().fold(
            RootSchema {
                schema: SchemaObject {
                    instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                    ..Default::default()
                },
                ..Default::default()
            },
            |mut root, (key, value)| {
                let object = root.schema.object();
                let mut value = value.clone();
                if value.metadata().default.is_some() {
                    object.required.insert(key.clone());
                }
                object.properties.insert(key.clone(), value.into());
                root
            },
        )
    }
}

impl From<&Variables> for serde_json::Value {
    fn from(val: &Variables) -> Self {
        let root_schema: RootSchema = val.into();
        serde_json::json!(&root_schema)
    }
}

impl From<Variable> for SchemaObject {
    fn from(val: Variable) -> Self {
        val.0
    }
}

impl<'de> Deserialize<'de> for Variable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer
            .deserialize_any(SchemaObjectVisitor)
            .map(Variable)
    }
}

struct SchemaObjectVisitor;

impl<'de> Visitor<'de> for SchemaObjectVisitor {
    type Value = SchemaObject;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("string or map")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            metadata: Some(Box::new(Metadata {
                default: Some(Value::String(value.to_string())),
                ..Default::default()
            })),
            ..Default::default()
        })
    }

    fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
    where
        M: de::MapAccess<'de>,
    {
        Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
    }
}

struct VariablesVisitor;

impl<'de> Visitor<'de> for VariablesVisitor {
    type Value = HashMap<String, SchemaObject>;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "a hashmap of string or schema object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut result = HashMap::new();
        while let Some((key, value)) = map.next_entry::<String, Variable>()? {
            result.insert(key, value.0);
        }
        Ok(result)
    }
}

fn deserialize_hash_map_value<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, SchemaObject>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_map(VariablesVisitor)
}
