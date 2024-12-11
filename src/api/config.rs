use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_yaml::Value;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

use crate::config::model::ProjectPath;

#[axum::debug_handler]
pub async fn load_config() -> Result<impl IntoResponse, (StatusCode, String)> {
    let config_path = ProjectPath::get_path("config.yml");
    let config_content = fs::read_to_string(config_path).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read config: {}", err),
        )
    })?;

    let config: Value = serde_yaml::from_str(&config_content).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse YAML: {}", err),
        )
    })?;

    Ok(Json(config))
}

fn list_dir_contents(path: &PathBuf) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let mut contents = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = if entry.file_type()?.is_dir() {
            "dir"
        } else {
            "file"
        };
        let name = entry.file_name().into_string().unwrap_or_default();
        let mut item = serde_json::json!({
            "type": file_type,
            "name": name,
        });

        if file_type == "dir" {
            let children = list_dir_contents(&entry.path())?;
            item["children"] = serde_json::json!(children);
        }

        contents.push(item);
    }
    Ok(contents)
}

#[axum::debug_handler]
pub async fn list_project_dir_structure() -> Result<impl IntoResponse, (StatusCode, String)> {
    let project_path = ProjectPath::get();
    let dir_structure = list_dir_contents(&project_path).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list directory contents: {}", err),
        )
    })?;

    Ok(Json(dir_structure))
}
