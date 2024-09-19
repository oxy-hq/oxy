use dirs::home_dir;
use std::fs;
use std::path::{Path, PathBuf};
use std::io::{self, Write};
use std::process::Command;
use serde_yaml;

pub fn init() -> io::Result<()> {
    // Step 1: Check for dbt-profiles.yml
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

    // Step 2: Create .onyx folder and config.yml
    let home_dir = home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))?;
    let onyx_dir = home_dir.join(".onyx");
    fs::create_dir_all(&onyx_dir)?;

    let config_path = onyx_dir.join("config.yml");
    let config_content = r#"
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
"#;
    fs::write(config_path, config_content)?;
    println!("Created .onyx/config.yml in home directory");

    // Step 3: Use cargo-generate for project scaffolding
    println!("Creating project scaffolding...");
    let output = Command::new("cargo")
        .args(&["generate", "--git", "https://github.com/dummy/repo", "--name", "onyx-project"])
        .output()?;

    if output.status.success() {
        println!("Project scaffolding created successfully");
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        println!("Failed to create project scaffolding: {}", error);
    }

    Ok(())
}
