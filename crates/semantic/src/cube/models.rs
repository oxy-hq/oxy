use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root structure for CubeJS semantic layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeSemanticLayer {
    pub cubes: Vec<CubeCube>,
    pub views: Vec<CubeView>,
}

/// CubeJS cube definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeCube {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub dimensions: Vec<CubeDimension>,
    pub measures: Vec<CubeMeasure>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub joins: Vec<CubeJoin>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub pre_aggregations: HashMap<String, serde_json::Value>,
}

/// CubeJS view definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeView {
    pub name: String,
    pub sql: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub dimensions: Vec<CubeDimension>,
    pub measures: Vec<CubeMeasure>,
}

/// CubeJS dimension definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeDimension {
    pub name: String,
    pub sql: String,
    #[serde(rename = "type")]
    pub dimension_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_key: Option<bool>,
}

/// CubeJS measure definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeMeasure {
    pub name: String,
    pub sql: String,
    #[serde(rename = "type")]
    pub measure_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// CubeJS join definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeJoin {
    pub name: String,
    pub sql: String,
    pub relationship: String,
}

/// CubeJS data source definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeDataSource {
    pub name: String,
    #[serde(rename = "type")]
    pub data_source_type: String,
    pub config: CubeDataSourceConfig,
}

/// CubeJS data source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeDataSourceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssl: Option<bool>,
    // BigQuery specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    // Other database specific fields can be added here
    #[serde(flatten)]
    pub additional_config: HashMap<String, serde_json::Value>,
}

/// Complete CubeJS semantic layer with data sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeSemanticLayerWithDataSources {
    pub cubes: Vec<CubeCube>,
    pub views: Vec<CubeView>,
    pub data_sources: Vec<CubeDataSource>,
}
