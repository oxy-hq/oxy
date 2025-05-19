mod init;
mod make;

use crate::adapters::connector::Connector;
use crate::config::model::AppConfig;
use crate::config::*;
use crate::errors::OxyError;
use crate::execute::types::utils::record_batches_to_table;
use crate::mcp::service::OxyMcpServer;
use crate::service::agent::AgentCLIHandler;
use crate::service::agent::run_agent;
use crate::service::eval::EvalEventsHandler;
use crate::service::eval::run_eval;
use crate::service::retrieval::{ReindexInput, SearchInput, reindex, search};
use crate::service::sync::sync_databases;
use crate::service::workflow::run_workflow;
use crate::theme::StyledText;
use crate::theme::detect_true_color_support;
use crate::theme::get_current_theme_mode;
use crate::utils::find_project_path;
use crate::utils::print_colored_sql;
use crate::workflow::loggers::cli::WorkflowCLILogger;
use axum::handler::Handler;
use axum::http::HeaderValue;
use clap::CommandFactory;
use clap::Parser;
use clap::builder::ValueParser;
use make::handle_make_command;
use minijinja::{Environment, Value};
use model::AgentConfig;
use model::{Config, Workflow};
use pyo3::Bound;
use pyo3::FromPyObject;
use pyo3::IntoPyObject;
use pyo3::PyAny;
use pyo3::PyErr;
use pyo3::Python;
use pyo3::types::PyAnyMethods;
use rmcp::transport::SseServer;
use rmcp::{ServiceExt, transport::stdio};
use std::backtrace;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::process::exit;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use init::init;

use crate::api::router;
use tower_http::trace::TraceLayer;
use tower_serve_static::ServeDir;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get_service,
};

use dotenv;
use include_dir::{Dir, include_dir};
use std::net::SocketAddr;
use tower::service_fn;
use tracing::{debug, error};

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
#[clap(
    author,
    version,
    long_version = if cfg!(debug_assertions) {
        Box::leak(format!(
            "version {}, built locally as debug, rust ver {}",
            env!("CARGO_PKG_VERSION"),
            rustc_version_runtime::version(),
        ).into_boxed_str()) as &'static str
    } else {
        Box::leak(format!(
            "version: {}\n\
            rust version: {}\n\
            commit SHA: {}\n\
            branch: {}\n\
            repository: {}\n\
            \
            triggered by: {}\n\
            event: {}\n\
            build timestamp: {}",
            env!("CARGO_PKG_VERSION"),
            rustc_version_runtime::version(),
            option_env!("GITHUB_SHA").unwrap_or("unknown"),
            option_env!("GITHUB_REF_NAME").unwrap_or("unknown"),
            option_env!("GITHUB_REPOSITORY").unwrap_or("unknown"),
            option_env!("GITHUB_ACTOR").unwrap_or("unknown"),
            option_env!("GITHUB_EVENT_NAME").unwrap_or("unknown"),
            chrono::Utc::now().to_rfc3339(),
        ).into_boxed_str()) as &'static str
    },
)]
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
}

#[derive(Parser, Debug)]
struct McpArgs {
    #[clap(long)]
    pub project_path: PathBuf,
}

#[derive(Parser, Debug)]
struct McpSseArgs {
    #[clap(long, default_value_t = 8000)]
    port: u16,
}

#[derive(Parser, Debug)]
struct AskArgs {
    #[clap(long)]
    pub question: String,
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
    Build(BuildArgs),
    /// Perform vector search
    VecSearch(VecSearchArgs),
    /// Collect semantic information from the databases
    Sync(SyncArgs),
    /// Validate the config file
    Validate,
    /// Start the API server and serve the frontend web app
    McpSse(McpSseArgs),
    McpStdio(McpArgs),
    Serve(ServeArgs),
    /// Test theme for terminal output
    TestTheme,
    /// Generate JSON schemas for config files
    GenConfigSchema(GenConfigSchemaArgs),
    /// Update the CLI to the latest version
    SelfUpdate,
    Make(MakeArgs),
    Ask(AskArgs),
}

#[derive(Parser, Debug)]
pub struct MakeArgs {
    file: String,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    file: String,

    #[clap(long)]
    database: Option<String>,

    #[clap(long, short = 'v', value_parser=ValueParser::new(parse_variable), num_args = 1..)]
    variables: Vec<(String, String)>,

    question: Option<String>,

    #[clap(long, default_value_t = false)]
    retry: bool,

    #[clap(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Parser, Debug)]
pub struct TestArgs {
    file: String,
    #[clap(long, short = 'q', default_value_t = false)]
    quiet: bool,
}

#[derive(Parser, Debug)]
pub struct BuildArgs {
    #[clap(long, short = 'd', default_value_t = false)]
    drop_all_tables: bool,
}

#[derive(Clone)]
pub struct RunOptions {
    database: Option<String>,
    variables: Option<Vec<(String, String)>>,
    question: Option<String>,
    retry: bool,
    dry_run: bool,
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
        let retry = ob
            .get_item("retry")
            .map(|v| v.extract::<bool>().unwrap_or(false))
            .unwrap_or(false);
        let dry_run = ob
            .get_item("dry_run")
            .map(|v| v.extract::<bool>().unwrap_or(false))
            .unwrap_or(false);

        Ok(RunOptions {
            database,
            variables,
            question,
            retry,
            dry_run,
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
                retry: options.retry,
                dry_run: options.dry_run,
            },
            None => Self {
                file,
                database: None,
                variables: vec![],
                question: None,
                retry: false,
                dry_run: false,
            },
        }
    }
}

#[derive(Parser, Debug)]
struct VecSearchArgs {
    question: String,
    /// Specify a custom agent configuration
    #[clap(long, value_name = "AGENT_NAME")]
    agent: String,
}

#[derive(Parser, Debug)]
struct SyncArgs {
    database: Option<String>,
    #[clap(long, short = 'd', num_args = 0..)]
    datasets: Vec<String>,
    #[clap(long, short = 'i', default_value_t = false)]
    ignore_changes: bool,
}

#[derive(Parser, Debug)]
struct ServeArgs {
    #[clap(long, default_value_t = 3000)]
    port: u16,
}

#[derive(Parser, Debug)]
struct GenConfigSchemaArgs {
    #[clap(long)]
    check: bool,
}

async fn handle_workflow_file(workflow_name: &PathBuf, retry: bool) -> Result<(), OxyError> {
    run_workflow(workflow_name, WorkflowCLILogger, retry, None).await?;
    Ok(())
}

pub async fn cli() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    use std::panic;

    panic::set_hook(Box::new(move |panic_info| {
        error!(
            error = %panic_info,
            trace = %backtrace::Backtrace::force_capture(),
            "panic occurred"
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
                (
                    "app.json",
                    serde_json::to_string_pretty(&schemars::schema_for!(AppConfig))?,
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
        Some(SubCommand::Build(build_args)) => {
            reindex(ReindexInput {
                project_path: find_project_path()?.to_string_lossy().to_string(),
                drop_all_tables: build_args.drop_all_tables,
            })
            .await?;
        }
        Some(SubCommand::VecSearch(search_args)) => {
            let project_path = find_project_path()?.to_string_lossy().to_string();
            search(SearchInput {
                project_path,
                agent_ref: search_args.agent.to_string(),
                query: search_args.question.to_string(),
            })
            .await?;
        }
        Some(SubCommand::Sync(sync_args)) => {
            let config = ConfigBuilder::new()
                .with_project_path(&find_project_path()?)?
                .build()
                .await?;
            let filter = sync_args
                .database
                .clone()
                .map(|db| (db, sync_args.datasets.clone()));
            debug!(sync_args = ?sync_args, "Syncing");
            println!("🔄Syncing databases");
            let sync_metrics =
                sync_databases(config.clone(), filter, !sync_args.ignore_changes).await?;
            println!(
                "✅Sync finished:\n\n{}",
                sync_metrics
                    .into_iter()
                    .map(|m| m.map_or_else(|e| e.to_string().error().to_string(), |v| v.to_string()))
                    .collect::<Vec<_>>()
                    .join("\n---\n")
            )
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
        Some(SubCommand::Serve(serve_args)) => {
            start_server_and_web_app(serve_args.port).await;
        }
        Some(SubCommand::McpSse(mcp_sse_args)) => {
            let cancellation_token = start_mcp_sse_server(mcp_sse_args.port)
                .await
                .expect("Failed to start MCP SSE server");

            tokio::signal::ctrl_c().await.unwrap();
            println!("Shutting down server...");
            cancellation_token.cancel();
        }
        Some(SubCommand::McpStdio(args)) => {
            let env_path = args.project_path.join(".env");
            dotenv::from_path(env_path).ok();
            let _ = start_mcp_stdio(args.project_path).await;
        }
        Some(SubCommand::SelfUpdate) => {
            if let Err(e) = handle_check_for_updates().await {
                error!(error = %e, "Failed to update");
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
        Some(SubCommand::Make(make_args)) => {
            handle_make_command(&make_args).await?;
        }

        Some(SubCommand::Ask(ask_args)) => {
            let project_path = find_project_path()?;
            let config = ConfigBuilder::new()
                .with_project_path(&project_path)?
                .build()
                .await?;

            let _ = run_agent(
                &project_path,
                &config.get_builder_agent_path().await?,
                ask_args.question,
                AgentCLIHandler::default(),
            )
            .await?;
        }

        None => {
            Args::command().print_help().unwrap();
        }
    }

    Ok(())
}

async fn handle_agent_file(file_path: &PathBuf, question: Option<String>) -> Result<(), OxyError> {
    let question = question.ok_or_else(|| {
        OxyError::ArgumentError("Question is required for agent files".to_string())
    })?;
    let project_path = find_project_path()?;
    let _ = run_agent(
        &project_path,
        file_path,
        question,
        AgentCLIHandler::default(),
    )
    .await?;
    Ok(())
}

async fn handle_sql_file(
    file_path: &PathBuf,
    database: Option<String>,
    config: &ConfigManager,
    variables: &[(String, String)],
    dry_run: bool,
) -> Result<String, OxyError> {
    let database = database.ok_or_else(|| {
        OxyError::ArgumentError(
            "Database is required for running SQL file. Please provide the database using --database or set a default database in config.yml".to_string(),
        )
    })?;
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
    let connector = Connector::from_database(&database, config, None).await?;
    let (datasets, schema) = match dry_run {
        false => connector.run_query_and_load(&query).await,
        true => connector.dry_run(&query).await,
    }?;
    let batches_display = record_batches_to_table(&datasets, &schema)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to display query results: {}", e)))?;
    println!("\n\x1b[1;32mResults:\x1b[0m");
    println!("{}", batches_display);

    Ok(batches_display.to_string())
}

pub enum RunResult {
    Workflow,
    Agent,
    Sql(String),
}

impl<'py> IntoPyObject<'py> for RunResult {
    type Target = PyAny;

    type Output = Bound<'py, Self::Target>;

    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self {
            RunResult::Workflow => Ok(None::<usize>.into_pyobject(py)?.into_any()),
            RunResult::Agent => Ok(None::<usize>.into_pyobject(py)?.into_any()),
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
                handle_workflow_file(&file_path, run_args.retry).await?;
                Ok(RunResult::Workflow)
            } else if file.ends_with(".agent.yml") {
                handle_agent_file(&file_path, run_args.question).await?;
                return Ok(RunResult::Agent);
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
            let sql_result = handle_sql_file(
                &file_path,
                database,
                &config,
                &run_args.variables,
                run_args.dry_run,
            )
            .await?;
            Ok(RunResult::Sql(sql_result))
        }
        _ => Err(OxyError::ArgumentError(
            "Invalid file extension. Must be .workflow.yml, .agent.yml, or .sql".into(),
        )),
    }
}

pub async fn start_mcp_stdio(project_path: PathBuf) -> anyhow::Result<()> {
    let service = OxyMcpServer::new(project_path)
        .await?
        .serve(stdio())
        .await
        .inspect_err(|e| {
            error!(error = ?e, "Error in MCP stdio server");
        })?;

    service.waiting().await?;
    Ok(())
}

pub async fn start_mcp_sse_server(mut port: u16) -> anyhow::Result<CancellationToken> {
    // require webserver to be started inside the project path
    let project_path = match find_project_path() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to find project path: {}", e);
            std::process::exit(1);
        }
    };
    while tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .is_err()
    {
        println!("Port {} for mcp is occupied. Trying next port...", port);
        port += 1;
    }
    let service = OxyMcpServer::new(project_path.clone()).await?;
    let bind = SocketAddr::from(([0, 0, 0, 0], port));
    let ct = SseServer::serve(bind)
        .await?
        .with_service(move || service.to_owned());

    println!(
        "{}",
        format!("MCP server running at http://localhost:{}", port).secondary()
    );
    anyhow::Ok(ct)
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

    run_eval(
        &find_project_path()?,
        &file_path,
        None,
        EvalEventsHandler::new(test_args.quiet),
    )
    .await?;
    Ok(())
}

#[derive(OpenApi)]
struct ApiDoc;

pub async fn start_server_and_web_app(mut web_port: u16) {
    // require webserver to be started inside the project path
    match find_project_path() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to find project path: {}", e);
            std::process::exit(1);
        }
    };
    async fn shutdown_signal() {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    let web_server_task = tokio::spawn(async move {
        while tokio::net::TcpListener::bind(("0.0.0.0", web_port))
            .await
            .is_err()
        {
            println!(
                "Port {} for web app is occupied. Trying next port...",
                web_port
            );
            web_port += 1;
        }
        let serve_with_fallback = service_fn(move |req: Request<Body>| {
            async move {
                let uri = req.uri().clone();
                let mut res = get_service(ServeDir::new(&DIST))
                    .call(req, None::<()>)
                    .await;
                if uri.path().starts_with("/assets/") {
                    res.headers_mut().insert(
                        "Cache-Control",
                        HeaderValue::from_static("public, max-age=31536000, immutable"),
                    );
                }
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
        let api_router = router::api_router().await.layer(TraceLayer::new_for_http());
        let openapi_router = router::openapi_router()
            .await
            .layer(TraceLayer::new_for_http());
        let (_, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
            .nest("/api", openapi_router)
            .fallback_service(serve_with_fallback)
            .split_for_parts();
        let web_app = Router::new()
            .merge(SwaggerUi::new("/apidoc").url("/apidoc/openapi.json", openapi))
            .nest("/api", api_router)
            .fallback_service(serve_with_fallback)
            .layer(TraceLayer::new_for_http());

        let web_addr = SocketAddr::from(([0, 0, 0, 0], web_port));
        let listener = tokio::net::TcpListener::bind(web_addr).await.unwrap();
        println!(
            "{} {}",
            "Web app running at".text(),
            format!("http://localhost:{}", web_port).secondary()
        );
        axum::serve(listener, web_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });

    web_server_task.await.unwrap();
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
