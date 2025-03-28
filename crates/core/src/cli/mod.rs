mod init;

use crate::ai::agent::AgentResult;
use crate::ai::utils::record_batches_to_table;
use crate::config::*;
use crate::errors::OxyError;
use crate::execute::agent::run_agent;
use crate::execute::eval::run_eval;
use crate::execute::workflow::run_workflow;
use crate::utils::find_project_path;
use crate::utils::print_colored_sql;
use crate::workflow::WorkflowResult;
use axum::handler::Handler;
use clap::CommandFactory;
use clap::Parser;
use clap::builder::ValueParser;
use minijinja::{Environment, Value};
use model::AgentConfig;
use model::FileFormat;
use model::ToolConfig;
use model::{Config, Workflow};
use pyo3::Bound;
use pyo3::FromPyObject;
use pyo3::IntoPyObject;
use pyo3::PyAny;
use pyo3::PyErr;
use pyo3::Python;
use pyo3::types::PyAnyMethods;
use std::backtrace;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::process::exit;

use init::init;

use crate::api::router;
use crate::connector::Connector;
use crate::theme::*;
use crate::{build, vector_search};
use tower_serve_static::ServeDir;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get_service,
};

use include_dir::{Dir, include_dir};
use std::net::SocketAddr;
use tower::service_fn;

// hardcode the path for windows because of macro expansion issues
// when using CARGO_MANIFEST_DIR with windows path separators
// TODO: replace with a more robust solution, like using env DIST_DIR_PATH
#[cfg(target_os = "windows")]
static DIST: Dir = include_dir!("D:\\a\\oxy\\oxy\\crates\\core\\dist");
#[cfg(not(target_os = "windows"))]
static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/dist");

type Variable = (String, String);
fn parse_variable(env: &str) -> Result<Variable, OxyError> {
    if let Some((var, value)) = env.split_once('=') {
        Ok((var.to_owned(), value.to_owned()))
    } else {
        Err(OxyError::ArgumentError(
            "Invalid variable format. Must be in the form of VAR=VALUE".to_string(),
        ))
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// The question to ask or command to execute
    #[clap(default_value = "")]
    input: String,

    /// Output format: 'text' (default) or 'code' for SQL
    #[clap(long, value_name = "FORMAT")]
    output: Option<String>,

    /// Subcommand
    #[clap(subcommand)]
    command: Option<SubCommand>,

    /// Specify a custom agent configuration
    #[clap(long, value_name = "AGENT_NAME")]
    agent: Option<String>,
}

#[derive(Parser, Debug)]
enum SubCommand {
    /// Initialize a repository as an oxy project. Also creates a ~/.config/oxy/config.yaml file if it doesn't exist
    Init,
    /// Search through SQL in your project path. Run them against the associated database on
    /// selection.
    Run(RunArgs),
    /// Run testing on a workflow file to get consistency metrics
    Test(TestArgs),
    /// Build embeddings for hybrid search
    Build,
    /// Perform vector search
    VecSearch(VecSearchArgs),
    /// Validate the config file
    Validate,
    /// Start the API server and serve the frontend web app
    Serve,
    /// Test theme for terminal output
    TestTheme,
    /// Generate JSON schemas for config files
    GenConfigSchema(GenConfigSchemaArgs),
    /// Update the CLI to the latest version
    SelfUpdate,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    file: String,

    #[clap(long)]
    database: Option<String>,

    #[clap(long, short = 'v', value_parser=ValueParser::new(parse_variable), num_args = 1..)]
    variables: Vec<(String, String)>,

    question: Option<String>,
}

#[derive(Parser, Debug)]
pub struct TestArgs {
    file: String,
    #[clap(long, short = 'q', default_value_t = false)]
    quiet: bool,
}

#[derive(Clone)]
pub struct RunOptions {
    database: Option<String>,
    variables: Option<Vec<(String, String)>>,
    question: Option<String>,
}

impl<'py> FromPyObject<'py> for RunOptions {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> pyo3::PyResult<Self> {
        let database = ob
            .get_item("database")
            .map(|v| v.extract::<Option<String>>().unwrap_or(None))
            .unwrap_or(None);
        let variables = ob
            .get_item("variables")
            .map(|v| v.extract::<Option<Vec<(String, String)>>>().unwrap_or(None))
            .unwrap_or(None);
        let question = ob
            .get_item("question")
            .map(|v| v.extract::<Option<String>>().unwrap_or(None))
            .unwrap_or(None);
        Ok(RunOptions {
            database,
            variables,
            question,
        })
    }
}

impl RunArgs {
    pub fn from(file: String, options: Option<RunOptions>) -> Self {
        match options {
            Some(options) => Self {
                file,
                database: options.database,
                variables: options.variables.unwrap_or(vec![]),
                question: options.question,
            },
            None => Self {
                file,
                database: None,
                variables: vec![],
                question: None,
            },
        }
    }
}

#[derive(Parser, Debug)]
struct VecSearchArgs {
    question: String,
}

#[derive(Parser, Debug)]
struct GenConfigSchemaArgs {
    #[clap(long)]
    check: bool,
}

async fn handle_workflow_file(workflow_name: &PathBuf) -> Result<WorkflowResult, OxyError> {
    run_workflow(workflow_name).await
}

pub async fn cli() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    use std::panic;

    panic::set_hook(Box::new(move |panic_info| {
        log::error!(
            "{}\nTrace:\n{}",
            panic_info,
            backtrace::Backtrace::force_capture()
        );
    }));

    match args.command {
        Some(SubCommand::GenConfigSchema(args)) => {
            let schemas_path = std::path::Path::new("json-schemas");
            if !schemas_path.exists() {
                std::fs::create_dir_all(schemas_path)?;
            }

            let schemas = vec![
                (
                    "config.json",
                    serde_json::to_string_pretty(&schemars::schema_for!(Config))?,
                ),
                (
                    "workflow.json",
                    serde_json::to_string_pretty(&schemars::schema_for!(Workflow))?,
                ),
                (
                    "agent.json",
                    serde_json::to_string_pretty(&schemars::schema_for!(AgentConfig))?,
                ),
            ];

            for (filename, schema) in &schemas {
                std::fs::write(schemas_path.join(filename), schema)?;
            }

            println!("Generated schema files successfully");

            if args.check {
                let output = Command::new("git").args(["status", "--short"]).output()?;

                if !output.status.success() {
                    eprintln!(
                        "Failed to get changed files: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    exit(1);
                }

                let changed_files = String::from_utf8(output.stdout)?;
                let schema_files: Vec<String> = schemas
                    .iter()
                    .map(|(filename, _)| format!("json-schemas/{}", filename))
                    .collect();

                for file in schema_files {
                    if changed_files.contains(&file) {
                        eprintln!("Unexpected changes were found in schema files.");
                        eprintln!(
                            "Please review these changes and update the schema generation code by `cargo run gen-config-schema.`"
                        );
                        exit(1)
                    }
                }
            }
        }
        Some(SubCommand::Init) => match init() {
            Ok(_) => println!("{}", "Initialization complete.".success()),
            Err(e) => eprintln!("{}", format!("Initialization failed: {}", e).error()),
        },
        Some(SubCommand::Run(run_args)) => {
            handle_run_command(run_args).await?;
        }
        Some(SubCommand::Test(test_args)) => {
            handle_test_command(test_args).await?;
        }
        Some(SubCommand::Build) => {
            let config = ConfigBuilder::new()
                .with_project_path(&find_project_path()?)?
                .build()
                .await?;
            build(&config).await?;
        }
        Some(SubCommand::VecSearch(search_args)) => {
            let config = ConfigBuilder::new()
                .with_project_path(&find_project_path()?)?
                .build()
                .await?;
            let agent = config.resolve_agent(args.agent.unwrap()).await?;

            for tool in agent.tools {
                if let ToolConfig::Retrieval(retrieval) = tool {
                    vector_search(&agent.name, &retrieval, &search_args.question, &config).await?;
                }
            }
        }
        Some(SubCommand::Validate) => {
            let result = load_config(None);
            match result {
                Ok(config) => match config.validate_workflows() {
                    Ok(_) => match config.validate_agents() {
                        Ok(_) => println!("{}", "Config file is valid".success()),
                        Err(e) => {
                            println!("{}", e.to_string().error());
                            exit(1)
                        }
                    },
                    Err(e) => {
                        println!("{}", e.to_string().error());
                        exit(1)
                    }
                },
                Err(e) => {
                    println!("{}", e.to_string().error());
                    exit(1)
                }
            }
        }
        Some(SubCommand::Serve) => {
            start_server_and_web_app().await;
        }
        Some(SubCommand::SelfUpdate) => {
            if let Err(e) = handle_check_for_updates().await {
                log::error!("Failed to update: {}", e);
                eprintln!("{}", format!("Failed to update: {}", e).error());
                exit(1);
            }
        }
        Some(SubCommand::TestTheme) => {
            println!("Initial theme mode: {:?}", get_current_theme_mode());
            println!("True color support: {:?}", detect_true_color_support());
            println!("{}", "analysis".primary());
            println!("{}", "success".success());
            println!("{}", "warning".warning());
            eprintln!("{}", "error".error());
            println!("{}", "https://github.com/oxy-hq/oxy/".secondary());
            println!("{}", "-region".tertiary());
            println!("{}", "Viewing repository".info());
            println!("{}", "text".text());
        }

        None => {
            Args::command().print_help().unwrap();
        }
    }

    Ok(())
}

async fn handle_agent_file(
    file_path: &PathBuf,
    question: Option<String>,
) -> Result<AgentResult, OxyError> {
    let question = question.ok_or_else(|| {
        OxyError::ArgumentError("Question is required for agent files".to_string())
    })?;
    let project_path = find_project_path()?;
    let config = std::sync::Arc::new(
        ConfigBuilder::new()
            .with_project_path(&project_path)?
            .build()
            .await?,
    );
    let result = run_agent(file_path, &FileFormat::Markdown, Some(question), config).await?;
    Ok(result)
}

async fn handle_sql_file(
    file_path: &PathBuf,
    database: Option<String>,
    config: &ConfigManager,
    variables: &[(String, String)],
) -> Result<String, OxyError> {
    let database = database.ok_or_else(|| OxyError::ArgumentError("Database is required for running SQL file. Please provide the database using --database or set a default database in config.yml".to_string()))?;
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to read SQL file: {}", e)))?;
    let mut env = Environment::new();
    let mut query = content.clone();

    // Handle variable templating if variables are provided
    if !variables.is_empty() {
        env.add_template("query", &query)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to parse SQL template: {}", e)))?;
        let tmpl = env.get_template("query").unwrap();
        let ctx = Value::from({
            let mut m = BTreeMap::new();
            for var in variables {
                m.insert(var.0.clone(), var.1.clone());
            }
            m
        });
        query = tmpl
            .render(ctx)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to render SQL template: {}", e)))?
    }

    // Print colored SQL and execute query
    print_colored_sql(&query);
    let (datasets, schema) = Connector::from_database(&database, config)
        .await?
        .run_query_and_load(&query)
        .await?;
    let batches_display = record_batches_to_table(&datasets, &schema)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to display query results: {}", e)))?;
    println!("\n\x1b[1;32mResults:\x1b[0m");
    println!("{}", batches_display);

    Ok(batches_display.to_string())
}

pub enum RunResult {
    Workflow(WorkflowResult),
    Agent(AgentResult),
    Sql(String),
}

impl<'py> IntoPyObject<'py> for RunResult {
    type Target = PyAny;

    type Output = Bound<'py, Self::Target>;

    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            RunResult::Workflow(result) => {
                let output = result.into_pyobject(py)?;
                Ok(output.into_any())
            }
            RunResult::Agent(result) => {
                let output = result.into_pyobject(py)?;
                Ok(output.into_any())
            }
            RunResult::Sql(result) => Ok(result.into_pyobject(py)?.into_any()),
        }
    }
}

pub async fn handle_run_command(run_args: RunArgs) -> Result<RunResult, OxyError> {
    let file = &run_args.file;

    let current_dir = std::env::current_dir().expect("Could not get current directory");

    let file_path = current_dir.join(file);
    if !file_path.exists() {
        return Err(OxyError::ConfigurationError(format!(
            "File not found: {:?}",
            file_path
        )));
    }

    let extension = file_path.extension().and_then(std::ffi::OsStr::to_str);

    match extension {
        Some("yml") => {
            if file.ends_with(".workflow.yml") {
                let workflow_result = handle_workflow_file(&file_path).await?;
                Ok(RunResult::Workflow(workflow_result))
            } else if file.ends_with(".agent.yml") {
                let agent_result = handle_agent_file(&file_path, run_args.question).await?;
                return Ok(RunResult::Agent(agent_result));
            } else {
                return Err(OxyError::ArgumentError(
                    "Invalid YAML file. Must be either *.workflow.yml or *.agent.yml".into(),
                ));
            }
        }
        Some("sql") => {
            let config = ConfigBuilder::new()
                .with_project_path(&find_project_path()?)?
                .build()
                .await?;
            let database = run_args
                .database
                .or_else(|| config.default_database_ref().cloned());

            if database.is_none() {
                return Err(OxyError::ArgumentError(
                    "Database is required for running SQL file. Please provide the database using --database or set a default database in config.yml".into(),
                ));
            }
            let sql_result =
                handle_sql_file(&file_path, database, &config, &run_args.variables).await?;
            Ok(RunResult::Sql(sql_result))
        }
        _ => Err(OxyError::ArgumentError(
            "Invalid file extension. Must be .workflow.yml, .agent.yml, or .sql".into(),
        )),
    }
}

pub async fn handle_test_command(test_args: TestArgs) -> Result<(), OxyError> {
    let file = &test_args.file;

    let current_dir = std::env::current_dir().expect("Could not get current directory");

    let file_path = current_dir.join(file);
    if !file_path.exists() {
        return Err(OxyError::ConfigurationError(format!(
            "File not found: {:?}",
            file_path
        )));
    }
    run_eval(file_path, test_args.quiet).await
}
pub async fn start_server_and_web_app() {
    let mut web_port = 3000;
    while tokio::net::TcpListener::bind(("127.0.0.1", web_port))
        .await
        .is_err()
    {
        println!(
            "Port {} for web app is occupied. Trying next port...",
            web_port
        );
        web_port += 1;
    }

    tokio::spawn(async move {
        let serve_with_fallback = service_fn(move |req: Request<Body>| {
            async move {
                let res = get_service(ServeDir::new(&DIST))
                    .call(req, None::<()>)
                    .await;
                if res.status() == StatusCode::NOT_FOUND {
                    // If 404, fallback to serving index.html
                    let index_req = Request::builder()
                        .uri("/index.html")
                        .body(Body::empty())
                        .unwrap();
                    let response = get_service(ServeDir::new(&DIST))
                        .call(index_req, None::<()>)
                        .await;
                    Ok(response)
                } else {
                    Ok(res)
                }
            }
        });
        let fallback_service =
            get_service(ServeDir::new(&DIST).append_index_html_on_directories(true));

        let api_router = router::api_router().await;
        let web_app = Router::new()
            .nest_service("/", serve_with_fallback)
            .nest("/api", api_router)
            .fallback_service(fallback_service);

        let web_addr = SocketAddr::from(([127, 0, 0, 1], web_port));
        let listener = tokio::net::TcpListener::bind(web_addr).await.unwrap();
        println!(
            "{} {}",
            "Web app running at".text(),
            format!("http://{}", web_addr).secondary()
        );
        axum::serve(listener, web_app).await.unwrap();
    })
    .await
    .unwrap();
}

async fn handle_check_for_updates() -> Result<(), OxyError> {
    println!("{}", "Checking for updates...".info());

    let target = format!(
        "{}-{}-{}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        std::env::consts::FAMILY
    );

    let status = tokio::task::spawn_blocking(move || {
        self_update::backends::github::Update::configure()
            .repo_owner("oxy-hq")
            .repo_name("oxy")
            .bin_name(&format!("oxy-{}", target))
            .show_download_progress(true)
            .current_version(self_update::cargo_crate_version!())
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Update configuration failed: {}", e)))?
            .update()
            .map_err(|e| OxyError::RuntimeError(format!("Update failed: {}", e)))
    })
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Task join error: {}", e)))??;

    if status.updated() {
        println!(
            "{}",
            "Update successful! Restart the application.".success()
        );
    } else {
        println!("{}", "No updates available.".info());
    }
    Ok(())
}
