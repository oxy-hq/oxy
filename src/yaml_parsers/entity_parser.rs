use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
pub struct EntityConfig {
    pub entities: Vec<Entity>,
    pub calculations: Vec<Calculation>,
}

#[derive(Deserialize)]
pub struct Entity {
    pub name: String,
    pub universal_key: String,
}

#[derive(Deserialize)]
pub struct Calculation {
    pub name: String,
    pub sql: String,
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
        message.push_str("You are a helpful assistant that responds to questions. ");
    }

    message.push_str("Here's information about our data model:\n\n");

    message.push_str("Entities:\n");
    for entity in &config.entities {
        message.push_str(&format!("- {}: key = {}\n", entity.name, entity.universal_key));
    }

    message.push_str("\nCalculations:\n");
    for calculation in &config.calculations {
        message.push_str(&format!("- {}: {}\n", calculation.name, calculation.sql));
    }

    message.push_str("\nUse this information to inform your responses.");
    message
}
