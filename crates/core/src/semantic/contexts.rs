use std::{collections::HashMap, sync::Arc};

use itertools::Itertools;
use minijinja::value::Object;
use schemars::schema::{Metadata, SchemaObject};
use serde::Serialize;

use crate::config::model::{Dimension, SemanticModels, Variables};
use oxy_shared::errors::OxyError;

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
                            "Failed to convert dimension into schema object: {err}"
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
                        "Failed to convert dimension into schema object: {err}"
                    ))
                })?,
            );
        }
        if let Some(is_partition_key) = self.is_partition_key {
            schema.extensions.insert(
                "is_partition_key".to_string(),
                serde_json::to_value(is_partition_key).map_err(|err| {
                    OxyError::SerializerError(format!(
                        "Failed to convert dimension into schema object: {err}"
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
                .map(|(table_name, model)| {
                    model
                        .dimensions
                        .into_iter()
                        .map(|dim| {
                            let dim_name = dim.name.clone();
                            dim.try_into().map(|s| (dim_name, s))
                        })
                        .try_collect::<(String, SchemaObject), Vec<_>, OxyError>()
                        .map(|variables| {
                            (
                                table_name,
                                Variables {
                                    variables: variables.into_iter().collect(),
                                },
                            )
                        })
                })
                .try_collect::<(String, Variables), HashMap<String, Variables>, OxyError>()?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticDimensionsContexts {
    pub dimensions: HashMap<String, SchemaObject>,
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
        writeln!(f, "{schema_value}")
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
        "bigint" | "smallint" => InstanceType::Integer,
        "numeric" | "real" | "double precision" => InstanceType::Number,
        "varchar" | "char" | "timestamptz" => InstanceType::String,
        "jsonb" | "xml" => InstanceType::Object,
        // map mysql types to schemars instance types
        "int" | "tinyint" => InstanceType::Integer,
        "double" | "decimal" => InstanceType::Number,
        "datetime" => InstanceType::String,
        "tinyint(1)" => InstanceType::Boolean, // MySQL boolean type
        "set" | "enum" => InstanceType::Object,
        // map clickhouse types to schemars instance types
        "Int8" | "Int16" | "Int32" | "Int64" => InstanceType::Integer,
        "Float32" | "Float64" => InstanceType::Number,
        "String" | "Date" | "DateTime" => InstanceType::String,
        "Boolean" => InstanceType::Boolean,
        "Map" | "Object" => InstanceType::Object,
        "Array" => InstanceType::Array,
        // map snowflake types to schemars instance types
        "NUMBER" | "INT" | "BIGINT" | "SMALLINT" => InstanceType::Integer,
        "FLOAT" | "DOUBLE" | "DECIMAL" => InstanceType::Number,
        "VARCHAR" | "TEXT" | "TIMESTAMP_LTZ" | "TIMESTAMP_NTZ" => InstanceType::String,
        "OBJECT" | "VARIANT" => InstanceType::Object,
        // map redshift types to schemars instance types
        "INTEGER" => InstanceType::Integer,
        "REAL" | "DOUBLE PRECISION" => InstanceType::Number,
        "CHAR" | "TIMESTAMPTZ" => InstanceType::String,
        "SUPER" => InstanceType::Object,
        // map domo types to schemars instance types (LONG, BOOLEAN only - others already matched)
        "LONG" => InstanceType::Integer,
        "BOOLEAN" => InstanceType::Boolean,

        _ => InstanceType::String, // Default case for unknown types
    };

    SingleOrVec::Single(Box::new(instance_type))
}
