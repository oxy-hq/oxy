use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Debug, Clone)]
pub struct EntityConfig {
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub metrics: Vec<Metric>,
    #[serde(default)]
    pub analyses: Vec<Analysis>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Entity {
    pub name: String,
    #[serde(default)]
    pub universal_key: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Metric {
    pub name: String,
    pub sql: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Analysis {
    pub name: String,
    pub sql: String,
}

impl EntityConfig {
    pub fn format_entities(&self) -> String {
        self.entities
            .iter()
            .map(|e| format!("- {}: key = {}", e.name, e.universal_key))
            .collect::<Vec<String>>()
            .join("\n")
    }

    pub fn format_metrics(&self) -> String {
        self.metrics
            .iter()
            .map(|m| format!("- {}: {}", m.name, m.sql))
            .collect::<Vec<String>>()
            .join("\n")
    }

    pub fn format_analyses(&self) -> String {
        self.analyses
            .iter()
            .map(|a| format!("- {}: {}", a.name, a.sql))
            .collect::<Vec<String>>()
            .join("\n")
    }

    pub fn format_schema(&self) -> String {
        // TODO: Implement schema formatting logic
        "Schema information placeholder".to_string()
    }
}

pub fn parse_entity_config_from_scope(
    scope: &str,
    project_path: &PathBuf,
) -> Result<EntityConfig, Box<dyn std::error::Error>> {
    let file_path: PathBuf = project_path.join("data").join(scope).join("entities.yml");
    let contents = fs::read_to_string(file_path)?;
    let config: EntityConfig = serde_yaml::from_str(&contents)?;
    Ok(config)
}
