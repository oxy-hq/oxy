use crate::cli::model::{BigQuery, Config, DuckDB, ProjectPath, WarehouseType};
use crate::theme::*;
use include_dir::{include_dir, Dir};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use super::model::{Defaults, Model, Warehouse};

#[derive(Debug)]
pub enum InitError {
    IoError(io::Error),
    ExtractionError(String),
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitError::IoError(err) => write!(f, "IO error: {}", err),
            InitError::ExtractionError(err) => write!(f, "Extraction error: {}", err),
        }
    }
}

impl From<io::Error> for InitError {
    fn from(error: io::Error) -> Self {
        InitError::IoError(error)
    }
}

// hardcode the path for windows because of macro expansion issues
// when using CARGO_MANIFEST_DIR with windows path separators
// TODO: replace with a more robust solution, like using env AGENTS_DIR_PATH
#[cfg(target_os = "windows")]
static AGENTS_DIR: Dir = include_dir!("D:\\a\\onyx\\onyx\\examples\\agents");
#[cfg(target_os = "windows")]
static DATA_DIR: Dir = include_dir!("D:\\a\\onyx\\onyx\\examples\\data");
#[cfg(target_os = "windows")]
static WORKFLOWS_DIR: Dir = include_dir!("D:\\a\\onyx\\onyx\\examples\\workflows");

#[cfg(not(target_os = "windows"))]
static AGENTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/agents");
#[cfg(not(target_os = "windows"))]
static DATA_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/data");
#[cfg(not(target_os = "windows"))]
static WORKFLOWS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/workflows");
fn prompt_with_default(prompt: &str, default: &str) -> io::Result<String> {
    print!("{} (default: {}): ", prompt, default);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_string();
    Ok(if input.is_empty() {
        default.to_string()
    } else {
        input
    })
}

fn collect_warehouses() -> Result<Vec<Warehouse>, InitError> {
    let mut warehouses = Vec::new();

    loop {
        println!("\nWarehouse {}:", warehouses.len() + 1);

        let name = prompt_with_default("Name", "warehouse-1")?;
        let warehouse_type = choose_warehouse_type()?;

        let warehouse = Warehouse {
            name: name.clone(),
            warehouse_type,
            dataset: prompt_with_default("Dataset", "dbt_prod_core")?,
        };

        warehouses.push(warehouse);

        if !prompt_continue("Add another warehouse")? {
            break;
        }
    }

    Ok(warehouses)
}

fn choose_warehouse_type() -> Result<WarehouseType, InitError> {
    println!("Choose warehouse type:");
    println!("1. BigQuery");
    println!("2. DuckDB");

    loop {
        let choice = prompt_with_default("Type (1 or 2)", "1")?;
        match choice.trim() {
            "1" => {
                return Ok(WarehouseType::Bigquery(BigQuery {
                    key_path: PathBuf::from(prompt_with_default("Key path", "bigquery.key")?),
                }))
            }
            "2" => return Ok(WarehouseType::DuckDB(DuckDB {})),
            _ => println!("Invalid choice. Please enter 1 or 2."),
        }
    }
}

fn collect_models() -> Result<Vec<Model>, InitError> {
    let mut models = Vec::new();

    loop {
        println!("\nModel {}:", models.len() + 1);
        println!("Select model type:");
        println!("1. OpenAI");
        println!("2. Ollama");

        let model_type = prompt_with_default("Type (1 or 2)", "1")?;

        let model = match model_type.as_str() {
            "1" => {
                let api_url = prompt_with_default(
                    "API URL (leave empty for default OpenAI URL)",
                    "https://api.openai.com/v1",
                )?;
                let (azure_deployment_id, azure_api_version) =
                    if api_url != "https://api.openai.com/v1" {
                        (
                            Some(prompt_with_default("Azure deployment ID", "")?),
                            Some(prompt_with_default("Azure API version", "")?),
                        )
                    } else {
                        (None, None)
                    };
                Model::OpenAI {
                    name: prompt_with_default("Name", "openai-4")?,
                    model_ref: prompt_with_default("Model reference", "gpt-4")?,
                    key_var: prompt_with_default("Key variable", "OPENAI_API_KEY")?,
                    api_url: Some(api_url),
                    azure_deployment_id,
                    azure_api_version,
                }
            }
            "2" => Model::Ollama {
                name: prompt_with_default("Name", "llama3.2")?,
                model_ref: prompt_with_default("Model reference", "llama3.2:latest")?,
                api_key: prompt_with_default("API Key", "secret")?,
                api_url: prompt_with_default("API URL", "http://localhost:11434/v1")?,
            },
            _ => {
                println!("Invalid model type selected. Using OpenAI as default.");
                Model::OpenAI {
                    name: prompt_with_default("Name", "openai-4")?,
                    model_ref: prompt_with_default("Model reference", "gpt-4")?,
                    key_var: prompt_with_default("Key variable", "OPENAI_API_KEY")?,
                    api_url: Some(prompt_with_default("API URL", "https://api.openai.com/v1")?),
                    azure_deployment_id: None,
                    azure_api_version: None,
                }
            }
        };

        models.push(model);

        if !prompt_continue("Add another model")? {
            break;
        }
    }

    Ok(models)
}

// Helper function to prompt for continuation
fn prompt_continue(message: &str) -> io::Result<bool> {
    print!("{} (y/n): ", message);
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_lowercase() == "y")
}
// Function to create and populate a directory
fn create_and_populate_directory(name: &str, dir: &Dir) -> Result<(), InitError> {
    fs::create_dir(name)?;
    dir.extract(name)
        .map_err(|e| InitError::ExtractionError(e.to_string()))?;
    println!(
        "{}",
        format!("Successfully extracted {} directory", name).success()
    );
    Ok(())
}

fn create_project_structure() -> Result<(), InitError> {
    let directories = [
        ("agents", &AGENTS_DIR),
        ("data", &DATA_DIR),
        ("workflows", &WORKFLOWS_DIR),
    ];

    for (name, dir) in directories.iter() {
        create_and_populate_directory(name, dir)?;
    }

    Ok(())
}

pub fn init() -> Result<(), InitError> {
    let config_path = ProjectPath::get_path("config.yml");

    if config_path.exists() {
        println!(
            "{}",
            format!(
                "config.yml found in {}. Only initializing current directory.",
                config_path.display().to_string().secondary()
            )
            .text()
        );
    } else {
        create_config_file(&config_path)?;
    }

    println!("{}", "Creating project scaffolding...".text());
    create_project_structure()?;
    println!("{}", "Project scaffolding created successfully".success());

    Ok(())
}

fn create_config_file(config_path: &Path) -> Result<(), InitError> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    println!("Please enter information for your warehouses:");
    let warehouses = collect_warehouses()?;

    println!("\nPlease enter information for your models:");
    let models = collect_models()?;

    let config = Config {
        warehouses,
        models,
        defaults: Defaults {
            agent: "default".to_string(),
            warehouse: Some("primary_warehouse".to_string()),
        },
    };

    let yaml =
        serde_yaml::to_string(&config).map_err(|e| InitError::ExtractionError(e.to_string()))?;

    let content = format!(
        "# yaml-language-server: $schema=https://raw.githubusercontent.com/onyx-hq/onyx-public-releases/refs/heads/main/json-schemas/config.json\n{}",
        yaml);

    fs::write(config_path, content)?;

    println!(
        "{}",
        format!(
            "Created config.yml in {}",
            config_path.display().to_string().secondary()
        )
        .text()
    );

    Ok(())
}
