use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

use crate::errors::SemanticLayerError;

/// Represents the type of an entity in the semantic layer
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    /// Primary entity representing the main subject of a view
    Primary,
    /// Foreign entity representing a reference to an entity defined in another view
    Foreign,
}

/// Represents an entity in the semantic layer
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Entity {
    /// Unique identifier for the entity within the view
    pub name: String,
    /// Type of entity (primary or foreign)
    #[serde(rename = "type")]
    pub entity_type: EntityType,
    /// Human-readable description of what this entity represents
    pub description: String,
    /// The dimension that should be used as the key for the entity
    pub key: String,
}

/// Represents the data type of a dimension
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DimensionType {
    String,
    Number,
    Date,
    Datetime,
    Boolean,
}

impl Display for DimensionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DimensionType::String => "String",
                DimensionType::Number => "Number",
                DimensionType::Date => "Date",
                DimensionType::Datetime => "Datetime",
                DimensionType::Boolean => "Boolean",
            }
        )
    }
}

/// Represents a dimension in the semantic layer
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Dimension {
    /// Unique identifier for the dimension within the view
    pub name: String,
    /// Data type of the dimension
    #[serde(rename = "type")]
    pub dimension_type: DimensionType,
    /// Human-readable description of what this dimension represents
    pub description: Option<String>,
    /// SQL expression that defines how to calculate this dimension
    pub expr: String,
    /// Example values to help users understand the dimension content
    pub samples: Option<Vec<String>>,
    /// Alternative names or terms that refer to this dimension
    pub synonyms: Option<Vec<String>>,
    /// Whether this dimension is a primary key
    pub primary_key: Option<bool>,
}

/// Represents the type of a measure aggregation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MeasureType {
    Count,
    Sum,
    Average,
    Min,
    Max,
    CountDistinct,
    Median,
    Custom,
}

impl Display for MeasureType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MeasureType::Count => "Count",
                MeasureType::Sum => "Sum",
                MeasureType::Average => "Average",
                MeasureType::Min => "Min",
                MeasureType::Max => "Max",
                MeasureType::CountDistinct => "Count Distinct",
                MeasureType::Median => "Median",
                MeasureType::Custom => "Custom",
            }
        )
    }
}

/// Represents a filter condition for measures
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MeasureFilter {
    /// SQL expression for the filter condition
    pub expr: String,
    /// Human-readable description of the filter
    pub description: Option<String>,
}

/// Represents a measure in the semantic layer
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Measure {
    /// Unique identifier for the measure within the view
    pub name: String,
    /// Type of measure aggregation
    #[serde(rename = "type")]
    pub measure_type: MeasureType,
    /// Human-readable description of what this measure represents
    pub description: Option<String>,
    /// SQL expression for the measure (required for most types, not for count)
    pub expr: Option<String>,
    /// List of filters to apply to the measure calculation
    pub filters: Option<Vec<MeasureFilter>>,
    /// Custom SQL expression (for custom type)
    pub sql: Option<String>,
    /// Sample values or example outputs to help users understand the measure
    pub samples: Option<Vec<String>>,
    /// Alternative names or terms that refer to this measure
    pub synonyms: Option<Vec<String>>,
}

/// Represents a view in the semantic layer
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct View {
    /// Unique identifier for the view within the semantic layer
    pub name: String,
    /// Human-readable description of what this view represents
    pub description: String,
    /// Display name for the view (defaults to name if not specified)
    pub label: Option<String>,
    /// Name of the datasource to use for this view
    pub datasource: Option<String>,
    /// Database table reference (required if sql not specified)
    pub table: Option<String>,
    /// Custom SQL query (required if table not specified)
    pub sql: Option<String>,
    /// List of entities that define the core objects in this view
    pub entities: Vec<Entity>,
    /// List of dimensions (attributes) available in this view
    pub dimensions: Vec<Dimension>,
    /// List of measures (aggregations) available in this view
    pub measures: Option<Vec<Measure>>,
}

/// Represents access control levels for topics
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AccessLevel {
    Public,
    Internal,
    Restricted,
}

/// Configuration for topic's retrieval by agents
/// This mirrors RouteRetrievalConfig in oxy::core
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct TopicRetrievalConfig {
    /// List of prompts that include this topic for retrieval
    #[serde(default)]
    pub include: Vec<String>,
    /// List of prompts that exclude this topic from retrieval
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Represents a topic in the semantic layer
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Topic {
    /// Unique identifier for the topic
    pub name: String,
    /// Human-readable description of the business domain
    pub description: String,
    /// List of view names included in this topic
    pub views: Vec<String>,
    /// Optional retrieval configuration for this topic
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub retrieval: Option<TopicRetrievalConfig>,
}

/// Represents the complete semantic layer configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticLayer {
    /// List of views in the semantic layer
    pub views: Vec<View>,
    /// List of topics in the semantic layer
    pub topics: Option<Vec<Topic>>,
    /// Global metadata and configuration
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl SemanticLayer {
    /// Creates a new semantic layer with the specified views and topics
    pub fn new(views: Vec<View>, topics: Option<Vec<Topic>>) -> Self {
        Self {
            views,
            topics,
            metadata: None,
        }
    }

    /// Converts the semantic layer to a tool description format
    pub fn to_tool_description(&self) -> String {
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| SemanticLayerError::ConfigurationError(e.to_string()))
            .unwrap_or_else(|_| "Failed to serialize semantic layer".to_string());

        format!(
            "Semantic Layer:\n{}\n\nThis semantic layer defines the structure and relationships of data
            within the system. It includes views, topics, and metadata that describe how data can be queried and understood.",
            yaml
        )
    }
}

/// Represents a reference to a semantic table
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticTableRef {
    /// Database name
    pub database: String,
    /// Dataset/schema name
    pub dataset: String,
    /// Table name
    pub table: String,
}

impl SemanticTableRef {
    /// Creates a new semantic table reference
    pub fn new(database: String, dataset: String, table: String) -> Self {
        Self {
            database,
            dataset,
            table,
        }
    }

    /// Returns the full table reference as a string
    pub fn table_ref(&self) -> String {
        format!("{}.{}.{}", self.database, self.dataset, self.table)
    }

    /// Returns the target reference including the specified dimension
    pub fn to_target(&self, dimension: &str) -> String {
        format!(
            "{}.{}.{}.{}",
            self.database, self.dataset, self.table, dimension
        )
    }
}

impl std::str::FromStr for SemanticTableRef {
    type Err = crate::SemanticLayerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() < 3 {
            return Err(crate::SemanticLayerError::ParsingError(format!(
                "Invalid semantic table reference format: '{}'. Expected format: 'database.dataset.table'",
                s
            )));
        }
        Ok(SemanticTableRef {
            database: parts[0].to_string(),
            dataset: parts[1].to_string(),
            table: parts[2].to_string(),
        })
    }
}
