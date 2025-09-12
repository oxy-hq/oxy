//! Data source configuration for CubeJS
//!
//! This module handles the generation of CubeJS data source configurations
//! from Oxy database configurations.

use std::collections::{HashMap, HashSet};

use crate::{SemanticLayer, errors::SemanticLayerError};

use super::models::{CubeDataSource, CubeDataSourceConfig};

/// Generate CubeJS data sources from a semantic layer
pub async fn generate_data_sources(
    semantic_layer: &SemanticLayer,
) -> Result<Vec<CubeDataSource>, SemanticLayerError> {
    let mut data_sources = Vec::new();

    // Collect unique datasources to create data sources
    let mut unique_datasources = HashSet::new();

    // Collect datasources from views
    for view in &semantic_layer.views {
        if let Some(datasource) = &view.datasource {
            unique_datasources.insert(datasource.as_str());
        }
    }

    // Generate data sources for each unique datasource
    for datasource_name in unique_datasources {
        let data_source = create_default_data_source(datasource_name);
        data_sources.push(data_source);
    }

    Ok(data_sources)
}

/// Create a default data source when configuration is not available
fn create_default_data_source(name: &str) -> CubeDataSource {
    CubeDataSource {
        name: name.to_string(),
        data_source_type: "duckdb".to_string(), // Default to duckdb
        config: CubeDataSourceConfig {
            host: None,
            port: None,
            database: Some(name.to_string()),
            user: None,
            password: None,
            ssl: None,
            project_id: None,
            key_file: None,
            location: None,
            additional_config: HashMap::new(),
        },
    }
}

/// Generate cube.js configuration file content with data sources
pub fn generate_cube_config(data_sources: &[CubeDataSource]) -> Result<String, SemanticLayerError> {
    let template = r#"// Cube.js configuration file
// This file defines data source configurations for query generation
// Since we're not executing with CubeJS, this just provides the database types

module.exports = {
  dbType: ({ securityContext, dataSource }) => {
{{db_type_mapping}}
    return 'duckdb'; // default
  },
  driverFactory: ({ securityContext, dataSource }) => {
{{driver_factory_mapping}}
    return {
      type: 'duckdb'
    };
  }
};
"#;

    // Generate the database type mapping
    let mut db_type_mapping = String::new();
    let mut driver_factory_mapping = String::new();

    for data_source in data_sources {
        db_type_mapping.push_str(&format!(
            "    if (dataSource === \"{}\") return \"{}\";\n",
            data_source.name, data_source.data_source_type
        ));

        // Generate driver factory mapping based on database type
        let driver_type = match data_source.data_source_type.as_str() {
            "postgres" | "redshift" => "postgres",
            "mysql" => "mysql",
            "bigquery" => "bigquery",
            "snowflake" => "snowflake",
            "clickhouse" => "clickhouse",
            "duckdb" | _ => "duckdb", // Default to duckdb for unknown types
        };

        driver_factory_mapping.push_str(&format!(
            "    if (dataSource === \"{}\") return {{ type: '{}' }};\n",
            data_source.name, driver_type
        ));
    }

    // Replace the placeholders in the template
    let config_content = template
        .replace("{{db_type_mapping}}", &db_type_mapping)
        .replace("{{driver_factory_mapping}}", &driver_factory_mapping);

    Ok(config_content)
}
