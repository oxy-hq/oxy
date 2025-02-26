use crate::cli::model::{BigQuery, Config, DatabaseType, DuckDB};
use crate::theme::*;
use crate::utils::find_project_path;
use include_dir::{include_dir, Dir};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use super::model::{Database, Defaults, Model};
use crate::StyledText;

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
static PROJECT_DIR: Dir = include_dir!("D:\\a\\onyx\\onyx\\examples\\sample_project");

#[cfg(not(target_os = "windows"))]
static PROJECT_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/sample_project");
fn prompt_with_default(prompt: &str, default: &str, info: Option<&str>) -> io::Result<String> {
    match info {
        Some(info) => println!("\n  {}", info.info()),
        None => (),
    }
    print!("  {} (default: {}): ", prompt, default);
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
        let name = prompt_with_default("Name", "local", None)?;
        let database_type = choose_database_type()?;

        let database = Database {
            name: name.clone(),
            database_type,
            dataset: prompt_with_default(
                "Dataset",
                ".db/",
                Some("(For duckdb, enter the directory where your files are located.\n  For other warehouses, this is the schema.)"),
            )?,
        };

        databases.push(database);

        if !prompt_continue("\nAdd another database")? {
            break;
        }
    }

    Ok(databases)
}

fn choose_database_type() -> Result<DatabaseType, InitError> {
    println!("  Choose database type:");
    println!("    1. DuckDB");
    println!("    2. BigQuery");

    loop {
        let choice = prompt_with_default("Type (1 or 2)", "1", None)?;
        match choice.trim() {
            "1" => return Ok(DatabaseType::DuckDB(DuckDB {})),
            "2" => {
                return Ok(DatabaseType::Bigquery(BigQuery {
                    key_path: PathBuf::from(prompt_with_default("Key path", "bigquery.key", None)?),
                }))
            }
            _ => println!("  Invalid choice. Please enter 1 or 2."),
        }
    }
}

fn collect_models() -> Result<Vec<Model>, InitError> {
    let mut models = Vec::new();

    loop {
        println!("  Select model type:");
        println!("  1. OpenAI");
        println!("  2. Ollama");

        let model_type = prompt_with_default("Type (1 or 2)", "1", None)?;

        let model = match model_type.as_str() {
            "1" => {
                let api_url = prompt_with_default(
                    "API URL (leave empty for default OpenAI URL)",
                    "https://api.openai.com/v1",
                    None,
                )?;
                let (azure_deployment_id, azure_api_version) =
                    if api_url != "https://api.openai.com/v1" {
                        (
                            Some(prompt_with_default("Azure deployment ID", "", None)?),
                            Some(prompt_with_default("Azure API version", "", None)?),
                        )
                    } else {
                        (None, None)
                    };
                Model::OpenAI {
                    name: prompt_with_default("Name", "openai-4o", None)?,
                    model_ref: prompt_with_default("Model reference", "gpt-4o", None)?,
                    key_var: prompt_with_default("Key variable", "OPENAI_API_KEY", None)?,
                    api_url: Some(api_url),
                    azure_deployment_id,
                    azure_api_version,
                }
            }
            "2" => Model::Ollama {
                name: prompt_with_default("Name", "llama3.2", None)?,
                model_ref: prompt_with_default("Model reference", "llama3.2:latest", None)?,
                api_key: prompt_with_default("API Key", "secret", None)?,
                api_url: prompt_with_default("API URL", "http://localhost:11434/v1", None)?,
            },
            _ => {
                println!("Invalid model type selected. Using OpenAI as default.");
                Model::OpenAI {
                    name: prompt_with_default("Name", "openai-4o", None)?,
                    model_ref: prompt_with_default("Model reference", "gpt-4o", None)?,
                    key_var: prompt_with_default("Key variable", "OPENAI_API_KEY", None)?,
                    api_url: Some(prompt_with_default(
                        "API URL",
                        "https://api.openai.com/v1",
                        None,
                    )?),
                    azure_deployment_id: None,
                    azure_api_version: None,
                }
            }
        };

        models.push(model);

        if !prompt_continue("\nAdd another model")? {
            break;
        }
    }

    Ok(models)
}

// Helper function to prompt for continuation
fn prompt_continue(message: &str) -> io::Result<bool> {
    print!("{} (y/N): ", message);
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_lowercase() == "y")
}
// Function to create and populate a directory
fn create_and_populate_directory(name: &str, dir: &Dir) -> Result<(), InitError> {
    fs::create_dir_all(name)?;
    dir.extract(name)
        .map_err(|e| InitError::ExtractionError(e.to_string()))?;
    Ok(())
}

fn create_project_structure() -> Result<(), InitError> {
    let directories = [("./", &PROJECT_DIR)];

    for (name, dir) in directories.iter() {
        create_and_populate_directory(name, dir)?;
    }

    Ok(())
}

pub fn init() -> Result<(), InitError> {
    let project_path = find_project_path().unwrap_or_else(|_| {
        println!(
            "{}",
            "Initializing current directory as onyx project.".info()
        );
        PathBuf::new()
    });

    let config_path =
        if project_path.as_os_str().is_empty() || !project_path.join("config.yml").exists() {
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

    create_project_structure()?;
    println!("{}", "Project sample files loaded successfully.");

    Ok(())
}

fn create_config_file(config_path: &Path) -> Result<(), InitError> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    println!("{}", "\nDATABASE SETTINGS:".tertiary());
    let databases = collect_databases()?;

    println!("{}", "\nMODEL CONFIGURATION:".tertiary());
    let models = collect_models()?;

    // Create defaults before moving warehouses
    let default_database = databases.first().unwrap().name.clone();

    let config = Config {
        databases,
        models,
        defaults: Some(Defaults {
            database: Some(default_database),
            agent: Some("sql-generator.agent.yml".to_string()),
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
