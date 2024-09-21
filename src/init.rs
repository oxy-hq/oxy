use crate::yaml_parsers::config_parser::get_config_path;
use std::error::Error;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::result::Result;
use std::{env, fs};

pub fn init() -> Result<(), Box<dyn Error>> {
    // TODO: Step 1: Check for dbt-profiles.yml NONFUNCTIONAL
    if Path::new("dbt-profiles.yml").exists() {
        print!("dbt-profiles.yml found. Do you want to use this? (y/n): ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() == "y" {
            println!("Using existing dbt-profiles.yml");
            return Ok(());
        }
    }

    let config_path = get_config_path();

    if config_path.exists() {
        println!(
            "config.yml found in {}. Only initializing current directory.",
            config_path.display()
        );
        return Ok(());
    } else {
        // Step 2: Create .onyx folder and config.yml
        let onyx_dir = config_path
            .parent()
            .expect("Failed to get parent directory");
        fs::create_dir_all(onyx_dir)?;

        let config_content = format!(
            r#"
warehouses:
- name: primary_warehouse
    type: bigquery
    key_path: /path/to/key

models:
- name: openai-3.5
    vendor: openai
    key_var: OPENAI_API_KEY
    model_ref: gpt-3.5-turbo

defaults:
agent: default
project_path: {}
    "#,
            env::current_dir()?.display()
        );
        fs::write(&config_path, config_content)?;
        println!("Created config.yml in {}", config_path.display());
    }

    // Step 3: Use cargo-generate for project scaffolding
    println!("Creating project scaffolding...");
    let output = Command::new("cargo")
        .args(&[
            "generate",
            "--git",
            "https://github.com/onyx-hq/onyx-sample-repo",
            "--name",
            "onyx-project",
        ])
        .output()?;

    if output.status.success() {
        println!("Project scaffolding created successfully");
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        println!("Failed to create project scaffolding: {}", error);
    }

    Ok(())
}
