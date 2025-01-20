mod init;

use crate::ai::agent::AgentResult;
use crate::ai::utils::record_batches_to_table;
use crate::config::*;
use crate::errors::OnyxError;
use crate::execute::agent::run_agent;
use crate::execute::workflow::run_workflow;
use crate::utils::print_colored_sql;
use crate::workflow::WorkflowResult;
use axum::handler::Handler;
use clap::builder::ValueParser;
use clap::CommandFactory;
use clap::Parser;
use minijinja::{Environment, Value};
use model::AgentConfig;
use model::FileFormat;
use model::ProjectPath;
use model::ToolConfig;
use model::{Config, Workflow};
use pyo3::types::PyAnyMethods;
use pyo3::Bound;
use pyo3::FromPyObject;
use pyo3::IntoPyObject;
use pyo3::PyAny;
use pyo3::PyErr;
use pyo3::Python;
use std::backtrace;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::process::exit;
use std::process::Command;

use init::init;

use crate::api::server;
use crate::connector::Connector;
use crate::theme::*;
use crate::{build, vector_search};
use tower_serve_static::ServeDir;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get_service,
    Router,
};

use include_dir::{include_dir, Dir};
use std::net::SocketAddr;
use tower::service_fn;

// hardcode the path for windows because of macro expansion issues
// when using CARGO_MANIFEST_DIR with windows path separators
// TODO: replace with a more robust solution, like using env DIST_DIR_PATH
#[cfg(target_os = "windows")]
static DIST: Dir = include_dir!("D:\\a\\onyx\\onyx\\dist");
#[cfg(not(target_os = "windows"))]
static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/dist");

type Variable = (String, String);
fn parse_variable(env: &str) -> Result<Variable, OnyxError> {
    if let Some((var, value)) = env.split_once('=') {
        Ok((var.to_owned(), value.to_owned()))
    } else {
        Err(OnyxError::ArgumentError(
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
    /// Initialize a repository as an onyx project. Also creates a ~/.config/onyx/config.yaml file if it doesn't exist
    Init,
    ListDatasets,
    ListTables,
    /// Search through SQL in your project path. Run them against the associated warehouse on
    /// selection.
    Run(RunArgs),
    Build,
    VecSearch(VecSearchArgs),
    Validate,
    Serve,
    TestTheme,
    GenConfigSchema(GenConfigSchemaArgs),
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    file: String,

    #[clap(long)]
    warehouse: Option<String>,

    #[clap(long, short = 'v', value_parser=ValueParser::new(parse_variable), num_args = 1..)]
    variables: Vec<(String, String)>,

    question: Option<String>,
}

#[derive(Clone)]
pub struct RunOptions {
    warehouse: Option<String>,
    variables: Option<Vec<(String, String)>>,
    question: Option<String>,
}

impl<'py> FromPyObject<'py> for RunOptions {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> pyo3::PyResult<Self> {
        let warehouse = ob
            .get_item("warehouse")
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
            warehouse,
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
                warehouse: options.warehouse,
                variables: options.variables.unwrap_or(vec![]),
                question: options.question,
            },
            None => Self {
                file,
                warehouse: None,
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

async fn handle_workflow_file(workflow_name: &PathBuf) -> Result<WorkflowResult, OnyxError> {
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
                        eprintln!("Please review these changes and update the schema generation code by `cargo run gen-config-schema.`");
                        exit(1)
                    }
                }
            }
        }
        Some(SubCommand::Init) => match init() {
            Ok(_) => println!("{}", "Initialization complete".success()),
            Err(e) => eprintln!("{}", format!("Initialization failed: {}", e).error()),
        },
        Some(SubCommand::ListTables) => {
            let config = load_config()?;
            for warehouse in &config.warehouses {
                let tables = Connector::new(warehouse).get_schemas().await;
                for table in tables {
                    println!("{}", table.text());
                }
            }
        }
        Some(SubCommand::ListDatasets) => {
            let config = load_config()?;
            for warehouse in &config.warehouses {
                let datasets = Connector::new(warehouse).list_datasets().await;
                for dataset in datasets {
                    println!("{}", dataset.text());
                }
            }
        }
        Some(SubCommand::Run(run_args)) => {
            handle_run_command(run_args).await?;
        }
        Some(SubCommand::Build) => {
            let config = load_config()?;
            build(&config).await?;
        }
        Some(SubCommand::VecSearch(search_args)) => {
            let config = load_config()?;
            let agent_file = args.agent.map(|agent| ProjectPath::get_path(&agent));
            let (agent, agent_name) = config.load_agent_config(agent_file.as_ref())?;

            for tool in agent.tools {
                if let ToolConfig::Retrieval(retrieval) = tool {
                    vector_search(&agent_name, &retrieval, &search_args.question).await?;
                }
            }
        }
        Some(SubCommand::Validate) => {
            let result = load_config();
            match result {
                Ok(config) => match config.validate_workflows() {
                    Ok(_) => {
                        println!("{}", "Config file is valid".success())
                    }
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

        Some(SubCommand::TestTheme) => {
            println!("Initial theme mode: {:?}", get_current_theme_mode());
            println!("True color support: {:?}", detect_true_color_support());
            println!("{}", "analysis".primary());
            println!("{}", "success".success());
            println!("{}", "warning".warning());
            eprintln!("{}", "error".error());
            println!(
                "{}",
                "https://github.com/onyx-hq/onyx-sample-repo/".secondary()
            );
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
    file_path: Option<&PathBuf>,
    question: Option<String>,
) -> Result<AgentResult, OnyxError> {
    let question = question.ok_or_else(|| {
        OnyxError::ArgumentError("Question is required for agent files".to_string())
    })?;
    let result = run_agent(file_path, &FileFormat::Markdown, &question, None).await?;
    Ok(result)
}

async fn handle_sql_file(
    file_path: &PathBuf,
    warehouse: Option<String>,
    config: &Config,
    variables: &[(String, String)],
) -> Result<String, OnyxError> {
    let warehouse = warehouse.ok_or_else(|| OnyxError::ArgumentError("Warehouse is required for running SQL file. Please provide the warehouse using --warehouse or set a default warehouse in config.yml".to_string()))?;
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| OnyxError::RuntimeError(format!("Failed to read SQL file: {}", e)))?;
    let wh_config = config
        .find_warehouse(&warehouse)
        .map_err(|e| OnyxError::ArgumentError(format!("Warehouse not found: {}", e)))?;

    let mut env = Environment::new();
    let mut query = content.clone();

    // Handle variable templating if variables are provided
    if !variables.is_empty() {
        env.add_template("query", &query)
            .map_err(|e| OnyxError::RuntimeError(format!("Failed to parse SQL template: {}", e)))?;
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
            .map_err(|e| OnyxError::RuntimeError(format!("Failed to render SQL template: {}", e)))?
    }

    // Print colored SQL and execute query
    print_colored_sql(&query);
    let (datasets, schema) = Connector::new(&wh_config)
        .run_query_and_load(&query)
        .await?;
    let batches_display = record_batches_to_table(&datasets, &schema)
        .map_err(|e| OnyxError::RuntimeError(format!("Failed to display query results: {}", e)))?;
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

pub async fn handle_run_command(run_args: RunArgs) -> Result<RunResult, OnyxError> {
    let file = &run_args.file;

    let current_dir = std::env::current_dir().expect("Could not get current directory");

    let file_path = current_dir.join(file);
    if !file_path.exists() {
        return Err(OnyxError::ConfigurationError(format!(
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
                let agent_result = handle_agent_file(Some(&file_path), run_args.question).await?;
                return Ok(RunResult::Agent(agent_result));
            } else {
                return Err(OnyxError::ArgumentError(
                    "Invalid YAML file. Must be either *.workflow.yml or *.agent.yml".into(),
                ));
            }
        }
        Some("sql") => {
            let config = load_config()?;
            let warehouse = run_args.warehouse.or(config.clone().defaults.warehouse);
            let sql_result =
                handle_sql_file(&file_path, warehouse, &config, &run_args.variables).await?;
            Ok(RunResult::Sql(sql_result))
        }
        _ => Err(OnyxError::ArgumentError(
            "Invalid file extension. Must be .workflow.yml, .agent.yml, or .sql".into(),
        )),
    }
}

pub async fn start_server_and_web_app() {
    let server_task = tokio::spawn(async move {
        let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
        println!(
            "{} {}",
            "API server running at".text(),
            format!("http://{}", addr).secondary()
        );
        server::serve(&addr).await;
    });

    let web_task = tokio::spawn(async move {
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

        let web_app = Router::new()
            .nest_service("/", serve_with_fallback)
            .fallback_service(fallback_service);

        let web_addr = SocketAddr::from(([127, 0, 0, 1], 3000));
        let listener = tokio::net::TcpListener::bind(web_addr).await.unwrap();
        println!(
            "{} {}",
            "Web app running at".text(),
            format!("http://{}", web_addr).secondary()
        );
        axum::serve(listener, web_app).await.unwrap();
    });

    let _ = tokio::try_join!(server_task, web_task);
}
