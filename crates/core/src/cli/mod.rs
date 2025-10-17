pub mod clean;
mod init;
mod make;
mod migrate;
mod seed;
mod serve;

use crate::adapters::connector::Connector;
use crate::adapters::project::builder::ProjectBuilder;
use crate::adapters::runs::RunsManager;
use crate::adapters::secrets::SecretsManager;
use crate::cli::migrate::migrate;
use crate::config::model::AppConfig;
use crate::config::*;
use crate::errors::OxyError;
use crate::execute::types::utils::record_batches_to_table;
use crate::mcp::service::OxyMcpServer;
use crate::sentry_config;
use crate::service::agent::AgentCLIHandler;
use crate::service::agent::run_agent;
use crate::service::agent::run_agentic_workflow;
use crate::service::eval::EvalEventsHandler;
use crate::service::eval::run_eval;
use crate::service::retrieval::{ReindexInput, SearchInput, reindex, search};
use crate::service::sync::sync_databases;
use crate::service::workflow::run_workflow;
use crate::theme::StyledText;
use crate::theme::detect_true_color_support;
use crate::theme::get_current_theme_mode;
use crate::utils::print_colored_sql;
use crate::workflow::RetryStrategy;
use crate::workflow::loggers::cli::WorkflowCLILogger;
use clap::CommandFactory;
use clap::Parser;
use clap::builder::ValueParser;
use make::handle_make_command;
use minijinja::{Environment, Value};
use model::AgentConfig;
use model::{Config, Semantics, Workflow};
use oxy_semantic::cube::models::DatabaseDetails;
use oxy_semantic::cube::translator::process_semantic_layer_to_cube;
use pyo3::Bound;
use pyo3::FromPyObject;
use pyo3::IntoPyObject;
use pyo3::PyAny;
use pyo3::PyErr;
use pyo3::Python;
use pyo3::types::PyAnyMethods;
use rmcp::transport::SseServer;
use rmcp::{ServiceExt, transport::stdio};
use serve::start_server_and_web_app;
use std::backtrace;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::process::exit;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use init::init;

use dotenv;
use std::net::SocketAddr;
use tracing::{debug, error};

// Constants
const CUBE_CONFIG_DIR_PATH: &str = ".semantics";

/// Get the cube configuration directory path (inside the project directory)
fn get_cube_config_dir() -> Result<PathBuf, OxyError> {
    Ok(resolve_local_project_path()?.join(CUBE_CONFIG_DIR_PATH))
}

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
    /// Build vector embeddings and sync integrations
    ///
    /// Process your project files and create searchable embeddings for
    /// enhanced semantic search and retrieval functionality. Also synchronizes
    /// configured integrations like Omni semantic layer metadata.
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
    /// Migrate the database schema to the latest version
    Migrate,
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
    /// Clean ephemeral data and reset project state
    ///
    /// Remove cached data, vector embeddings, and temporary files to reset
    /// the project to a clean state. Useful for troubleshooting data corruption.
    Clean(CleanArgs),
    /// Start the semantic engine (Cube.js) server for semantic layer queries
    ///
    /// Launch a Cube.js server that provides access to your semantic layer
    /// with pre-configured data sources and schema definitions.
    SemanticEngine(SemanticEngineArgs),
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
    #[clap(long, default_value_t = false, group = "named")]
    retry: bool,

    /// Retry from a specific step in the workflow
    #[clap(long, group = "unnamed", conflicts_with = "named")]
    retry_from: Option<String>,

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
                retry_from: None,
            },
            None => Self {
                file,
                database: None,
                variables: vec![],
                question: None,
                retry: false,
                dry_run: false,
                retry_from: None,
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
pub struct ServeArgs {
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
    /// Authentication mode for the web application (always built-in)
    ///
    /// This flag is deprecated and removed; built-in auth is the only supported mode.
    /// Enable git-based project detection and onboarding
    ///
    /// When enabled, allows starting the server outside of an Oxy project
    /// directory and provides git-based onboarding functionality.
    #[clap(long, default_value_t = false)]
    readonly: bool,
    /// Force HTTP/2 only mode (disable HTTP/1.1)
    ///
    /// When enabled, the server will only accept HTTP/2 connections (h2c).
    /// HTTP/1.1 requests will be rejected. Default supports both protocols.
    #[clap(long, default_value_t = false)]
    http2_only: bool,
    /// TLS certificate file for HTTPS (local development)
    #[clap(long, default_value = "localhost+2.pem")]
    tls_cert: String,
    /// TLS private key file for HTTPS (local development)
    #[clap(long, default_value = "localhost+2-key.pem")]
    tls_key: String,

    #[clap(long, default_value_t = false)]
    cloud: bool,
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

#[derive(Parser, Debug)]
pub struct CleanArgs {
    /// What to clean
    #[clap(subcommand)]
    pub target: CleanTarget,
}

#[derive(Parser, Debug)]
pub enum CleanTarget {
    /// Clear all ephemeral data (database artifacts, vector embeddings, and cache)
    ///
    /// Performs a complete cleanup of all ephemeral data including
    /// the .databases folder (semantic models and build artifacts),
    /// vector embeddings, and cached files.
    All,
    /// Clear only the .databases folder
    ///
    /// Removes the .databases folder which contains semantic models,
    /// dataset schemas, and other build artifacts created during
    /// sync and build operations. User data remains preserved.
    DatabaseFolder,
    /// Clear only vector embeddings and search indexes
    ///
    /// Removes all LanceDB vector databases and search indexes
    /// while preserving the .databases folder and cache files.
    Vectors,
    /// Clear cached files and temporary data
    ///
    /// Removes cached chart files, logs, and other temporary data
    /// while preserving .databases folder and vector embeddings.
    Cache,
}

#[derive(Parser, Debug)]
pub struct SemanticEngineArgs {
    /// Port number for the Cube.js server
    ///
    /// Specify which port to bind the Cube.js semantic engine.
    /// Default is 4000 if not specified.
    #[clap(long, default_value_t = 4000)]
    port: u16,
    /// Host address to bind the Cube.js server
    ///
    /// Specify which host address to bind the Cube.js server.
    /// Default is 0.0.0.0 to listen on all interfaces.
    #[clap(long, default_value = "0.0.0.0")]
    host: String,
    /// Enable development mode with hot reloading
    ///
    /// When enabled, Cube.js will run in development mode with
    /// automatic schema reloading and enhanced debugging.
    #[clap(long, default_value_t = true)]
    dev_mode: bool,
    /// Set log level for Cube.js server
    ///
    /// Control the verbosity of Cube.js logging output.
    /// Options: error, warn, info, debug, trace
    #[clap(long, default_value = "info")]
    log_level: String,
}

async fn handle_workflow_file(
    workflow_name: &PathBuf,
    retry: bool,
    retry_from: Option<String>,
) -> Result<(), OxyError> {
    let project_path = resolve_local_project_path()?;
    let project = ProjectBuilder::new()
        .with_project_path(&project_path)
        .await?
        .with_runs_manager(RunsManager::default(Uuid::nil(), Uuid::nil()).await?)
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;
    // Add Sentry context for workflow execution
    let workflow_name_str = workflow_name
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    sentry_config::add_workflow_context(workflow_name_str, None);

    if let Some(retry_from) = retry_from {
        tracing::debug!(retry_from = %retry_from, "Running workflow from last run with retry from step");
        sentry_config::add_workflow_context(workflow_name_str, Some("retry_from"));

        // Extract the run_id and replay_id from the retry_from string
        let (run_id, replay_id) = if retry_from.contains("::") {
            let parts: Vec<&str> = retry_from.split("::").collect();
            if parts.len() == 2 {
                (
                    parts[0].to_string().parse::<u32>().map_err(|err| {
                        OxyError::ArgumentError(format!(
                            "Invalid replay_id format: {err}. Expected a number."
                        ))
                    })?,
                    parts[1].to_string(),
                )
            } else {
                return Err(OxyError::ArgumentError(
                    "Invalid retry_from format. Expected 'run_id::replay_id'".to_string(),
                ));
            }
        } else {
            return Err(OxyError::ArgumentError(
                "Invalid retry_from format. Expected 'run_id::replay_id'".to_string(),
            ));
        };

        run_workflow(
            workflow_name,
            WorkflowCLILogger,
            RetryStrategy::Retry {
                replay_id: Some(replay_id),
                run_index: run_id,
            },
            None,
            project,
            None,
        )
        .await?;
    } else if retry {
        tracing::debug!("Running workflow from last failed run");
        sentry_config::add_workflow_context(workflow_name_str, Some("retry"));
        run_workflow(
            workflow_name,
            WorkflowCLILogger,
            RetryStrategy::LastFailure,
            None,
            project,
            None,
        )
        .await?;
    } else {
        tracing::debug!("Running workflow without retry");
        sentry_config::add_workflow_context(workflow_name_str, Some("normal"));
        run_workflow(
            workflow_name,
            WorkflowCLILogger,
            RetryStrategy::NoRetry,
            None,
            project,
            None,
        )
        .await?;
    }
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

        // Capture panic in Sentry
        sentry::capture_message(
            &format!("Panic occurred: {}", panic_info),
            sentry::Level::Fatal,
        );
    }));

    // Add breadcrumb for CLI command
    if let Some(ref command) = args.command {
        let command_name = match command {
            SubCommand::Init => "init",
            SubCommand::Run(_) => "run",
            SubCommand::Test(_) => "test",
            SubCommand::Build(_) => "build",
            SubCommand::VecSearch(_) => "vec-search",
            SubCommand::Sync(_) => "sync",
            SubCommand::Validate => "validate",
            SubCommand::Migrate => "migrate",
            SubCommand::Serve(_) => "serve",
            SubCommand::McpSse(_) => "mcp-sse",
            SubCommand::McpStdio(_) => "mcp-stdio",
            SubCommand::SelfUpdate => "self-update",
            SubCommand::TestTheme => "test-theme",
            SubCommand::GenConfigSchema(_) => "gen-config-schema",
            SubCommand::Make(_) => "make",
            SubCommand::Ask(_) => "ask",
            SubCommand::Seed(_) => "seed",
            SubCommand::Clean(_) => "clean",
            SubCommand::SemanticEngine(_) => "semantic-engine",
        };

        sentry_config::add_breadcrumb(
            &format!("Executing CLI command: {}", command_name),
            "cli",
            sentry::Level::Info,
        );
        sentry_config::add_operation_context(command_name, None);
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
            sentry_config::add_operation_context("run", Some(&run_args.file));
            handle_run_command(run_args).await?;
        }
        Some(SubCommand::Test(test_args)) => {
            sentry_config::add_operation_context("test", Some(&test_args.file));
            handle_test_command(test_args).await?;
        }
        Some(SubCommand::Build(build_args)) => {
            sentry_config::add_operation_context("build", None);

            // Synchronize Omni integration if configured
            handle_omni_sync().await?;

            // Build vector embeddings for routing agents
            let project_path = resolve_local_project_path()?.to_string_lossy().to_string();
            let config_manager = ConfigBuilder::new()
                .with_project_path(project_path)?
                .build()
                .await?;
            let secrets_manager = SecretsManager::from_environment()?;
            reindex(ReindexInput {
                config: config_manager.clone(),
                secrets_manager,
                drop_all_tables: build_args.drop_all_tables,
            })
            .await?;

            // Process semantic layer to generate CubeJS schema
            let semantic_dir = resolve_semantics_dir()?;
            if semantic_dir.exists() {
                // target_dir: .semantics/ (inside project directory)
                let target_dir = get_cube_config_dir()?;

                // Clean up existing cube directory for fresh generation
                if target_dir.exists() {
                    std::fs::remove_dir_all(&target_dir).map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to clean cube directory: {}", e))
                    })?;
                }

                // Ensure the target directory exists
                std::fs::create_dir_all(&target_dir).map_err(|e| {
                    OxyError::RuntimeError(format!("Failed to create cube directory: {}", e))
                })?;

                let config_manager_clone = config_manager.clone();
                let databases: HashMap<String, DatabaseDetails> = config_manager_clone
                    .list_databases()
                    .iter()
                    .map(|db| {
                        (
                            db.name.clone(),
                            DatabaseDetails {
                                name: db.name.clone(),
                                db_type: db.dialect(),
                            },
                        )
                    })
                    .collect();

                process_semantic_layer_to_cube(semantic_dir, target_dir, databases).await?;
            } else {
                println!("No semantic directory found at {}", semantic_dir.display());
            }
        }
        Some(SubCommand::VecSearch(search_args)) => {
            sentry_config::add_agent_context(&search_args.agent, Some(&search_args.question));
            let project_path = resolve_local_project_path()?.to_string_lossy().to_string();

            let config_manager = ConfigBuilder::new()
                .with_project_path(project_path)?
                .build()
                .await?;

            let secrets_manager = SecretsManager::from_environment()?;

            search(SearchInput {
                config: config_manager,
                secrets_manager,
                agent_ref: search_args.agent.to_string(),
                query: search_args.question.to_string(),
            })
            .await?;
        }
        Some(SubCommand::Sync(sync_args)) => {
            sentry_config::add_operation_context("sync", None);
            if let Some(ref db) = sync_args.database {
                sentry_config::add_database_context(db, None);
            }
            let config = ConfigBuilder::new()
                .with_project_path(&resolve_local_project_path()?)?
                .build()
                .await?;

            let secrets_manager = SecretsManager::from_environment()?;
            let filter = sync_args
                .database
                .clone()
                .map(|db| (db, sync_args.datasets.clone()));
            debug!(sync_args = ?sync_args, "Syncing");
            println!("🔄Syncing databases");
            let sync_metrics =
                sync_databases(config.clone(), secrets_manager, filter, sync_args.overwrite)
                    .await?;
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
            let config = ConfigBuilder::new()
                .with_project_path(&resolve_local_project_path()?)?
                .build()
                .await?;
            match config.get_config().validate_workflows() {
                Ok(_) => match config.get_config().validate_agents() {
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
            }
        }
        Some(SubCommand::Migrate) => {
            if let Err(e) = migrate().await {
                eprintln!("{}", format!("Migration failed: {e}").error());
                exit(1);
            } else {
                println!("{}", "Migration completed successfully".success());
            }
        }
        Some(SubCommand::Serve(serve_args)) => {
            if let Err(e) = start_server_and_web_app(serve_args).await {
                eprintln!("{}", format!("Server failed: {e}").error());
                exit(1);
            }
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
            let project_path = resolve_local_project_path()?;
            let project = ProjectBuilder::new()
                .with_project_path(&project_path)
                .await?
                .with_runs_manager(RunsManager::default(Uuid::nil(), Uuid::nil()).await?)
                .build()
                .await
                .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;

            let _ = run_agent(
                project.clone(),
                &project.config_manager.get_builder_agent_path().await?,
                ask_args.question,
                AgentCLIHandler::default(),
                vec![],
                None,
            )
            .await?;
        }

        Some(SubCommand::Seed(seed_args)) => {
            handle_seed_command(seed_args).await?;
        }

        Some(SubCommand::Clean(clean_args)) => {
            handle_clean_command(clean_args).await?;
        }

        Some(SubCommand::SemanticEngine(semantic_args)) => {
            handle_semantic_engine_command(semantic_args).await?;
        }

        None => {
            Args::command().print_help().unwrap();
        }
    }

    Ok(())
}

async fn handle_omni_sync() -> Result<(), OxyError> {
    use crate::service::omni_sync::OmniSyncService;
    use omni::{OmniApiClient, OmniError as AdapterOmniError};

    // Load configuration to get Omni integration settings
    let project_path = resolve_local_project_path()?;

    let project = ProjectBuilder::new()
        .with_project_path(&project_path)
        .await?
        .with_runs_manager(RunsManager::default(Uuid::nil(), Uuid::nil()).await?)
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;

    let config = project.config_manager.clone();

    // Get all Omni integration configurations - if none found, skip silently
    let omni_integrations: Vec<_> = config
        .get_config()
        .integrations
        .iter()
        .filter_map(|integration| match &integration.integration_type {
            crate::config::model::IntegrationType::Omni(omni_integration) => {
                Some((integration.name.clone(), omni_integration.clone()))
            }
        })
        .collect();

    if omni_integrations.is_empty() {
        // No Omni integrations configured, skip silently
        return Ok(());
    }

    println!(
        "🔗 Synchronizing {} Omni integration(s)...",
        omni_integrations.len()
    );

    let mut all_sync_results = Vec::new();
    let mut total_successful_topics = Vec::new();

    for (integration_name, omni_integration) in omni_integrations {
        println!("\n🔗 Processing integration: {}", integration_name);

        // Resolve API key from environment variable
        let api_key = project
            .secrets_manager
            .resolve_secret(&omni_integration.api_key_var)
            .await?
            .unwrap();
        let base_url = omni_integration.base_url.clone();
        let topics = omni_integration.topics.clone();

        // Sync all configured topics for this integration
        println!("🔄 Synchronizing Omni metadata for {} topics", topics.len());
        let topics_to_sync: Vec<_> = topics.iter().collect();

        // Create API client
        let api_client =
            OmniApiClient::new(base_url.clone(), api_key.clone()).map_err(|e| match e {
                AdapterOmniError::ConfigError(msg) => {
                    OxyError::ConfigurationError(format!("Omni configuration error: {}", msg))
                }
                _ => OxyError::RuntimeError(format!("Failed to create Omni API client: {}", e)),
            })?;

        // Create sync service
        let sync_service =
            OmniSyncService::new(api_client, &project_path, integration_name.clone());

        // Perform synchronization for each topic in this integration
        println!("📥 Fetching metadata from Omni API...");

        let mut integration_results = Vec::new();
        for topic in &topics_to_sync {
            println!(
                "  📋 Syncing topic: {} (model: {})",
                topic.name, topic.model_id
            );
            let sync_result = sync_service
                .sync_metadata(&topic.model_id, &topic.name)
                .await
                .map_err(|e| {
                    OxyError::RuntimeError(format!(
                        "Sync operation failed for topic '{}' (model '{}'): {}",
                        topic.name, topic.model_id, e
                    ))
                })?;
            integration_results.push(sync_result);
        }

        // Collect results for this integration
        if let Some(first_result) = integration_results.into_iter().next() {
            total_successful_topics.extend(first_result.successful_topics.clone());
            all_sync_results.push(first_result);
        }
    }

    // Display overall results
    println!("\n{}", "🎉 Omni synchronization completed!".success());

    if !all_sync_results.is_empty() {
        let overall_success = all_sync_results.iter().all(|r| r.is_success());
        let partial_success = all_sync_results.iter().any(|r| r.is_partial_success());

        if overall_success {
            println!(
                "{}",
                "All integrations synchronized successfully.".success()
            );
        } else if partial_success {
            println!(
                "{}",
                "Partial synchronization completed with some errors.".warning()
            );
            // Show error summaries from failed integrations
            for sync_result in &all_sync_results {
                if let Some(error_summary) = sync_result.error_summary() {
                    println!("\n{}", "Errors encountered:".warning());
                    println!("{}", error_summary.error());
                }
            }
        } else {
            println!("{}", "Some integrations failed to synchronize.".error());
            for sync_result in &all_sync_results {
                if let Some(error_summary) = sync_result.error_summary() {
                    println!("\n{}", "Errors encountered:".error());
                    println!("{}", error_summary.error());
                }
            }
            return Err(OxyError::RuntimeError(
                "Some Omni sync operations failed".to_string(),
            ));
        }

        // Show all successful topics across all integrations
        if !total_successful_topics.is_empty() {
            println!("\n{}", "Successfully synchronized topics:".success());
            for topic in &total_successful_topics {
                println!("  ✅ {}", topic);
            }
        }
    }

    Ok(())
}

async fn handle_agent_file(file_path: &PathBuf, question: Option<String>) -> Result<(), OxyError> {
    let agent_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    sentry_config::add_agent_context(agent_name, question.as_deref());

    let question = question.ok_or_else(|| {
        OxyError::ArgumentError("Question is required for agent files".to_string())
    })?;
    let project_path = resolve_local_project_path()?;

    let project_manager = ProjectBuilder::new()
        .with_project_path(&project_path)
        .await?
        .with_runs_manager(RunsManager::default(Uuid::nil(), Uuid::nil()).await?)
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;

    let _ = run_agent(
        project_manager,
        file_path,
        question,
        AgentCLIHandler::default(),
        vec![],
        None,
    )
    .await?;
    Ok(())
}

async fn handle_agentic_workflow_file(
    file_path: &PathBuf,
    question: Option<String>,
) -> Result<(), OxyError> {
    let agent_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    sentry_config::add_agent_context(agent_name, question.as_deref());

    let question = question.ok_or_else(|| {
        OxyError::ArgumentError("Question is required for agentic workflow files".to_string())
    })?;
    let project_path = resolve_local_project_path()?;

    let project_manager = ProjectBuilder::new()
        .with_project_path(&project_path)
        .await?
        .with_runs_manager(RunsManager::default(Uuid::nil(), Uuid::nil()).await?)
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;

    println!("🤖 Running agentic workflow: {}", file_path.display());
    let res = run_agentic_workflow(
        project_manager,
        file_path,
        question,
        AgentCLIHandler::default(),
        vec![],
    )
    .await?;
    println!("{:?}", res);
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

    // Add Sentry context for SQL execution
    sentry_config::add_database_context(&database, Some("sql_file"));
    sentry_config::add_operation_context("sql", Some(&file_path.to_string_lossy()));

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
    let secrets_manager = SecretsManager::from_environment()?;
    let connector =
        Connector::from_database(&database, config, &secrets_manager, None, None).await?;
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
                handle_workflow_file(&file_path, run_args.retry, run_args.retry_from).await?;
                Ok(RunResult::Workflow)
            } else if file.ends_with(".agent.yml") {
                handle_agent_file(&file_path, run_args.question).await?;
                Ok(RunResult::Agent)
            } else if file.ends_with(".aw.yml") {
                handle_agentic_workflow_file(&file_path, run_args.question).await?;
                Ok(RunResult::Agent)
            } else {
                Err(OxyError::ArgumentError(
                    "Invalid YAML file. Must be either *.workflow.yml or *.agent.yml".into(),
                ))
            }
        }
        Some("sql") => {
            let config = ConfigBuilder::new()
                .with_project_path(&resolve_local_project_path()?)?
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
    let project_path = match resolve_local_project_path() {
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

    let project_path = resolve_local_project_path()?;

    let project_manager = ProjectBuilder::new()
        .with_project_path(&project_path)
        .await?
        .with_runs_manager(RunsManager::default(Uuid::nil(), Uuid::nil()).await?)
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;

    run_eval(
        project_manager,
        &file_path,
        None,
        EvalEventsHandler::new(test_args.quiet),
    )
    .await?;
    Ok(())
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

async fn handle_clean_command(clean_args: CleanArgs) -> Result<(), OxyError> {
    use clean::*;

    let config_manager = ConfigBuilder::new()
        .with_project_path(&resolve_local_project_path()?)?
        .build()
        .await?;

    match clean_args.target {
        CleanTarget::All => {
            clean_all(true, &config_manager).await?;
        }
        CleanTarget::DatabaseFolder => {
            clean_database_folder(true, &config_manager).await?;
        }
        CleanTarget::Vectors => {
            clean_vectors(true, &config_manager).await?;
        }
        CleanTarget::Cache => {
            clean_cache(true, &config_manager).await?;
        }
    }
    Ok(())
}

async fn handle_semantic_engine_command(semantic_args: SemanticEngineArgs) -> Result<(), OxyError> {
    sentry_config::add_operation_context("semantic-engine", None);

    // Ensure we're in a valid project
    let project_path = resolve_local_project_path()?;

    // Check if semantic layer exists
    let semantic_dir = resolve_semantics_dir()?;
    if !semantic_dir.exists() {
        return Err(OxyError::ConfigurationError(
            "No semantic layer found. Please create a 'semantics' directory with your semantic definitions.".to_string()
        ));
    }

    // Get config first to get database details
    let config = ConfigBuilder::new()
        .with_project_path(&project_path)?
        .build()
        .await?;

    // Ensure cube configuration directory exists
    let cube_config_dir = get_cube_config_dir()?;

    if !cube_config_dir.exists() {
        println!("🔄 Generating Cube.js configuration from semantic layer...");

        // Get database details from config
        let databases: HashMap<String, DatabaseDetails> = config
            .list_databases()
            .iter()
            .map(|db| {
                (
                    db.name.clone(),
                    DatabaseDetails {
                        name: db.name.clone(),
                        db_type: db.dialect(),
                    },
                )
            })
            .collect();

        // Process semantic layer to generate CubeJS schema
        process_semantic_layer_to_cube(semantic_dir.clone(), cube_config_dir.clone(), databases)
            .await?;
        println!("✅ Cube.js configuration generated successfully");
    } else {
        // Clean up existing cube directory and regenerate for isolation
        println!("🧹 Cleaning existing Cube.js configuration...");
        std::fs::remove_dir_all(&cube_config_dir).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to clean cube directory: {}", e))
        })?;

        println!("🔄 Regenerating Cube.js configuration from semantic layer...");

        // Get database details from config
        let databases: HashMap<String, DatabaseDetails> = config
            .list_databases()
            .iter()
            .map(|db| {
                (
                    db.name.clone(),
                    DatabaseDetails {
                        name: db.name.clone(),
                        db_type: db.dialect(),
                    },
                )
            })
            .collect();

        // Process semantic layer to generate CubeJS schema
        process_semantic_layer_to_cube(semantic_dir.clone(), cube_config_dir.clone(), databases)
            .await?;
        println!("✅ Cube.js configuration generated successfully");
    }

    // Check if Docker is available
    let docker_check = Command::new("docker").args(["--version"]).output();

    match docker_check {
        Ok(output) if output.status.success() => {
            println!("🐳 Docker detected, starting Cube.js container...");
        }
        _ => {
            return Err(OxyError::RuntimeError(
                "Docker is required to run the semantic engine. Please install Docker and try again.".to_string()
            ));
        }
    }

    // Prepare environment variables for Cube.js
    let mut env_vars = vec![
        format!("CUBEJS_DEV_MODE={}", semantic_args.dev_mode),
        format!("CUBEJS_LOG_LEVEL={}", semantic_args.log_level),
    ];

    if let Some(default_db) = config.default_database_ref()
        && let Ok(db_config) = config.resolve_database(default_db)
    {
        // Add database URL to environment
        let db_url = match &db_config.database_type {
            crate::config::model::DatabaseType::Postgres(pg_config) => {
                format!(
                    "postgresql://{}:{}@{}:{}/{}",
                    pg_config.user.as_deref().unwrap_or("postgres"),
                    pg_config.password.as_deref().unwrap_or(""),
                    pg_config.host.as_deref().unwrap_or("localhost"),
                    pg_config.port.as_deref().unwrap_or("5432"),
                    pg_config.database.as_deref().unwrap_or("postgres")
                )
            }
            crate::config::model::DatabaseType::Mysql(mysql_config) => {
                format!(
                    "mysql://{}:{}@{}:{}/{}",
                    mysql_config.user.as_deref().unwrap_or("root"),
                    mysql_config.password.as_deref().unwrap_or(""),
                    mysql_config.host.as_deref().unwrap_or("localhost"),
                    mysql_config.port.as_deref().unwrap_or("3306"),
                    mysql_config.database.as_deref().unwrap_or("mysql")
                )
            }
            _ => {
                tracing::warn!("Database type not supported for Cube.js connection");
                String::new()
            }
        };

        if !db_url.is_empty() {
            env_vars.push(format!("CUBEJS_DB_URL={}", db_url));
        }
    }

    // Build Docker command
    let mut docker_cmd = Command::new("docker");
    docker_cmd
        .args(["run", "--rm", "-it"])
        .args(["-p", &format!("{}:4000", semantic_args.port)])
        .args(["-v", &format!("{}:/cube/conf", cube_config_dir.display())])
        .args(["-v", &format!("{}/.db:/cube/.db", project_path.display())]);

    // Add environment variables
    for env_var in env_vars {
        docker_cmd.args(["-e", &env_var]);
    }

    // Allow configuring Cube version ?
    docker_cmd.args(["cubejs/cube:v1.3.81"]);

    let display_host = if semantic_args.host == "0.0.0.0" {
        "localhost"
    } else {
        &semantic_args.host
    };

    println!(
        "{} {}",
        "🚀 Starting Cube.js semantic engine at".text(),
        format!("http://{}:{}", display_host, semantic_args.port).secondary()
    );

    println!(
        "{}",
        "📊 Cube.js Developer Playground will be available for testing queries".info()
    );
    println!("{}", "Press Ctrl+C to stop the semantic engine".tertiary());

    // Execute the Docker command
    let status = docker_cmd
        .status()
        .map_err(|e| OxyError::RuntimeError(format!("Failed to start Cube.js container: {}", e)))?;

    if !status.success() {
        return Err(OxyError::RuntimeError(
            "Cube.js container exited with non-zero status".to_string(),
        ));
    }

    Ok(())
}
