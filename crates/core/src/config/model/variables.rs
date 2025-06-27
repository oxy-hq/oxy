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
    pub fn resolve_params(
        &self,
        params: Option<HashMap<String, Value>>,
    ) -> Result<HashMap<String, Value>, OxyError> {
        match params {
            Some(params) => {
                let mut resolved = HashMap::new();
                for (key, value) in self.variables.iter() {
                    if let Some(param_value) = params.get(key) {
                        resolved.insert(key.clone(), param_value.clone());
                    } else if let Some(default_value) = &value.clone().metadata().default {
                        resolved.insert(key.clone(), default_value.clone());
                    } else {
                        return Err(OxyError::ArgumentError(format!(
                            "Missing required variable: {}",
                            key
                        )));
                    }
                }
                Ok(resolved)
            }
            None => Ok(self.into()),
        }
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
