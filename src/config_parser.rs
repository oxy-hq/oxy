use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
pub struct EntityConfig {
    pub entities: Vec<Entity>,
    pub metrics: Vec<Metric>,
    pub time_grains: Vec<TimeGrain>,
}

#[derive(Deserialize)]
pub struct Entity {
    pub name: String,
    pub universal_key: String,
}

#[derive(Deserialize)]
pub struct Metric {
    pub name: String,
    pub sql: String,
}

#[derive(Deserialize)]
pub struct TimeGrain {
    pub name: String,
    pub universal_key: String,
}

pub fn parse_entity_config() -> Result<EntityConfig, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string("entities.yml")?;
    let config: EntityConfig = serde_yaml::from_str(&contents)?;
    Ok(config)
}

pub fn format_system_message(config: &EntityConfig, output_type: &str) -> String {
    let mut message = String::new();

    if output_type == "code" {
        message.push_str("You are a helpful assistant that responds with SQL queries. Always respond with only the SQL query, no explanations. ");
    } else {
        message.push_str("You are a helpful assistant that responds to questions about a software product. ");
    }

    message.push_str("Here's information about our data model:\n\n");

    message.push_str("Entities:\n");
    for entity in &config.entities {
        message.push_str(&format!("- {}: key = {}\n", entity.name, entity.universal_key));
    }

    message.push_str("\nCalculations:\n");
    for metric in &config.calculations {
        message.push_str(&format!("- {}: {}\n", metric.name, metric.sql));
    }

    message.push_str("\nUse this information to inform your responses.");
    message
}
