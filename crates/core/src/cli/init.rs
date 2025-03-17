use crate::cli::model::{BigQuery, Config, DatabaseType, DuckDB};
use crate::config::model::Postgres;
use crate::utils::find_project_path;
use include_dir::{Dir, include_dir};
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

const IO_ERROR: &str = "IO error";
const EXTRACTION_ERROR: &str = "Extraction error";
const INVALID_CHOICE: &str = "Invalid choice. Please enter a valid number.";
const REQUIRED_FIELDS_ERROR: &str =
    "All fields are required when connection string is not specified.";

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitError::IoError(err) => write!(f, "{}: {}", IO_ERROR, err),
            InitError::ExtractionError(err) => write!(f, "{}: {}", EXTRACTION_ERROR, err),
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
static PROJECT_DIR: Dir = include_dir!("D:\\a\\oxy\\oxy\\sample_project");

#[cfg(not(target_os = "windows"))]
static PROJECT_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/sample_project");
fn prompt_with_default(prompt: &str, default: &str, info: Option<&str>) -> io::Result<String> {
    if let Some(info) = info {
        println!("\n  {}", info.info())
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
            name,
            database_type,
        };

        databases.push(database);

        if !prompt_continue("\nAdd another database")? {
            break;
        }
    }

    Ok(databases)
}

fn choose_database_type() -> Result<DatabaseType, InitError> {
    println!("\tChoose database type:");
    println!("\t\t1. DuckDB");
    println!("\t\t2. BigQuery");
    println!("\t\t3. Postgres");
    println!("\t\t4. Redshift");

    loop {
        let choice = prompt_with_default("Type (1 or 2 or ..<number>..)", "1", None)?;
        match choice.trim() {
            "1" => {
                return Ok(DatabaseType::DuckDB(DuckDB {
                    file_search_path: prompt_with_default(
                        "File search path",
                        ".db/",
                        Some("Enter the directory where your files are located."),
                    )?,
                }));
            }
            "2" => {
                return Ok(DatabaseType::Bigquery(BigQuery {
                    key_path: Some(PathBuf::from(prompt_with_default(
                        "Key path",
                        "bigquery.key",
                        None,
                    )?)),
                    dataset: prompt_with_default("Dataset", "bigquery-public-data", None)?,
                }));
            }
            "3" => {
                return Ok(DatabaseType::Postgres(collect_postgres_or_redshift_config(
                    "postgres",
                )?));
            }
            "4" => {
                return Ok(DatabaseType::Postgres(collect_postgres_or_redshift_config(
                    "redshift",
                )?));
            }
            _ => println!("  {}", INVALID_CHOICE),
        }
    }
}

fn collect_postgres_or_redshift_config(db_type: &str) -> Result<Postgres, InitError> {
    let use_connection_string_file = prompt_with_default(
        "Use connection string file? (y/N)",
        "N",
        Some("If 'N', you will be prompted for individual connection parameters."),
    )?;
    if use_connection_string_file.to_lowercase() == "y" {
        let connection_string_file = PathBuf::from(prompt_with_default(
            "Connection string file",
            &format!("{}_connection_string.txt", db_type),
            None,
        )?);

        Ok(Postgres {
            connection_string_file: Some(connection_string_file),
            host: None,
            port: None,
            user: None,
            password_file: None,
            database: None,
        })
    } else {
        let host = prompt_with_default("Host", "localhost", None)?;
        let port = prompt_with_default(
            "Port",
            if db_type == "postgres" {
                "5432"
            } else {
                "5439"
            },
            None,
        )?;
        let user = prompt_with_default(
            "User",
            if db_type == "postgres" {
                "postgres"
            } else {
                "awsuser"
            },
            None,
        )?;
        let password_file =
            PathBuf::from(prompt_with_default("Password file", "password.txt", None)?);
        let database = prompt_with_default(
            "Database",
            if db_type == "postgres" {
                "postgres"
            } else {
                "dev"
            },
            None,
        )?;

        if host.is_empty()
            || port.is_empty()
            || user.is_empty()
            || password_file.to_str().unwrap().is_empty()
            || database.is_empty()
        {
            return Err(InitError::ExtractionError(
                REQUIRED_FIELDS_ERROR.to_string(),
            ));
        }

        Ok(Postgres {
            connection_string_file: None,
            host: Some(host),
            port: Some(port),
            user: Some(user),
            password_file: Some(password_file),
            database: Some(database),
        })
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

fn ignore_sensitive_files(project_path: &Path, files: &[&Path]) -> Result<(), InitError> {
    let gitignore_path = project_path.join(".gitignore");
    let mut content = String::new();

    if gitignore_path.exists() {
        content = fs::read_to_string(&gitignore_path)?;
    }

    for file in files {
        content.push_str(&format!("{}\n", file.display()));
    }

    fs::write(&gitignore_path, content)?;
    println!(
        "{}",
        format!(
            "Updated .gitignore in {}",
            gitignore_path.display().to_string().secondary()
        )
        .text()
    );
    Ok(())
}

pub fn init() -> Result<(), InitError> {
    let project_path = find_project_path().unwrap_or_else(|_| {
        println!(
            "{}",
            "Initializing current directory as oxy project.".info()
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
    println!("Project sample files loaded successfully.");

    Ok(())
}

fn create_config_file(config_path: &Path) -> Result<(), InitError> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut sensitive_files = Vec::new();

    println!("{}", "\nDATABASE SETTINGS:".tertiary());
    let databases = collect_databases()?;

    println!("{}", "\nMODEL CONFIGURATION:".tertiary());
    let models = collect_models()?;

    // Create defaults before moving databases
    let default_database = databases.first().unwrap().name.clone();

    let config = Config {
        databases: databases.clone(),
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
        "# yaml-language-server: $schema=https://raw.githubusercontent.com/oxy-hq/oxy/refs/heads/main/json-schemas/config.json\n{}",
        yaml
    );

    fs::write(config_path, content)?;

    for database in &databases {
        if let DatabaseType::Postgres(postgres) = &database.database_type {
            if let Some(password_file) = &postgres.password_file {
                sensitive_files.push(password_file.as_path());
            }
            if let Some(connection_string_file) = &postgres.connection_string_file {
                sensitive_files.push(connection_string_file.as_path());
            }
        }
    }
    ignore_sensitive_files(&config.project_path, &sensitive_files)?;

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
