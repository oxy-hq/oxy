use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use oxy_shared::errors::OxyError;

use super::data_sources::generate_cube_config;
use super::models::{CubeCube, CubeSemanticLayerWithDataSources, CubeView};

/// Save cube semantics with data sources to directory
pub async fn save_cube_semantics(
    cube: CubeSemanticLayerWithDataSources,
    dir: &str,
) -> Result<(), OxyError> {
    // Expand tilde to home directory
    let expanded_dir = if dir.starts_with("~/") {
        let home = std::env::var("HOME")
            .map_err(|_| OxyError::RuntimeError("Could not find HOME directory".to_string()))?;
        dir.replace("~", &home)
    } else {
        dir.to_string()
    };

    let dir_path = Path::new(&expanded_dir);

    // Create directory if it doesn't exist
    if !dir_path.exists() {
        fs::create_dir_all(dir_path).map_err(|e| {
            OxyError::RuntimeError(format!(
                "Failed to create directory {}: {}",
                expanded_dir, e
            ))
        })?;
    }

    println!("Saving Cube semantics to: {}", expanded_dir);

    let cubes_count = cube.cubes.len();
    let views_count = cube.views.len();
    let data_sources_count = cube.data_sources.len();

    // Create model subdirectory for both cubes and views
    let model_dir = dir_path.join("model");
    if !model_dir.exists() {
        fs::create_dir_all(&model_dir).map_err(|e| {
            OxyError::RuntimeError(format!(
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
            OxyError::RuntimeError(format!("Failed to write cube file {}: {}", filename, e))
        })?;

        println!("  📦 Created cube file: model/{}", filename);
    }

    // Save each view as a separate YAML file to the model subdirectory
    for view_def in cube.views.clone() {
        let filename = format!("{}.yml", view_def.name);
        let file_path = model_dir.join(&filename);

        let view_yaml = generate_view_yaml(&view_def)?;

        fs::write(&file_path, view_yaml).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to write view file {}: {}", filename, e))
        })?;

        println!("  📊 Created view file: model/{}", filename);
    }

    // Generate cube.js configuration file with data sources
    if !cube.data_sources.is_empty() {
        let config_file_path: PathBuf = dir_path.join("cube.js");
        let config_content = generate_cube_config(&cube.data_sources)?;

        fs::write(&config_file_path, config_content).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to write cube.js config file: {}", e))
        })?;

        println!("  ⚙️  Created cube.js configuration file");
    }

    println!(
        "✅ Successfully saved {} cubes, {} views, and {} data sources to {}",
        cubes_count, views_count, data_sources_count, expanded_dir
    );

    Ok(())
}

/// Generate Cube.js YAML code for cubes
fn generate_cube_yaml(cube: &CubeCube) -> Result<String, OxyError> {
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

    let cube_yaml_str = serde_yaml::to_string(&cube_yaml)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize cube to YAML: {}", e)))?;

    // Indent the YAML content properly
    for line in cube_yaml_str.lines() {
        if !line.trim().is_empty() {
            yaml_content.push_str(&format!("    {}\n", line));
        }
    }

    Ok(yaml_content)
}

/// Generate Cube.js YAML code for views
fn generate_view_yaml(view: &CubeView) -> Result<String, OxyError> {
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

    let view_yaml_str = serde_yaml::to_string(&view_yaml)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize view to YAML: {}", e)))?;

    // Indent the YAML content properly
    for line in view_yaml_str.lines() {
        if !line.trim().is_empty() {
            yaml_content.push_str(&format!("    {}\n", line));
        }
    }

    Ok(yaml_content)
}
