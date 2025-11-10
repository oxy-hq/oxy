use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::SemanticLayerError;

use super::data_sources::generate_cube_config;
use super::models::{CubeCube, CubeSemanticLayerWithDataSources, CubeView};

/// Save cube semantics with data sources to directory
pub async fn save_cube_semantics(
    cube: CubeSemanticLayerWithDataSources,
    dir: &str,
) -> Result<(), SemanticLayerError> {
    // Expand tilde to home directory
    let expanded_dir = if dir.starts_with("~/") {
        let home = std::env::var("HOME").map_err(|_| {
            SemanticLayerError::IOError("Could not find HOME directory".to_string())
        })?;
        dir.replace("~", &home)
    } else {
        dir.to_string()
    };

    let dir_path = Path::new(&expanded_dir);

    // Create directory if it doesn't exist
    if !dir_path.exists() {
        fs::create_dir_all(dir_path).map_err(|e| {
            SemanticLayerError::IOError(format!(
                "Failed to create directory {}: {}",
                expanded_dir, e
            ))
        })?;
    }

    println!("Saving Cube semantics to: {}", expanded_dir);

    // Validate that encoded variables are CubeJS-safe before writing
    validate_encoded_variables(&cube)?;

    let cubes_count = cube.cubes.len();
    let views_count = cube.views.len();
    let data_sources_count = cube.data_sources.len();

    // Create model subdirectory for both cubes and views
    let model_dir = dir_path.join("model");
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir).map_err(|e| {
            SemanticLayerError::IOError(format!(
                "Failed to create model directory {}: {}",
                model_dir.display(),
                e
            ))
        })?;
    }

    // Save each cube as a separate YAML file to the model subdirectory
    for cube_def in cube.cubes.clone() {
        let filename = format!("{}.yml", cube_def.name);
        let file_path = model_dir.join(&filename);

        let cube_yaml = generate_cube_yaml(&cube_def)?;

        fs::write(&file_path, cube_yaml).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to write cube file {}: {}", filename, e))
        })?;

        println!("  📦 Created cube file: model/{}", filename);
    }

    // Save each view as a separate YAML file to the model subdirectory
    for view_def in cube.views.clone() {
        let filename = format!("{}.yml", view_def.name);
        let file_path = model_dir.join(&filename);

        let view_yaml = generate_view_yaml(&view_def)?;

        fs::write(&file_path, view_yaml).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to write view file {}: {}", filename, e))
        })?;

        println!("  📊 Created view file: model/{}", filename);
    }

    // Generate cube.js configuration file with data sources
    if !cube.data_sources.is_empty() {
        let config_file_path: PathBuf = dir_path.join("cube.js");
        let config_content = generate_cube_config(&cube.data_sources)?;

        fs::write(&config_file_path, config_content).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to write cube.js config file: {}", e))
        })?;

        println!("  ⚙️  Created cube.js configuration file");
    }

    // Save variable mappings metadata for runtime decoding
    if !cube.variable_mappings.is_empty() {
        let mappings_file_path = dir_path.join("variable_mappings.json");
        let mappings_json = serde_json::to_string_pretty(&cube.variable_mappings).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to serialize variable mappings: {}", e))
        })?;

        fs::write(&mappings_file_path, mappings_json).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to write variable mappings file: {}", e))
        })?;

        println!(
            "  🔀 Created variable mappings file with {} view mappings",
            cube.variable_mappings.len()
        );
    }

    println!(
        "✅ Successfully saved {} cubes, {} views, and {} data sources to {}",
        cubes_count, views_count, data_sources_count, expanded_dir
    );

    Ok(())
}

/// Generate Cube.js YAML code for cubes
fn generate_cube_yaml(cube: &CubeCube) -> Result<String, SemanticLayerError> {
    #[derive(Serialize)]
    struct CubeYaml {
        #[serde(skip_serializing_if = "Option::is_none")]
        sql_table: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sql: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        data_source: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        dimensions: Vec<super::models::CubeDimension>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        measures: Vec<super::models::CubeMeasure>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        joins: Vec<super::models::CubeJoin>,
        #[serde(skip_serializing_if = "HashMap::is_empty")]
        pre_aggregations: HashMap<String, serde_json::Value>,
    }

    let cube_yaml = CubeYaml {
        sql_table: cube.sql_table.clone(),
        sql: cube.sql.clone(),
        data_source: cube.data_source.clone(),
        title: cube.title.clone(),
        description: cube.description.clone(),
        dimensions: cube.dimensions.clone(),
        measures: cube.measures.clone(),
        joins: cube.joins.clone(),
        pre_aggregations: cube.pre_aggregations.clone(),
    };

    let mut yaml_content = String::new();
    yaml_content.push_str(&format!("cubes:\n  - name: {}\n", cube.name));

    let cube_yaml_str = serde_yaml::to_string(&cube_yaml).map_err(|e| {
        SemanticLayerError::IOError(format!("Failed to serialize cube to YAML: {}", e))
    })?;

    // Indent the YAML content properly
    for line in cube_yaml_str.lines() {
        if !line.trim().is_empty() {
            yaml_content.push_str(&format!("    {}\n", line));
        }
    }

    Ok(yaml_content)
}

/// Generate Cube.js YAML code for views
fn generate_view_yaml(view: &CubeView) -> Result<String, SemanticLayerError> {
    #[derive(Serialize)]
    struct ViewYaml {
        sql: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        data_source: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        dimensions: Vec<super::models::CubeDimension>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        measures: Vec<super::models::CubeMeasure>,
    }

    let view_yaml = ViewYaml {
        sql: view.sql.clone(),
        data_source: view.data_source.clone(),
        title: view.title.clone(),
        description: view.description.clone(),
        dimensions: view.dimensions.clone(),
        measures: view.measures.clone(),
    };

    let mut yaml_content = String::new();
    yaml_content.push_str(&format!("views:\n  - name: {}\n", view.name));

    let view_yaml_str = serde_yaml::to_string(&view_yaml).map_err(|e| {
        SemanticLayerError::IOError(format!("Failed to serialize view to YAML: {}", e))
    })?;

    // Indent the YAML content properly
    for line in view_yaml_str.lines() {
        if !line.trim().is_empty() {
            yaml_content.push_str(&format!("    {}\n", line));
        }
    }

    Ok(yaml_content)
}

/// Validate that encoded variables in the cube data are safe for CubeJS
fn validate_encoded_variables(
    cube: &CubeSemanticLayerWithDataSources,
) -> Result<(), SemanticLayerError> {
    // CubeJS-safe identifier regex: alphanumeric and underscores, starting with letter or underscore
    let safe_identifier =
        Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").expect("Safe identifier regex should compile");

    let encoded_var_pattern =
        Regex::new(r"__VAR_[a-zA-Z0-9_]+__").expect("Encoded variable pattern should compile");

    // Validate dimensions in cubes
    for cube_def in &cube.cubes {
        for dimension in &cube_def.dimensions {
            // Find encoded variables in the SQL
            for encoded_match in encoded_var_pattern.find_iter(&dimension.sql) {
                let encoded_var = encoded_match.as_str();
                if !safe_identifier.is_match(encoded_var) {
                    return Err(SemanticLayerError::ValidationError(format!(
                        "Encoded variable '{}' in dimension '{}' is not CubeJS-safe",
                        encoded_var, dimension.name
                    )));
                }
            }
        }

        // Validate measures in cubes
        for measure in &cube_def.measures {
            for encoded_match in encoded_var_pattern.find_iter(&measure.sql) {
                let encoded_var = encoded_match.as_str();
                if !safe_identifier.is_match(encoded_var) {
                    return Err(SemanticLayerError::ValidationError(format!(
                        "Encoded variable '{}' in measure '{}' is not CubeJS-safe",
                        encoded_var, measure.name
                    )));
                }
            }

            // Validate measure filters
            if let Some(filters) = &measure.filters {
                for filter in filters {
                    for encoded_match in encoded_var_pattern.find_iter(&filter.sql) {
                        let encoded_var = encoded_match.as_str();
                        if !safe_identifier.is_match(encoded_var) {
                            return Err(SemanticLayerError::ValidationError(format!(
                                "Encoded variable '{}' in measure filter for measure '{}' is not CubeJS-safe",
                                encoded_var, measure.name
                            )));
                        }
                    }
                }
            }
        }

        // Validate table references
        if let Some(table) = &cube_def.sql_table {
            for encoded_match in encoded_var_pattern.find_iter(table) {
                let encoded_var = encoded_match.as_str();
                if !safe_identifier.is_match(encoded_var) {
                    return Err(SemanticLayerError::ValidationError(format!(
                        "Encoded variable '{}' in table reference for cube '{}' is not CubeJS-safe",
                        encoded_var, cube_def.name
                    )));
                }
            }
        }
    }

    // Validate views
    for view in &cube.views {
        // Validate SQL queries
        for encoded_match in encoded_var_pattern.find_iter(&view.sql) {
            let encoded_var = encoded_match.as_str();
            if !safe_identifier.is_match(encoded_var) {
                return Err(SemanticLayerError::ValidationError(format!(
                    "Encoded variable '{}' in SQL for view '{}' is not CubeJS-safe",
                    encoded_var, view.name
                )));
            }
        }

        // Validate dimensions in views
        for dimension in &view.dimensions {
            for encoded_match in encoded_var_pattern.find_iter(&dimension.sql) {
                let encoded_var = encoded_match.as_str();
                if !safe_identifier.is_match(encoded_var) {
                    return Err(SemanticLayerError::ValidationError(format!(
                        "Encoded variable '{}' in dimension '{}' of view '{}' is not CubeJS-safe",
                        encoded_var, dimension.name, view.name
                    )));
                }
            }
        }

        // Validate measures in views
        for measure in &view.measures {
            for encoded_match in encoded_var_pattern.find_iter(&measure.sql) {
                let encoded_var = encoded_match.as_str();
                if !safe_identifier.is_match(encoded_var) {
                    return Err(SemanticLayerError::ValidationError(format!(
                        "Encoded variable '{}' in measure '{}' of view '{}' is not CubeJS-safe",
                        encoded_var, measure.name, view.name
                    )));
                }
            }
        }
    }

    Ok(())
}
