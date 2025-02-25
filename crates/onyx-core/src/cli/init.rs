use crate::cli::model::{BigQuery, Config, DatabaseType, DuckDB};
use crate::theme::*;
use crate::utils::find_project_path;
use include_dir::{include_dir, Dir};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use super::model::{Database, Defaults, Model};

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

fn collect_databases() -> Result<Vec<Database>, InitError> {
    let mut databases = Vec::new();

    loop {
        println!("\nDatabase {}:", databases.len() + 1);

        let name = prompt_with_default("Name", "database-1")?;
        let database_type = choose_database_type()?;

        let database = Database {
            name: name.clone(),
            database_type,
            dataset: prompt_with_default("Dataset", "dbt_prod_core")?,
        };

        databases.push(database);

        if !prompt_continue("Add another database")? {
            break;
        }
    }

    Ok(databases)
}

fn choose_database_type() -> Result<DatabaseType, InitError> {
    println!("Choose database type:");
    println!("1. BigQuery");
    println!("2. DuckDB");

    loop {
        let choice = prompt_with_default("Type (1 or 2)", "1")?;
        match choice.trim() {
            "1" => {
                return Ok(DatabaseType::Bigquery(BigQuery {
                    key_path: PathBuf::from(prompt_with_default("Key path", "bigquery.key")?),
                }))
            }
            "2" => return Ok(DatabaseType::DuckDB(DuckDB {})),
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
    let project_path = find_project_path().unwrap_or_else(|_| {
        println!(
            "{}",
            "Project path not found. Using current directory.".warning()
        );
        PathBuf::new()
    });

    let config_path =
        if project_path.as_os_str().is_empty() || !project_path.join("config.yml").exists() {
            println!(
                "{}",
                "Project path is empty or config.yml does not exist. Using current directory."
                    .warning()
            );
            std::env::current_dir()
                .map_err(InitError::IoError)?
                .join("config.yml")
        } else {
            println!(
                "{}",
                format!(
                    "config.yml found in {}. Only initializing current directory.",
                    project_path.display().to_string().secondary()
                )
                .text()
            );
            project_path.join("config.yml")
        };

    if !config_path.exists() {
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

    println!("Please enter information for your databases:");
    let databases = collect_databases()?;

    println!("\nPlease enter information for your models:");
    let models = collect_models()?;

    let config = Config {
        databases,
        models,
        defaults: Some(Defaults {
            database: Some("primary_database".to_string()),
        }),
        project_path: PathBuf::new(),
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
