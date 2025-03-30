use crate::config::model::AgentContext;
use crate::config::model::AgentContextType;
use crate::config::model::Database;
use crate::config::model::DatabaseType;
use crate::config::model::Defaults;
use crate::config::model::Dimension;
use crate::config::model::DuckDB;
use crate::config::model::ExecuteSQLTool;
use crate::config::model::FileContext;
use crate::config::model::Model;
use crate::config::model::OutputFormat;
use crate::config::model::SemanticModelContext;
use crate::config::model::SemanticModels;
use crate::config::*;
use crate::theme::*;
use crate::utils::get_relative_path;
use csv::StringRecord;
use model::AgentConfig;
use model::Config;
use model::ToolConfig;
use std::env::current_dir;
use std::path::PathBuf;
use std::process::exit;
use tokio::fs::create_dir;

use super::MakeArgs;

const OPENAI_API_KEY_VAR: &str = "OPENAI_API_KEY";
const GEMINI_API_KEY_VAR: &str = "GEMINI_API_KEY";

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
        eprintln!("File not found: {}", file_path);
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
                name,
                model_ref: "gemini-1.5-pro".to_string(),
                key_var: GEMINI_API_KEY_VAR.to_string(),
            },
        )
    } else {
        let name = "gpt4o".to_string();
        (
            name.clone(),
            Model::OpenAI {
                name,
                model_ref: "gpt-4o".to_string(),
                key_var: OPENAI_API_KEY_VAR.to_string(),
                api_url: None,
                azure_deployment_id: None,
                azure_api_version: None,
            },
        )
    }
}

fn create_semantic_models(
    file_path: &str,
    db_file_path: &PathBuf,
    db_dir: &PathBuf,
) -> anyhow::Result<SemanticModels> {
    let mut reader = csv::Reader::from_path(file_path)?;
    let columns = reader.headers()?.to_owned();
    let mut first_row = StringRecord::new();
    reader.read_record(&mut first_row)?;

    let dimensions = columns
        .iter()
        .enumerate()
        .map(|(i, column)| Dimension {
            name: column.to_string(),
            sample: vec![first_row[i].to_string()],
            synonyms: None,
        })
        .collect();

    Ok(SemanticModels {
        table: get_relative_path(db_file_path.clone(), db_dir.clone())?,
        database: "local".to_string(),
        dimensions,
        description: "".to_string(),
        entities: vec![],
        measures: vec![],
    })
}

async fn create_agent_file(
    setup: &ProjectSetup,
    model_name: String,
    semantic_file_path: PathBuf,
    sql_file_path: PathBuf,
) -> anyhow::Result<()> {
    let agents_dir = setup.output_dir.join("agents");
    create_dir(agents_dir.clone()).await?;
    let agent_file = agents_dir.join(format!("{}.agent.yml", setup.file_name_without_ext));

    let agent_content = AgentConfig {
        name: setup.file_name_without_ext.clone(),
        tools: vec![ToolConfig::ExecuteSQL(ExecuteSQLTool {
            name: "execute_sql".to_string(),
            description: "".to_string(),
            database: "local".to_string(),
        })],
        model: model_name,
        context: Some(vec![
            AgentContext {
                name: "semantic_model".to_string(),
                context_type: AgentContextType::SemanticModel(SemanticModelContext {
                    src: get_relative_path(semantic_file_path, setup.output_dir.clone())?,
                }),
            },
            AgentContext {
                name: "sql".to_string(),
                context_type: AgentContextType::File(FileContext {
                    src: vec![get_relative_path(sql_file_path, setup.output_dir.clone())?],
                }),
            },
        ]),
        system_instructions: include_str!("../templates/agent_instructions.txt").to_string(),
        output_format: OutputFormat::Default,
        anonymize: None,
        tests: vec![],
    };

    serde_yaml::to_writer(std::fs::File::create(&agent_file)?, &agent_content)?;
    println!("Created agent file: {}", agent_file.display());
    Ok(())
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
    let semantic_file_path = data_dir.join(format!("{}.sem.yml", setup.file_name_without_ext));
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
                file_search_path: "db/".to_string(),
            }),
        }],
        defaults: Some(Defaults {
            database: Some("local".to_string()),
        }),
        models: vec![model.clone()],
        project_path: PathBuf::from("."),
    };
    serde_yaml::to_writer(
        std::fs::File::create(setup.output_dir.join("config.yml"))?,
        &config_content,
    )?;

    // Create agent file
    create_agent_file(&setup, model_name, semantic_file_path, sql_file_path).await?;

    println!("{}", "Make command completed successfully".success());
    Ok(())
}
