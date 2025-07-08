mod init;
mod make;
mod seed;

use crate::adapters::connector::Connector;
use crate::auth::types::AuthMode;
use crate::config::model::AppConfig;
use crate::config::*;
use crate::db::client::establish_connection;
use crate::errors::OxyError;
use crate::execute::types::utils::record_batches_to_table;
use crate::mcp::service::OxyMcpServer;
use crate::project::initialize_project_manager;
use crate::project::resolve_project_path;
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
use crate::utils::print_colored_sql;
use crate::workflow::loggers::cli::WorkflowCLILogger;
use axum::handler::Handler;
use axum::http::HeaderValue;
use clap::CommandFactory;
use clap::Parser;
use clap::builder::ValueParser;
use make::handle_make_command;
use migration::Migrator;
use migration::MigratorTrait;
use minijinja::{Environment, Value};
use model::AgentConfig;
use model::{Config, Semantics, Workflow};
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
use utoipa::openapi::SecurityRequirement;
use utoipa::openapi::security::ApiKeyValue;
use utoipa::openapi::security::SecurityScheme;
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;

use init::init;

use crate::api::router;
use tower_http::trace::{self, TraceLayer};
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
            commit: {commit_link}\n\
            workflow url: {workflow_link}\n",
            env!("CARGO_PKG_VERSION"),
            rustc_version_runtime::version(),
            commit_link = match (
                option_env!("GITHUB_SERVER_URL"),
                option_env!("GITHUB_REPOSITORY"),
                option_env!("GITHUB_SHA")
            ) {
                (Some(server), Some(repo), Some(sha)) => format!("{}/repo/{}/commit/{} ({})", server, repo, sha, sha),
                _ => option_env!("GITHUB_SHA").unwrap_or("unknown").to_string(),
            },
            workflow_link = match (
                option_env!("GITHUB_SERVER_URL"),
                option_env!("GITHUB_REPOSITORY"),
                option_env!("GITHUB_RUN_ID")
            ) {
                (Some(server), Some(repo), Some(run_id)) => format!("{}/repo/{}/actions/runs/{} ({})", server, repo, run_id, run_id),
                _ => option_env!("GITHUB_RUN_ID").unwrap_or("unknown").to_string(),
            },
        ).into_boxed_str()) as &'static str
    },
)]
struct Args {
    /// The question to ask or command to execute
    ///
    /// When no subcommand is provided, this input will be processed
    /// as a question for the default AI agent or as a query suggestion.
    #[clap(default_value = "")]
    input: String,

    /// Output format: 'text' (default) or 'code' for SQL
    ///
    /// Control how results are displayed in the terminal.
    /// Use 'code' for syntax-highlighted SQL output.
    #[clap(long, value_name = "FORMAT")]
    output: Option<String>,

    /// Subcommand to execute
    #[clap(subcommand)]
    command: Option<SubCommand>,
}

#[derive(Parser, Debug)]
struct McpArgs {
    /// Path to the Oxy project directory
    ///
    /// Specify the root directory of your Oxy project where
    /// config.yml and other project files are located.
    #[clap(long)]
    pub project_path: PathBuf,
}

#[derive(Parser, Debug)]
struct McpSseArgs {
    /// Port number for the MCP Server-Sent Events server
    ///
    /// Specify which port to bind the MCP SSE server for
    /// web-based integrations. Default is 8000.
    #[clap(long, default_value_t = 8000)]
    port: u16,
    /// Host address to bind the MCP SSE server
    ///
    /// Specify which host address to bind the MCP SSE server.
    /// Default is 0.0.0.0 to listen on all interfaces.
    #[clap(long, default_value = "0.0.0.0")]
    host: String,
}

#[derive(Parser, Debug)]
struct AskArgs {
    /// Question to ask the AI agent
    ///
    /// Provide your question or request for analysis to the
    /// configured AI agent. The agent will use your project context
    /// to provide relevant insights and answers.
    #[clap(long)]
    pub question: String,
}

#[derive(Parser, Debug)]
enum SubCommand {
    /// Initialize a repository as an oxy project. Also creates a ~/.config/oxy/config.yaml file if it doesn't exist
    Init,
    /// Execute workflow (.workflow.yml), agent (.agent.yml), or SQL (.sql) files
    ///
    /// Run SQL queries against databases, execute workflows for data processing,
    /// or interact with AI agents for analysis and insights.
    Run(RunArgs),
    /// Run evaluation tests on workflow files to measure consistency and performance
    ///
    /// Execute test cases defined in workflow files and generate metrics
    /// to validate workflow reliability and output quality.
    Test(TestArgs),
    /// Build vector embeddings for hybrid search capabilities
    ///
    /// Process your project files and create searchable embeddings for
    /// enhanced semantic search and retrieval functionality.
    Build(BuildArgs),
    /// Perform semantic vector search across your project
    ///
    /// Search through your codebase, documentation, and data using
    /// natural language queries powered by vector embeddings.
    VecSearch(VecSearchArgs),
    /// Synchronize and collect metadata from connected databases
    ///
    /// Extract schema information, table structures, and relationships
    /// from your databases to enable better query suggestions and validation.
    Sync(SyncArgs),
    /// Validate configuration files for syntax and structure
    ///
    /// Check your config.yml, workflow files, and agent configurations
    /// for errors and compliance with the expected schema.
    Validate,
    /// Start MCP (Model Context Protocol) server with Server-Sent Events transport
    ///
    /// Launch a web-accessible MCP server that enables integration with
    /// MCP-compatible AI tools and applications via HTTP/SSE.
    McpSse(McpSseArgs),
    /// Start MCP (Model Context Protocol) server with stdio transport
    ///
    /// Launch an MCP server using standard input/output for direct
    /// integration with local AI tools and development environments.
    McpStdio(McpArgs),
    /// Start the web application server with API endpoints
    ///
    /// Launch the full Oxy web interface with authentication,
    /// database connectivity, and interactive query capabilities.
    Serve(ServeArgs),
    /// Test and preview terminal color theme support
    ///
    /// Display color samples and theme information to verify
    /// terminal compatibility and appearance settings.
    TestTheme,
    /// Generate JSON schema files for configuration validation
    ///
    /// Create or update schema files used by IDEs and tools
    /// for configuration file validation and autocompletion.
    GenConfigSchema(GenConfigSchemaArgs),
    /// Update the Oxy CLI to the latest available version
    ///
    /// Download and install the newest release of Oxy,
    /// ensuring you have access to the latest features and fixes.
    SelfUpdate,
    /// Execute and manage workflow files with advanced options
    ///
    /// Run workflow files with additional control over execution,
    /// error handling, and output formatting.
    Make(MakeArgs),
    /// Ask questions to AI agents for analysis and insights
    ///
    /// Interact with configured AI agents to get answers about
    /// your data, generate queries, or analyze results.
    Ask(AskArgs),
    /// Database seeding commands for development and testing
    #[clap(hide = true)]
    Seed(SeedArgs),
}

#[derive(Parser, Debug)]
pub struct MakeArgs {
    /// Path to the workflow file to execute
    file: String,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Path to the file to execute (.sql, .workflow.yml, or .agent.yml)
    file: String,

    /// Database connection to use for SQL execution
    ///
    /// Specify which database from your config.yml to use.
    /// If not provided, uses the default database from configuration.
    #[clap(long)]
    database: Option<String>,

    /// Template variables in the format VAR=VALUE
    ///
    /// Pass variables to SQL templates using Jinja2 syntax.
    /// Example: --variables user_id=123 --variables status=active
    #[clap(long, short = 'v', value_parser=ValueParser::new(parse_variable), num_args = 1..)]
    variables: Vec<(String, String)>,

    /// Question to ask when running agent files
    ///
    /// Required when executing .agent.yml files to provide context
    /// for the AI agent's analysis or response.
    question: Option<String>,

    /// Retry failed operations automatically
    ///
    /// Enable automatic retry logic for transient failures
    /// during workflow or query execution.
    #[clap(long, default_value_t = false)]
    retry: bool,

    /// Preview SQL without executing against the database
    ///
    /// Validate and display the generated SQL query without
    /// actually running it against your database.
    #[clap(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Parser, Debug)]
pub struct TestArgs {
    /// Path to the workflow file to test
    file: String,
    /// Suppress detailed output and show only results summary
    ///
    /// Enable quiet mode to reduce verbose logging during test execution
    /// and display only essential test results and metrics.
    #[clap(long, short = 'q', default_value_t = false)]
    quiet: bool,
}

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Drop all existing embedding tables before rebuilding
    ///
    /// Warning: This will delete all existing vector embeddings
    /// and rebuild the entire search index from scratch.
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
    /// Natural language query to search for
    ///
    /// Enter your search question in plain English. The system will
    /// find relevant code, documentation, and data using semantic matching.
    question: String,
    /// Specify a custom agent configuration for enhanced search
    ///
    /// Use a specific agent from your configuration to process
    /// and interpret the search results with domain expertise.
    #[clap(long, value_name = "AGENT_NAME")]
    agent: String,
}

#[derive(Parser, Debug)]
struct SyncArgs {
    /// Specific database to sync (syncs all if not specified)
    ///
    /// Target a single database connection from your config.yml
    /// instead of syncing metadata from all configured databases.
    database: Option<String>,
    /// Specific datasets/tables to sync within the database
    ///
    /// Limit synchronization to particular tables or schemas
    /// instead of processing the entire database structure.
    #[clap(long, short = 'd', num_args = 0..)]
    datasets: Vec<String>,
    /// Overwrite existing metadata files during sync
    ///
    /// Replace existing schema files and metadata instead of
    /// skipping tables that have already been synchronized.
    #[clap(
        long,
        short = 'o',
        default_value_t = false,
        help = "Overwrite existing files during sync"
    )]
    overwrite: bool,
}

#[derive(Parser, Debug)]
struct ServeArgs {
    /// Port number for the web application server
    ///
    /// Specify which port to bind the Oxy web interface.
    /// Default is 3000 if not specified.
    #[clap(long, default_value_t = 3000)]
    port: u16,
    /// Host address to bind the web application server
    ///
    /// Specify which host address to bind the Oxy web interface.
    /// Default is 0.0.0.0 to listen on all interfaces.
    #[clap(long, default_value = "0.0.0.0")]
    host: String,
    /// Authentication mode for the web application
    ///
    /// Choose between 'local' for development or 'oauth' for
    /// production deployments with proper user authentication.
    #[clap(long, default_value_t = AuthMode::BuiltIn, value_enum)]
    auth_mode: AuthMode,
    /// Enable git-based project detection and onboarding
    ///
    /// When enabled, allows starting the server outside of an Oxy project
    /// directory and provides git-based onboarding functionality.
    #[clap(long, default_value_t = false)]
    readonly: bool,
}

#[derive(Parser, Debug)]
struct GenConfigSchemaArgs {
    /// Check for uncommitted schema changes in git
    ///
    /// Verify that generated schema files match the current
    /// configuration structure and fail if changes are detected.
    #[clap(long)]
    check: bool,
}

#[derive(Parser, Debug)]
pub struct SeedArgs {
    /// Database seeding action to perform
    #[clap(subcommand)]
    pub action: SeedAction,
}

#[derive(Parser, Debug)]
pub enum SeedAction {
    /// Create test users for development environment
    ///
    /// Generates 3 test users including guest@oxy.local for
    /// local authentication testing and development.
    Users,
    /// Create sample threads for existing test users
    ///
    /// Generates 1000 sample analysis threads per test user
    /// with realistic SQL queries and responses.
    Threads,
    /// Clear all test data (users and threads)
    ///
    /// Removes all test users and their associated threads
    /// to reset the development database to a clean state.
    Clear,
    /// Full seed - create users and sample threads
    ///
    /// Complete seeding process that creates test users
    /// and generates sample threads for comprehensive testing.
    Full,
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

    let readonly_mode = match &args.command {
        Some(SubCommand::Serve(serve_args)) => serve_args.readonly,
        _ => false, // For other commands, try to auto-detect
    };

    // Initialize project manager early for most commands (except Init and GenConfigSchema)
    let needs_project_manager = match &args.command {
        Some(SubCommand::Init)
        | Some(SubCommand::GenConfigSchema(_))
        | Some(SubCommand::SelfUpdate)
        | Some(SubCommand::TestTheme) => false,
        _ => true,
    };

    if needs_project_manager {
        if let Err(e) = initialize_project_manager(readonly_mode).await {
            // For some commands, not having a project is acceptable
            match &args.command {
                Some(SubCommand::Serve(_)) if readonly_mode => {
                    // In git mode, we'll handle this in the serve command
                }
                _ => {
                    tracing::debug!("Failed to initialize project manager: {}", e);
                    // For non-git commands, we may still want to proceed in some cases
                }
            }
        }
    }

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
                (
                    "global-semantics.json",
                    serde_json::to_string_pretty(&schemars::schema_for!(Semantics))?,
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
                    .map(|(filename, _)| format!("json-schemas/{filename}"))
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
            Err(e) => eprintln!("{}", format!("Initialization failed: {e}").error()),
        },
        Some(SubCommand::Run(run_args)) => {
            handle_run_command(run_args).await?;
        }
        Some(SubCommand::Test(test_args)) => {
            handle_test_command(test_args).await?;
        }
        Some(SubCommand::Build(build_args)) => {
            reindex(ReindexInput {
                project_path: resolve_project_path()?.to_string_lossy().to_string(),
                drop_all_tables: build_args.drop_all_tables,
            })
            .await?;
        }
        Some(SubCommand::VecSearch(search_args)) => {
            let project_path = resolve_project_path()?.to_string_lossy().to_string();
            search(SearchInput {
                project_path,
                agent_ref: search_args.agent.to_string(),
                query: search_args.question.to_string(),
            })
            .await?;
        }
        Some(SubCommand::Sync(sync_args)) => {
            let config = ConfigBuilder::new()
                .with_project_path(&resolve_project_path()?)?
                .build()
                .await?;
            let filter = sync_args
                .database
                .clone()
                .map(|db| (db, sync_args.datasets.clone()));
            debug!(sync_args = ?sync_args, "Syncing");
            println!("🔄Syncing databases");
            let sync_metrics = sync_databases(config.clone(), filter, sync_args.overwrite).await?;
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
            start_server_and_web_app(
                serve_args.port,
                serve_args.host,
                serve_args.auth_mode,
                serve_args.readonly,
            )
            .await;
        }
        Some(SubCommand::McpSse(mcp_sse_args)) => {
            let cancellation_token = start_mcp_sse_server(mcp_sse_args.port, mcp_sse_args.host)
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
                eprintln!("{}", format!("Failed to update: {e}").error());
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
            let project_path = resolve_project_path()?;
            let config = ConfigBuilder::new()
                .with_project_path(&project_path)?
                .build()
                .await?;

            let _ = run_agent(
                &project_path,
                &config.get_builder_agent_path().await?,
                ask_args.question,
                AgentCLIHandler::default(),
                vec![],
            )
            .await?;
        }

        Some(SubCommand::Seed(seed_args)) => {
            handle_seed_command(seed_args).await?;
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
    let project_path = resolve_project_path()?;
    let _ = run_agent(
        &project_path,
        file_path,
        question,
        AgentCLIHandler::default(),
        vec![],
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
        .map_err(|e| OxyError::RuntimeError(format!("Failed to read SQL file: {e}")))?;
    let mut env = Environment::new();
    let mut query = content.clone();

    // Handle variable templating if variables are provided
    if !variables.is_empty() {
        env.add_template("query", &query)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to parse SQL template: {e}")))?;
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
            .map_err(|e| OxyError::RuntimeError(format!("Failed to render SQL template: {e}")))?
    }

    // Print colored SQL and execute query
    print_colored_sql(&query);
    let connector = Connector::from_database(&database, config, None).await?;
    let (datasets, schema) = match dry_run {
        false => connector.run_query_and_load(&query).await,
        true => connector.dry_run(&query).await,
    }?;
    let batches_display = record_batches_to_table(&datasets, &schema)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to display query results: {e}")))?;
    println!("\n\x1b[1;32mResults:\x1b[0m");
    println!("{batches_display}");

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
            "File not found: {file_path:?}"
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
                .with_project_path(&resolve_project_path()?)?
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

pub async fn start_mcp_sse_server(
    mut port: u16,
    host: String,
) -> anyhow::Result<CancellationToken> {
    // require webserver to be started inside the project path
    let project_path = match resolve_project_path() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to find project path: {e}");
            std::process::exit(1);
        }
    };

    let original_port = port;
    let mut port_increment_count = 0;
    const MAX_PORT_INCREMENTS: u16 = 10;

    loop {
        match tokio::net::TcpListener::bind((host.as_str(), port)).await {
            Ok(_) => break,
            Err(e) => {
                if port <= 1024 && e.kind() == std::io::ErrorKind::PermissionDenied {
                    eprintln!(
                        "Permission denied binding to port {port}. Try running with sudo or use a port above 1024."
                    );
                    std::process::exit(1);
                }

                if port_increment_count >= MAX_PORT_INCREMENTS {
                    eprintln!(
                        "Failed to bind to any port after trying {} ports starting from {}. Error: {}",
                        port_increment_count + 1,
                        original_port,
                        e
                    );
                    std::process::exit(1);
                }

                println!("Port {port} for mcp is occupied. Trying next port...");
                port += 1;
                port_increment_count += 1;
            }
        }
    }

    let service = OxyMcpServer::new(project_path.clone()).await?;
    let bind = format!("{host}:{port}")
        .parse::<SocketAddr>()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], port)));
    let ct = SseServer::serve(bind)
        .await?
        .with_service(move || service.to_owned());

    let display_host = if host == "0.0.0.0" {
        "localhost"
    } else {
        &host
    };
    println!(
        "{}",
        format!("MCP server running at http://{display_host}:{port}").secondary()
    );
    anyhow::Ok(ct)
}

pub async fn handle_test_command(test_args: TestArgs) -> Result<(), OxyError> {
    let file = &test_args.file;
    let current_dir = std::env::current_dir().expect("Could not get current directory");
    let file_path = current_dir.join(file);

    if !file_path.exists() {
        return Err(OxyError::ConfigurationError(format!(
            "File not found: {file_path:?}"
        )));
    }

    run_eval(
        &resolve_project_path()?,
        &file_path,
        None,
        EvalEventsHandler::new(test_args.quiet),
    )
    .await?;
    Ok(())
}

#[derive(OpenApi)]
struct ApiDoc;

pub async fn start_server_and_web_app(
    mut web_port: u16,
    web_host: String,
    auth_mode: AuthMode,
    readonly_mode: bool,
) {
    // Set global readonly mode
    crate::readonly::set_readonly_mode(readonly_mode);

    // migrate database if needed
    let db = establish_connection()
        .await
        .expect("Failed to connect to database");
    Migrator::up(&db, None)
        .await
        .expect("Failed to run database migrations");

    // Initialize background task manager singleton
    if let Err(e) = crate::github::background_tasks::initialize_background_task_manager().await {
        tracing::warn!("Failed to initialize background task manager: {}", e);
    }

    // Initialize project manager for the web app
    let project_manager_initialized = match initialize_project_manager(readonly_mode).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("Failed to initialize project manager: {}", e);
            false
        }
    };

    // Check if we're in a valid project
    let project_path = if project_manager_initialized {
        match resolve_project_path() {
            Ok(path) => Some(path),
            Err(e) => {
                tracing::warn!(
                    "Project manager initialized but failed to resolve path: {}",
                    e
                );
                None
            }
        }
    } else {
        // Fallback to old behavior for backward compatibility
        let project_path_result = resolve_project_path();
        match project_path_result {
            Ok(path) => Some(path),
            Err(_) => {
                if readonly_mode {
                    tracing::info!("Readonly mode enabled");
                    None
                } else {
                    // Old behavior - exit if not in an Oxy project
                    eprintln!(
                        "Error: Not in an Oxy project directory. Run 'oxy init' first or use --readonly to use git integration."
                    );
                    std::process::exit(1);
                }
            }
        }
    };

    // Only build config if we have a valid project path
    let config = if let Some(ref path) = project_path {
        Some(
            ConfigBuilder::new()
                .with_project_path(path)
                .expect("Failed to find project path")
                .build()
                .await
                .expect("Failed to load configuration"),
        )
    } else {
        None
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
        let original_port = web_port;
        let mut port_increment_count = 0;
        const MAX_PORT_INCREMENTS: u16 = 10;

        loop {
            match tokio::net::TcpListener::bind((web_host.as_str(), web_port)).await {
                Ok(_) => break,
                Err(e) => {
                    // For privileged ports (1024 and below), don't auto-increment if it's a permission error
                    if web_port <= 1024 && e.kind() == std::io::ErrorKind::PermissionDenied {
                        eprintln!(
                            "Permission denied binding to port {web_port}. Try running with sudo or use a port above 1024."
                        );
                        std::process::exit(1);
                    }

                    // If we've tried too many increments, give up
                    if port_increment_count >= MAX_PORT_INCREMENTS {
                        eprintln!(
                            "Failed to bind to any port after trying {} ports starting from {}. Error: {}",
                            port_increment_count + 1,
                            original_port,
                            e
                        );
                        std::process::exit(1);
                    }

                    println!("Port {web_port} for web app is occupied. Trying next port...");
                    web_port += 1;
                    port_increment_count += 1;
                }
            }
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

        // Configure HTTP request/response logging
        let trace_layer = TraceLayer::new_for_http()
            .make_span_with(trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
            .on_request(trace::DefaultOnRequest::new().level(tracing::Level::INFO))
            .on_response(
                trace::DefaultOnResponse::new()
                    .level(tracing::Level::INFO)
                    .latency_unit(tower_http::LatencyUnit::Millis),
            )
            .on_failure(trace::DefaultOnFailure::new().level(tracing::Level::ERROR));
        let api_router = match router::api_router(auth_mode, readonly_mode).await {
            Ok(router) => router.layer(trace_layer.clone()),
            Err(e) => {
                eprintln!("Failed to create API router: {e}");
                std::process::exit(1);
            }
        };
        let openapi_router = router::openapi_router(readonly_mode)
            .await
            .layer(trace_layer.clone());
        let mut openapi_router = OpenApiRouter::with_openapi(ApiDoc::openapi())
            .nest("/api", openapi_router)
            .fallback_service(serve_with_fallback);
        let openapi = openapi_router.get_openapi_mut();
        if let Some(cfg) = config {
            if let Some(auth_config) = cfg.get_authentication() {
                if let Some(api_key_auth) = auth_config.api_key {
                    let security_schema_name = "ApiKey";

                    // Get existing components or create new ones, then add the security scheme
                    let mut components = openapi.components.take().unwrap_or_default();
                    components.security_schemes.insert(
                        security_schema_name.to_string(),
                        SecurityScheme::ApiKey(utoipa::openapi::security::ApiKey::Header(
                            ApiKeyValue::new(api_key_auth.header),
                        )),
                    );
                    openapi.components = Some(components);

                    // Apply for all endpoints
                    let scopes: Vec<String> = vec![];
                    openapi.security =
                        Some(vec![SecurityRequirement::new(security_schema_name, scopes)]);
                }
            }
        }
        let web_app = Router::new()
            .merge(SwaggerUi::new("/apidoc").url(
                "/apidoc/openapi.json",
                openapi_router.into_openapi().clone(),
            ))
            .nest("/api", api_router)
            .fallback_service(serve_with_fallback)
            .layer(trace_layer);

        let web_addr = format!("{web_host}:{web_port}")
            .parse::<SocketAddr>()
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], web_port)));
        let listener = tokio::net::TcpListener::bind(web_addr).await.unwrap();
        let display_host = if web_host == "0.0.0.0" {
            "localhost"
        } else {
            &web_host
        };

        if readonly_mode {
            println!(
                "{} {} {}",
                "Web app running at".text(),
                format!("http://{}:{}", display_host, web_port).secondary(),
                "(readonly mode)".tertiary()
            );
        } else {
            println!(
                "{} {}",
                "Web app running at".text(),
                format!("http://{}:{}", display_host, web_port).secondary()
            );
        }
        println!(
            "{} {}",
            "Web app running at".text(),
            format!("http://{display_host}:{web_port}").secondary()
        );

        if let Err(e) = crate::auth::user::UserService::sync_admin_roles_from_config().await {
            tracing::warn!("Failed to sync admin roles: {}", e);
        } else {
            tracing::info!("Admin roles synced successfully");
        }

        axum::serve(listener, web_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });

    // Start Apalis worker in readonly mode for background tasks
    let worker_task = if readonly_mode {
        Some(tokio::spawn(async move {
            if let Err(e) = crate::github::start_apalis_worker().await {
                tracing::error!("Failed to start Apalis worker: {}", e);
            }
        }))
    } else {
        None
    };

    // Wait for web server to complete
    let web_result = web_server_task.await;

    // If we started a worker, cancel it when web server stops
    if let Some(worker) = worker_task {
        worker.abort();
    }

    web_result.unwrap();
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
            .bin_name(&format!("oxy-{target}"))
            .show_download_progress(true)
            .current_version(self_update::cargo_crate_version!())
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Update configuration failed: {e}")))?
            .update()
            .map_err(|e| OxyError::RuntimeError(format!("Update failed: {e}")))
    })
    .await
    .map_err(|e| OxyError::RuntimeError(format!("Task join error: {e}")))??;

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

async fn handle_seed_command(seed_args: SeedArgs) -> Result<(), OxyError> {
    use seed::*;

    match seed_args.action {
        SeedAction::Users => {
            seed_test_users().await?;
        }
        SeedAction::Threads => {
            create_sample_threads_for_users().await?;
        }
        SeedAction::Clear => {
            clear_test_data().await?;
        }
        SeedAction::Full => {
            println!("🚀 Performing full database seed...");
            seed_test_users().await?;
            create_sample_threads_for_users().await?;
            println!("✨ Full seed completed successfully!");
        }
    }
    Ok(())
}
