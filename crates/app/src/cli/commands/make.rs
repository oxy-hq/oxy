use ::oxy::config::model::Database;
use ::oxy::config::model::DatabaseType;
use ::oxy::config::model::Defaults;
use ::oxy::config::model::DuckDB;
use ::oxy::config::model::Model;
use ::oxy::config::model::SemanticModels;
use ::oxy::config::model::{AnthropicModelConfig, GeminiModelConfig, OpenAIModelConfig};
use ::oxy::config::*;
use ::oxy::theme::*;
use ::oxy::utils::extract_csv_dimensions;
use ::oxy::utils::get_relative_path;
use model::Config;
use std::env::current_dir;
use std::path::PathBuf;
use std::process::exit;
use tokio::fs::create_dir;

use super::MakeArgs;

use ::oxy::config::constants::{ANTHROPIC_API_KEY_VAR, GEMINI_API_KEY_VAR, OPENAI_API_KEY_VAR};

struct ProjectSetup {
    file_path: String,
    output_dir: PathBuf,
    file_name: String,
    file_name_without_ext: String,
}

fn setup_project(file_path: String) -> anyhow::Result<ProjectSetup> {
    if !file_path.ends_with(".csv") {
        eprintln!("Invalid file format. Must be a CSV file.");
        exit(1);
    }

    if !std::path::Path::new(&file_path).exists() {
        eprintln!("File not found: {file_path}");
        exit(1);
    }

    let file_name: String = std::path::Path::new(&file_path)
        .file_name()
        .expect("Failed to get file name")
        .to_str()
        .expect("Failed to convert file name to string")
        .to_string();

    let file_name_without_ext = file_name.replace(".csv", "");
    let output_dir = current_dir().expect("Could not get current directory");

    Ok(ProjectSetup {
        file_path,
        output_dir,
        file_name,
        file_name_without_ext,
    })
}

async fn setup_directories(setup: &ProjectSetup) -> anyhow::Result<(PathBuf, PathBuf)> {
    let db_dir = setup.output_dir.join("db");
    let data_dir = setup.output_dir.join("data");
    create_dir(db_dir.clone()).await?;
    create_dir(data_dir.clone()).await?;
    Ok((db_dir, data_dir))
}

fn determine_model() -> (String, Model) {
    if std::env::var(GEMINI_API_KEY_VAR).is_ok() {
        let name = "gemini1.5pro".to_string();
        (
            name.clone(),
            Model::Google {
                config: GeminiModelConfig {
                    name,
                    model_ref: "gemini-1.5-pro".to_string(),
                    key_var: GEMINI_API_KEY_VAR.to_string(),
                },
            },
        )
    } else if std::env::var(ANTHROPIC_API_KEY_VAR).is_ok() {
        let name = "claude-3-7-sonnet".to_string();
        (
            name.clone(),
            Model::Anthropic {
                config: AnthropicModelConfig {
                    name,
                    model_ref: "claude-3-7-sonnet-20250219".to_string(),
                    key_var: ANTHROPIC_API_KEY_VAR.to_string(),
                    api_url: None,
                    headers: None,
                },
            },
        )
    } else if std::env::var(OPENAI_API_KEY_VAR).is_ok() {
        let name = "openai-4.1".to_string();
        (
            name.clone(),
            Model::OpenAI {
                config: OpenAIModelConfig {
                    name,
                    model_ref: "gpt-4.1".to_string(),
                    key_var: OPENAI_API_KEY_VAR.to_string(),
                    api_url: None,
                    azure: None,
                    headers: None,
                },
            },
        )
    } else {
        let name = "openai-4.1".to_string();
        (
            name.clone(),
            Model::OpenAI {
                config: OpenAIModelConfig {
                    name,
                    model_ref: "gpt-4.1".to_string(),
                    key_var: OPENAI_API_KEY_VAR.to_string(),
                    api_url: None,
                    azure: None,
                    headers: None,
                },
            },
        )
    }
}

fn create_semantic_models(
    file_path: &str,
    db_file_path: &PathBuf,
    db_dir: &PathBuf,
) -> anyhow::Result<SemanticModels> {
    use std::path::Path;

    let dimensions = extract_csv_dimensions(Path::new(file_path))
        .map_err(|e| anyhow::anyhow!("Failed to extract CSV dimensions: {e}"))?;

    Ok(SemanticModels {
        table: get_relative_path(db_file_path.clone(), db_dir.clone())?,
        database: "local".to_string(),
        dimensions,
        description: Path::new(file_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        entities: vec![],
        measures: vec![],
        database_name: String::new(),
    })
}

pub async fn handle_make_command(make_args: &MakeArgs) -> anyhow::Result<()> {
    let setup = setup_project(make_args.file.clone())?;
    let (db_dir, data_dir) = setup_directories(&setup).await?;

    // Handle database file
    let db_file_path = db_dir.join(&setup.file_name);
    if !db_file_path.exists() {
        std::fs::copy(&setup.file_path, &db_file_path)?;
        println!("Copied file to: {}", db_file_path.display());
    }

    // Create SQL file
    let sql_file_path = data_dir.join(format!("{}.sql", setup.file_name_without_ext));
    std::fs::write(
        &sql_file_path,
        format!(
            "select * from {};",
            get_relative_path(db_file_path.clone(), db_dir.clone())?
        ),
    )?;
    println!("Created SQL file: {}", sql_file_path.display());

    // Create semantic file
    let semantic_file_path = data_dir.join(format!("{}.schema.yml", setup.file_name_without_ext));
    let semantic_content = create_semantic_models(&setup.file_path, &db_file_path, &db_dir)?;
    serde_yaml::to_writer(
        std::fs::File::create(&semantic_file_path)?,
        &semantic_content,
    )?;
    println!("Created semantic file: {}", semantic_file_path.display());

    // Create config
    let (model_name, model) = determine_model();
    let config_content = Config {
        databases: vec![Database {
            name: "local".to_string(),
            database_type: DatabaseType::DuckDB(DuckDB {
                options: model::DuckDBOptions::Local {
                    file_search_path: "db/".to_string(),
                },
            }),
        }],
        defaults: Some(Defaults {
            database: Some("local".to_string()),
        }),
        models: vec![model.clone()],
        workspace_path: PathBuf::from("."),
        builder_agent: None,
        integrations: vec![],
        slack_legacy: None,
        mcp: None,
        protected_branches: None,
        base_branch: None,
        repositories: vec![],
        admins: vec![],
        pre_aggregations: None,
    };
    serde_yaml::to_writer(
        std::fs::File::create(setup.output_dir.join("config.yml"))?,
        &config_content,
    )?;

    let _ = model_name;
    let _ = semantic_file_path;
    let _ = sql_file_path;

    println!("{}", "Make command completed successfully".success());
    Ok(())
}
