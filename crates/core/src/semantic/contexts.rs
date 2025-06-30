use std::{collections::HashMap, str::FromStr, sync::Arc};

use itertools::Itertools;
use minijinja::value::Object;
use schemars::schema::{Metadata, SchemaObject};
use serde::Serialize;

use crate::{
    config::model::{Dimension, SemanticDimension, SemanticModels, Variables},
    errors::OxyError,
};

use super::types::SemanticTableRef;

#[derive(Debug, Clone, Serialize)]
pub struct SemanticContexts {
    models: HashMap<String, SemanticModels>,
}

impl SemanticContexts {
    pub fn new(models: HashMap<String, SemanticModels>) -> Self {
        SemanticContexts { models }
    }
}

impl Object for SemanticModels {
    fn get_value(
        self: &Arc<Self>,
        key: &minijinja::value::Value,
    ) -> Option<minijinja::value::Value> {
        let key = key.as_str()?;
        self.dimensions
            .iter()
            .find(|dim| dim.name == key)
            .map(minijinja::value::Value::from_serialize)
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    where
        Self: Sized + 'static,
    {
        writeln!(f, "{:?}", self)
    }
}

impl Object for SemanticContexts {
    fn get_value(
        self: &Arc<Self>,
        key: &minijinja::value::Value,
    ) -> Option<minijinja::value::Value> {
        let key = key.as_str()?;
        tracing::info!("Fetching semantic model for key: {}", key);
        tracing::info!("Available models: {:?}", self.models.keys());
        self.models
            .get(key)
            .map(|entity| minijinja::value::Value::from_object(entity.clone()))
    }
}

impl TryInto<SchemaObject> for Dimension {
    type Error = OxyError;

    fn try_into(self) -> Result<SchemaObject, Self::Error> {
        let mut schema = SchemaObject::default();
        if let Some(data_type) = self.data_type {
            schema.instance_type = Some(map_instance_type(&data_type));
        }
        schema.metadata = Some(Box::new(Metadata {
            title: Some(self.name),
            description: self.description,
            examples: self
                .sample
                .into_iter()
                .map(|v| {
                    serde_json::to_value(v).map_err(|err| {
                        OxyError::SerializerError(format!(
                            "Failed to convert dimension into schema object: {}",
                            err
                        ))
                    })
                })
                .try_collect::<serde_json::Value, Vec<_>, OxyError>()?,
            ..Default::default()
        }));
        if let Some(synonyms) = self.synonyms {
            schema.extensions.insert(
                "synonyms".to_string(),
                serde_json::to_value(synonyms).map_err(|err| {
                    OxyError::SerializerError(format!(
                        "Failed to convert dimension into schema object: {}",
                        err
                    ))
                })?,
            );
        }
        if let Some(is_partition_key) = self.is_partition_key {
            schema.extensions.insert(
                "is_partition_key".to_string(),
                serde_json::to_value(is_partition_key).map_err(|err| {
                    OxyError::SerializerError(format!(
                        "Failed to convert dimension into schema object: {}",
                        err
                    ))
                })?,
            );
        }
        Ok(schema)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticVariablesContexts {
    variables: HashMap<String, Variables>,
}

impl SemanticVariablesContexts {
    pub fn new(models: HashMap<String, SemanticModels>) -> Result<Self, OxyError> {
        Ok(SemanticVariablesContexts {
            variables: models
                .into_iter()
                .map(|(name, model)| {
                    model
                        .dimensions
                        .into_iter()
                        .map(|dim| {
                            let name = dim.name.clone();
                            dim.try_into().map(|s| (name, s))
                        })
                        .try_collect::<(String, SchemaObject), Vec<_>, OxyError>()
                        .map(|variables| {
                            (
                                name,
                                Variables {
                                    variables: variables.into_iter().collect(),
                                },
                            )
                        })
                })
                .try_collect::<(String, Variables), HashMap<String, Variables>, OxyError>()?,
        })
    }

    pub fn get_base_schema(&self, target: &str) -> Option<&SchemaObject> {
        let dim = target.split('.').next_back()?;
        let table_ref = SemanticTableRef::from_str(target).ok()?;
        match self
            .variables
            .get(&table_ref.table)
            .and_then(|v| v.variables.get(dim))
        {
            Some(variable) => Some(variable),
            None => {
                tracing::warn!(
                    "Semantic variable '{}' not found in table '{:?}'",
                    dim,
                    table_ref
                );
                None
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticDimensionsContexts {
    pub dimensions: HashMap<String, SchemaObject>,
}

impl SemanticDimensionsContexts {
    pub fn new(
        dimensions: HashMap<String, SemanticDimension>,
        model_contexts: &SemanticVariablesContexts,
    ) -> Self {
        // Override the dimensions with the variables from the model contexts
        let dimensions = dimensions
            .into_iter()
            .map(|(name, dim)| {
                let mut schema: SchemaObject = dim.schema;
                if let Some(target) = dim.targets.first() {
                    // If the dimension has targets, merge the schema with the target variable
                    if let Some(variable) = model_contexts.get_base_schema(target) {
                        let mut override_schema = variable.clone();
                        if let Some(instance_type) = schema.instance_type {
                            override_schema.instance_type = Some(instance_type);
                        }
                        if let Some(metadata) = schema.metadata {
                            override_schema.metadata = Some(metadata);
                        }
                        if let Some(format) = schema.format {
                            override_schema.format = Some(format);
                        }
                        if let Some(enum_values) = schema.enum_values {
                            override_schema.enum_values = Some(enum_values);
                        }
                        if let Some(const_value) = schema.const_value {
                            override_schema.const_value = Some(const_value);
                        }
                        if let Some(subschemas) = schema.subschemas {
                            override_schema.subschemas = Some(subschemas);
                        }
                        if let Some(number) = schema.number {
                            override_schema.number = Some(number);
                        }
                        if let Some(string) = schema.string {
                            override_schema.string = Some(string);
                        }
                        if let Some(array) = schema.array {
                            override_schema.array = Some(array);
                        }
                        if let Some(object) = schema.object {
                            override_schema.object = Some(object);
                        }
                        if let Some(reference) = schema.reference {
                            override_schema.reference = Some(reference);
                        }
                        override_schema.extensions = schema.extensions;
                        schema = override_schema;
                    }
                }
                (name, schema)
            })
            .collect::<HashMap<String, SchemaObject>>();
        SemanticDimensionsContexts { dimensions }
    }
}

impl Object for Variables {
    fn get_value(
        self: &Arc<Self>,
        key: &minijinja::value::Value,
    ) -> Option<minijinja::value::Value> {
        let key = key.as_str()?;
        self.variables
            .get(key)
            .map(|variable| minijinja::value::Value::from_serialize(serde_json::json!(variable)))
    }

    fn render(self: &Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    where
        Self: Sized + 'static,
    {
        let schema_value: serde_json::Value = self.as_ref().into();
        writeln!(f, "{}", schema_value)
    }
}

impl Object for SemanticVariablesContexts {
    fn get_value(
        self: &Arc<Self>,
        key: &minijinja::value::Value,
    ) -> Option<minijinja::value::Value> {
        let key = key.as_str()?;
        tracing::info!("Fetching semantic variable for key: {}", key);
        tracing::info!("Available variables: {:?}", self.variables.keys());
        self.variables
            .get(key)
            .map(|variable| minijinja::value::Value::from_object(variable.clone()))
    }
}

impl Object for SemanticDimensionsContexts {
    fn get_value(
        self: &Arc<Self>,
        key: &minijinja::value::Value,
    ) -> Option<minijinja::value::Value> {
        let key = key.as_str()?;
        tracing::info!("Fetching semantic dimension for key: {}", key);
        tracing::info!("Available dimensions: {:?}", self.dimensions.keys());
        self.dimensions
            .get(key)
            .map(|variable| minijinja::value::Value::from_serialize(serde_json::json!(variable)))
    }
}

fn map_instance_type(
    instance_type: &str,
) -> schemars::schema::SingleOrVec<schemars::schema::InstanceType> {
    use schemars::schema::{InstanceType, SingleOrVec};

    let instance_type = match instance_type {
        // map duckdb types to schemars instance types
        "integer" => InstanceType::Integer,
        "float" => InstanceType::Number,
        "text" | "date" | "timestamp" | "uuid" => InstanceType::String,
        "boolean" => InstanceType::Boolean,
        "json" | "object" => InstanceType::Object,
        "array" => InstanceType::Array,
        // map bigquery types to schemars instance types
        "INT64" => InstanceType::Integer,
        "FLOAT64" => InstanceType::Number,
        "STRING" | "DATE" | "DATETIME" | "TIMESTAMP" | "UUID" => InstanceType::String,
        "BOOL" => InstanceType::Boolean,
        "JSON" | "STRUCT" => InstanceType::Object,
        "ARRAY" => InstanceType::Array,
        // map postgres types to schemars instance types
        "bigint" | "integer" | "smallint" => InstanceType::Integer,
        "numeric" | "real" | "double precision" => InstanceType::Number,
        "text" | "varchar" | "char" | "date" | "timestamp" | "timestamptz" | "uuid" => {
            InstanceType::String
        }
        "boolean" => InstanceType::Boolean,
        "json" | "jsonb" | "xml" => InstanceType::Object,
        "array" => InstanceType::Array,
        // map mysql types to schemars instance types
        "int" | "bigint" | "smallint" | "tinyint" => InstanceType::Integer,
        "float" | "double" | "decimal" => InstanceType::Number,
        "char" | "varchar" | "text" | "date" | "datetime" | "timestamp" | "uuid" => {
            InstanceType::String
        }
        "tinyint(1)" => InstanceType::Boolean, // MySQL boolean type
        "json" | "set" | "enum" => InstanceType::Object,
        "array" => InstanceType::Array,
        // map clickhouse types to schemars instance types
        "Int8" | "Int16" | "Int32" | "Int64" => InstanceType::Integer,
        "Float32" | "Float64" => InstanceType::Number,
        "String" | "Date" | "DateTime" | "UUID" => InstanceType::String,
        "Boolean" => InstanceType::Boolean,
        "JSON" | "Map" | "Object" => InstanceType::Object,
        "Array" => InstanceType::Array,
        // map snowflake types to schemars instance types
        "NUMBER" | "INT" | "BIGINT" | "SMALLINT" => InstanceType::Integer,
        "FLOAT" | "DOUBLE" | "DECIMAL" => InstanceType::Number,
        "STRING" | "VARCHAR" | "TEXT" | "DATE" | "TIMESTAMP_LTZ" | "TIMESTAMP_NTZ" | "UUID" => {
            InstanceType::String
        }
        "BOOLEAN" => InstanceType::Boolean,
        "OBJECT" | "VARIANT" => InstanceType::Object,
        "ARRAY" => InstanceType::Array,
        // map redshift types to schemars instance types
        "SMALLINT" | "INTEGER" | "BIGINT" => InstanceType::Integer,
        "REAL" | "DOUBLE PRECISION" | "DECIMAL" => InstanceType::Number,
        "CHAR" | "VARCHAR" | "DATE" | "TIMESTAMP" | "TIMESTAMPTZ" | "UUID" => InstanceType::String,
        "BOOLEAN" => InstanceType::Boolean,
        "JSON" | "SUPER" => InstanceType::Object,
        "ARRAY" => InstanceType::Array,

        _ => InstanceType::String, // Default case for unknown types
    };

    SingleOrVec::Single(Box::new(instance_type))
}
