mod a2a;
pub mod clean;
mod init;
mod intent;
mod make;
mod mcp;
mod migrate;
mod seed;
mod serve;
mod start;
mod status;

use crate::cli::commands::mcp::{start_mcp_sse_server, start_mcp_stdio};
use crate::cli::commands::migrate::migrate;
use crate::server::service::agent::AgentCLIHandler;
use crate::server::service::agent::run_agent;
use crate::server::service::agent::run_agentic_workflow;
use crate::server::service::eval::EvalEventsHandler;
use crate::server::service::eval::run_eval;
use crate::server::service::retrieval::{ReindexInput, SearchInput, reindex, search};
use crate::server::service::sync::sync_databases;
use crate::server::service::workflow::run_workflow;
use ::oxy::adapters::project::builder::ProjectBuilder;
use ::oxy::adapters::runs::RunsManager;
use ::oxy::adapters::secrets::SecretsManager;
use ::oxy::checkpoint::types::RetryStrategy;
use ::oxy::config::model::AppConfig;
use ::oxy::config::*;
use ::oxy::connector::Connector;
use ::oxy::database::docker;
use ::oxy::execute::types::utils::record_batches_to_table;
use ::oxy::sentry_config;
use ::oxy::theme::StyledText;
use ::oxy::theme::detect_true_color_support;
use ::oxy::theme::get_current_theme_mode;
use ::oxy::utils::print_colored_sql;
use clap::CommandFactory;
use clap::Parser;
use clap::builder::ValueParser;
use make::handle_make_command;
use minijinja::{Environment, Value};
use model::AgentConfig;
use model::{Config, Semantics, Workflow};
use oxy_globals::GlobalRegistry;
use oxy_semantic::cube::models::DatabaseDetails;
use oxy_semantic::cube::translator::process_semantic_layer_to_cube;
use oxy_shared::errors::OxyError;
use oxy_workflow::loggers::cli::WorkflowCLILogger;
use serve::start_server_and_web_app;
use std::backtrace;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::process::exit;
use uuid::Uuid;

use init::init;

use dotenv;
use tracing::{debug, error};

// Constants
const CUBE_CONFIG_DIR_PATH: &str = ".semantics";

/// Get the cube configuration directory path (inside the project directory)
pub fn get_cube_config_dir() -> Result<PathBuf, OxyError> {
    Ok(resolve_local_project_path()?.join(CUBE_CONFIG_DIR_PATH))
}

/// Clear all contents of a directory without removing the directory itself
///
/// This is useful when the directory is mounted as a Docker volume, where
/// removing the directory itself would fail with "Device or resource busy"
fn clear_directory_contents(dir_path: &PathBuf) -> Result<(), OxyError> {
    for entry in std::fs::read_dir(dir_path)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to read directory: {}", e)))?
    {
        let entry = entry.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to read directory entry: {}", e))
        })?;
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| {
                OxyError::RuntimeError(format!(
                    "Failed to remove directory {}: {}",
                    path.display(),
                    e
                ))
            })?;
        } else {
            std::fs::remove_file(&path).map_err(|e| {
                OxyError::RuntimeError(format!("Failed to remove file {}: {}", path.display(), e))
            })?;
        }
    }
    Ok(())
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
                (Some(server), Some(repo), Some(sha)) => format!("{}/{}/commit/{} ({})", server, repo, sha, sha),
                _ => option_env!("GITHUB_SHA").unwrap_or("unknown").to_string(),
            },
            workflow_link = match (
                option_env!("GITHUB_SERVER_URL"),
                option_env!("GITHUB_REPOSITORY"),
                option_env!("GITHUB_RUN_ID")
            ) {
                (Some(server), Some(repo), Some(run_id)) => format!("{}/{}/actions/runs/{} ({})", server, repo, run_id, run_id),
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
    #[clap(subcommand)]
    pub transport: McpTransport,
}

#[derive(Parser, Debug)]
enum McpTransport {
    /// Start MCP server with stdio transport
    ///
    /// Launch an MCP server using standard input/output for direct
    /// integration with local AI tools and development environments.
    Stdio {
        /// Path to the Oxy project directory (required)
        ///
        /// Specify the root directory of your Oxy project where
        /// config.yml and other project files are located.
        project_path: PathBuf,
    },
    /// Start MCP server with Server-Sent Events transport
    ///
    /// Launch a web-accessible MCP server that enables integration with
    /// MCP-compatible AI tools and applications via HTTP/SSE.
    Sse {
        /// Path to the Oxy project directory (optional, defaults to current directory)
        ///
        /// Specify the root directory of your Oxy project where
        /// config.yml and other project files are located.
        project_path: Option<PathBuf>,
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
    },
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
    /// Execute workflow (.workflow.yml or .automation.yml), agent (.agent.yml), or SQL (.sql) files
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
    /// Start MCP (Model Context Protocol) server
    ///
    /// Launch an MCP server with either stdio or SSE transport for
    /// integration with AI tools and development environments.
    Mcp(McpArgs),
    /// Migrate the database schema to the latest version
    Migrate,
    /// Start with Docker PostgreSQL (recommended)
    ///
    /// Launch PostgreSQL in Docker and start the Oxy web server.
    /// Uses postgres:18-alpine container for modern PostgreSQL features.
    /// Data persists in Docker volume 'oxy-postgres-data'.
    Start(ServeArgs),
    /// Start the web server (requires OXY_DATABASE_URL)
    ///
    /// Launch the Oxy server. Requires OXY_DATABASE_URL environment variable
    /// to be set to a PostgreSQL connection string.
    /// For automatic PostgreSQL setup, use 'oxy start' instead.
    Serve(ServeArgs),
    /// Show status of Oxy services and Docker containers
    ///
    /// Display the current status of PostgreSQL, Docker, and database
    /// connectivity along with helpful troubleshooting commands.
    Status,
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
    /// Prepare Cube.js configuration from semantic layer without starting server
    ///
    /// Generate Cube.js schema files from your semantic layer definitions.
    /// Useful for deploying to containerized environments or when running
    /// Cube.js separately from the Oxy CLI.
    PrepareSemanticEngine(PrepareSemanticEngineArgs),
    /// Start A2A (Agent-to-Agent) protocol server
    ///
    /// Launch an A2A server that exposes configured Oxy agents for
    /// external agent communication using JSON-RPC or HTTP+JSON protocols.
    A2a(A2aArgs),
    /// Intent classification and clustering
    ///
    /// Discover and classify user intents from agent questions using
    /// unsupervised clustering techniques (HDBSCAN) and LLM labeling.
    Intent(intent::IntentArgs),
}

#[derive(Parser, Debug)]
pub struct MakeArgs {
    /// Path to the workflow file to execute
    file: String,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    /// Path to the file to execute (.sql, .workflow.yml, .automation.yml, or .agent.yml)
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

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Pretty,
    Json,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum ThresholdMode {
    /// Average of all test accuracies must meet threshold
    Average,
    /// All individual test accuracies must meet threshold
    All,
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
    /// Output format (pretty or json)
    #[clap(long, value_enum, default_value = "pretty")]
    format: OutputFormat,
    /// Minimum accuracy threshold (0.0-1.0). Exit with code 1 if accuracy is below this value
    #[clap(long, value_name = "THRESHOLD")]
    min_accuracy: Option<f32>,
    /// Threshold mode: 'average' checks average of all tests, 'all' checks each test individually
    #[clap(long, value_enum, default_value = "average")]
    threshold_mode: ThresholdMode,
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

pub use crate::cli::{A2aArgs, ServeArgs};

// Removed duplicate A2aArgs and ServeArgs - using from cli.rs instead

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

#[derive(Parser, Debug)]
pub struct PrepareSemanticEngineArgs {
    /// Output directory for generated Cube.js configuration
    ///
    /// Specify where to write the generated Cube.js schema files.
    /// If not specified, uses the default .semantics directory.
    #[clap(long)]
    output_dir: Option<PathBuf>,
    /// Force regeneration even if configuration already exists
    ///
    /// Clean and regenerate all Cube.js configuration files.
    #[clap(long, default_value_t = false)]
    force: bool,
}

async fn handle_workflow_file(
    workflow_name: &PathBuf,
    retry: bool,
    retry_from: Option<String>,
) -> Result<(), OxyError> {
    let project_path = resolve_local_project_path()?;
    let project = ProjectBuilder::new(Uuid::nil())
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
            project,
            None,
            None,
            None, // No globals override from CLI
            Some(crate::server::service::agent::ExecutionSource::Cli),
            None, // No authenticated user in CLI context
        )
        .await?;
    } else if retry {
        tracing::debug!("Running workflow from last failed run");
        sentry_config::add_workflow_context(workflow_name_str, Some("retry"));
        run_workflow(
            workflow_name,
            WorkflowCLILogger,
            RetryStrategy::LastFailure,
            project,
            None,
            None,
            None, // No globals override from CLI
            Some(crate::server::service::agent::ExecutionSource::Cli),
            None, // No authenticated user in CLI context
        )
        .await?;
    } else {
        tracing::debug!("Running workflow without retry");
        sentry_config::add_workflow_context(workflow_name_str, Some("normal"));
        run_workflow(
            workflow_name,
            WorkflowCLILogger,
            RetryStrategy::NoRetry { variables: None },
            project,
            None,
            None,
            None, // No globals override from CLI
            Some(crate::server::service::agent::ExecutionSource::Cli),
            None, // No authenticated user in CLI context
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
            SubCommand::Start(_) => "start",
            SubCommand::Serve(_) => "serve",
            SubCommand::Status => "status",
            SubCommand::Mcp(_) => "mcp",
            SubCommand::SelfUpdate => "self-update",
            SubCommand::TestTheme => "test-theme",
            SubCommand::GenConfigSchema(_) => "gen-config-schema",
            SubCommand::Make(_) => "make",
            SubCommand::Ask(_) => "ask",
            SubCommand::Seed(_) => "seed",
            SubCommand::Clean(_) => "clean",
            SubCommand::SemanticEngine(_) => "semantic-engine",
            SubCommand::PrepareSemanticEngine(_) => "prepare-semantic-engine",
            SubCommand::A2a(_) => "a2a",
            SubCommand::Intent(_) => "intent",
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
                    // Instead of removing the directory itself (which fails when mounted as a volume),
                    // remove all contents within it
                    clear_directory_contents(&target_dir)?;
                } else {
                    // Ensure the target directory exists if it doesn't
                    std::fs::create_dir_all(&target_dir).map_err(|e| {
                        OxyError::RuntimeError(format!("Failed to create cube directory: {}", e))
                    })?;
                }

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

                process_semantic_layer_to_cube(
                    semantic_dir,
                    target_dir,
                    databases,
                    config_manager.get_globals_registry(),
                )
                .await?;
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
        Some(SubCommand::A2a(a2a_args)) => {
            if let Err(e) = a2a::start_a2a_server(a2a_args).await {
                eprintln!("{}", format!("A2A server failed: {e}").error());
            }
        }
        Some(SubCommand::Start(serve_args)) => {
            if let Err(e) = start::start_database_and_server(serve_args).await {
                eprintln!("{}", format!("Failed to start: {e}").error());
                exit(1);
            }
        }
        Some(SubCommand::Serve(serve_args)) => {
            if let Err(e) = start_server_and_web_app(serve_args).await {
                eprintln!("{}", format!("Server failed: {e}").error());
                exit(1);
            }
        }
        Some(SubCommand::Status) => {
            if let Err(e) = status::show_status().await {
                eprintln!("{}", format!("Failed to get status: {e}").error());
                exit(1);
            }
        }
        Some(SubCommand::Mcp(mcp_args)) => match mcp_args.transport {
            McpTransport::Stdio { project_path } => {
                let env_path = project_path.join(".env");
                dotenv::from_path(env_path).ok();
                let _ = start_mcp_stdio(project_path).await;
            }
            McpTransport::Sse {
                project_path,
                port,
                host,
            } => {
                let project_path = match project_path {
                    Some(path) => path,
                    None => resolve_local_project_path()?,
                };
                let cancellation_token = start_mcp_sse_server(port, host, project_path)
                    .await
                    .expect("Failed to start MCP SSE server");

                tokio::signal::ctrl_c().await.unwrap();
                println!("Shutting down server...");
                cancellation_token.cancel();
            }
        },
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
            let project = ProjectBuilder::new(Uuid::nil())
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
                None,
                None, // No globals from CLI
                None, // No variables from CLI (yet)
                Some(crate::server::service::agent::ExecutionSource::Cli),
                None, // No sandbox info from CLI
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

        Some(SubCommand::PrepareSemanticEngine(prepare_args)) => {
            handle_prepare_semantic_engine_command(prepare_args).await?;
        }

        Some(SubCommand::Intent(intent_args)) => {
            intent::handle_intent_command(intent_args).await?;
        }

        None => {
            Args::command().print_help().unwrap();
        }
    }

    Ok(())
}

async fn handle_omni_sync() -> Result<(), OxyError> {
    use crate::server::service::omni_sync::OmniSyncService;
    use omni::{OmniApiClient, OmniError as AdapterOmniError};

    // Load configuration to get Omni integration settings
    let project_path = resolve_local_project_path()?;

    let project = ProjectBuilder::new(Uuid::nil())
        .with_project_path(&project_path)
        .await?
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
            ::oxy::config::model::IntegrationType::Omni(omni_integration) => {
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

    let project_manager = ProjectBuilder::new(Uuid::nil())
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
        None,
        None, // No globals from CLI
        None, // No variables from CLI (yet)
        Some(crate::server::service::agent::ExecutionSource::Cli),
        None, // No sandbox info from CLI
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

    let project_manager = ProjectBuilder::new(Uuid::nil())
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
        Connector::from_database(&database, config, &secrets_manager, None, None, None).await?;
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
            if file.ends_with(".workflow.yml") || file.ends_with(".automation.yml") {
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
                    "Invalid YAML file. Must be either *.workflow.yml, *.automation.yml, or *.agent.yml".into(),
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
            "Invalid file extension. Must be .workflow.yml, .automation.yml, .agent.yml, or .sql"
                .into(),
        )),
    }
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

    // Validate threshold if provided
    if let Some(threshold) = test_args.min_accuracy
        && !(0.0..=1.0).contains(&threshold)
    {
        return Err(OxyError::ConfigurationError(format!(
            "min-accuracy must be between 0.0 and 1.0, got: {threshold}"
        )));
    }

    let project_path = resolve_local_project_path()?;

    let project_manager = ProjectBuilder::new(Uuid::nil())
        .with_project_path(&project_path)
        .await?
        .with_runs_manager(RunsManager::default(Uuid::nil(), Uuid::nil()).await?)
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;

    // Run evaluation and capture results
    let results = run_eval(
        project_manager,
        &file_path,
        None,
        EvalEventsHandler::new(test_args.quiet),
    )
    .await?;

    // Select reporter based on format
    use crate::integrations::eval::{JsonReporter, MetricKind, PrettyReporter, Reporter};

    let reporter: Box<dyn Reporter> = match test_args.format {
        OutputFormat::Pretty => Box::new(PrettyReporter {
            quiet: test_args.quiet,
        }),
        OutputFormat::Json => Box::new(JsonReporter),
    };

    // Generate output
    let mut stdout = std::io::stdout();
    reporter.report(&results, &mut stdout)?;

    // Check threshold if provided
    if let Some(min_accuracy) = test_args.min_accuracy {
        // Collect all accuracy scores from all results
        let accuracies: Vec<f32> = results
            .iter()
            .flat_map(|r| &r.metrics)
            .filter_map(|m| match m {
                MetricKind::Similarity(s) => Some(s.score),
                _ => None,
            })
            .collect();

        if accuracies.is_empty() {
            eprintln!("Warning: --min-accuracy specified but no accuracy metrics found");
        } else {
            match test_args.threshold_mode {
                ThresholdMode::Average => {
                    // Check if average accuracy meets threshold
                    let avg_accuracy: f32 =
                        accuracies.iter().sum::<f32>() / accuracies.len() as f32;
                    if avg_accuracy < min_accuracy {
                        return Err(OxyError::RuntimeError(format!(
                            "Average accuracy {:.4} below threshold {:.4}",
                            avg_accuracy, min_accuracy
                        )));
                    }
                }
                ThresholdMode::All => {
                    // Check if all individual accuracies meet threshold
                    let failing_tests: Vec<(usize, f32)> = accuracies
                        .iter()
                        .enumerate()
                        .filter(|(_, acc)| **acc < min_accuracy)
                        .map(|(i, acc)| (i, *acc))
                        .collect();

                    if !failing_tests.is_empty() {
                        let failure_msg = failing_tests
                            .iter()
                            .map(|(i, acc)| format!("Test {}: {:.4}", i + 1, acc))
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Err(OxyError::RuntimeError(format!(
                            "{} test(s) below threshold {:.4}: {}",
                            failing_tests.len(),
                            min_accuracy,
                            failure_msg
                        )));
                    }
                }
            }
        }
    }

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

    // Get config first to get database details
    let config = ConfigBuilder::new()
        .with_project_path(&project_path)?
        .build()
        .await?;

    // Ensure cube configuration directory exists
    let cube_config_dir = get_cube_config_dir()?;

    // Always regenerate configuration for isolation
    generate_cube_config(cube_config_dir.clone(), true, config.get_globals_registry()).await?;

    // Check if Docker is available
    println!("{}", "🔍 Checking Docker availability...".text());
    docker::check_docker_available().await?;
    println!("{}", "   ✓ Docker is available\n".success());

    // Get database URL
    let db_url = if let Some(default_db) = config.default_database_ref()
        && let Ok(db_config) = config.resolve_database(default_db)
    {
        match &db_config.database_type {
            ::oxy::config::model::DatabaseType::Postgres(pg_config) => {
                format!(
                    "postgresql://{}:{}@{}:{}/{}",
                    pg_config.user.as_deref().unwrap_or("postgres"),
                    pg_config.password.as_deref().unwrap_or(""),
                    pg_config.host.as_deref().unwrap_or("localhost"),
                    pg_config.port.as_deref().unwrap_or("5432"),
                    pg_config.database.as_deref().unwrap_or("postgres")
                )
            }
            ::oxy::config::model::DatabaseType::Mysql(mysql_config) => {
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
        }
    } else {
        String::new()
    };

    if db_url.is_empty() {
        return Err(OxyError::ConfigurationError(
            "No default database configured. Please configure a database in your oxy.toml file."
                .to_string(),
        ));
    }

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
    println!(
        "{}",
        "Press Ctrl+C to stop the semantic engine\n".tertiary()
    );

    // Start Cube.js container
    println!("{}", "🐳 Starting Cube.js container...".text());
    println!("{}", format!("   Container: {}", "oxy-cubejs").tertiary());
    println!(
        "{}",
        format!("   Image: {}", "cubejs/cube:v1.3.81").tertiary()
    );
    println!(
        "{}",
        format!("   Port: {}:4000", semantic_args.port).tertiary()
    );

    docker::start_cubejs_container(
        cube_config_dir.display().to_string(),
        project_path.display().to_string(),
        db_url,
        semantic_args.dev_mode,
        semantic_args.log_level.clone(),
    )
    .await?;

    println!("{}", "   ✓ Cube.js container started\n".success());

    // Wait for Cube.js to be ready
    println!("{}", "⏳ Waiting for Cube.js to be ready...".text());
    docker::wait_for_cubejs_ready(docker::CUBEJS_READY_TIMEOUT_SECS).await?;
    println!("{}", "✓ Cube.js is ready".success());
    println!(
        "{}",
        format!(
            "   Access at: http://{}:{}\n",
            display_host, semantic_args.port
        )
        .tertiary()
    );

    println!("{}", "💡 Useful Docker Commands:".text());
    println!(
        "{}",
        "   View logs:        docker logs oxy-cubejs".secondary()
    );
    println!(
        "{}",
        "   Follow logs:      docker logs -f oxy-cubejs".secondary()
    );
    println!(
        "{}",
        "   Stop container:   docker stop oxy-cubejs".secondary()
    );
    println!();

    // Wait for Ctrl+C signal
    println!("{}", "Container is running. Press Ctrl+C to stop...".text());
    tokio::signal::ctrl_c().await.map_err(|e| {
        OxyError::RuntimeError(format!("Failed to listen for shutdown signal: {}", e))
    })?;

    println!("\n{}", "🛑 Stopping Cube.js container...".text());
    docker::stop_cubejs_container().await?;
    println!("{}", "   ✓ Cube.js container stopped".success());

    Ok(())
}

/// Generate Cube.js configuration from semantic layer
pub async fn generate_cube_config(
    cube_config_dir: PathBuf,
    force: bool,
    globals_registry: GlobalRegistry,
) -> Result<(), OxyError> {
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

    if cube_config_dir.exists() && !force {
        println!(
            "✅ Cube.js configuration already exists at {}",
            cube_config_dir.display()
        );
        return Ok(());
    }

    if cube_config_dir.exists() {
        // Clean up existing cube directory contents and regenerate
        println!("🧹 Cleaning existing Cube.js configuration...");
        // Instead of removing the directory itself (which fails when mounted as a volume),
        // remove all contents within it
        clear_directory_contents(&cube_config_dir)?;
    }

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
    process_semantic_layer_to_cube(
        semantic_dir.clone(),
        cube_config_dir.clone(),
        databases,
        globals_registry,
    )
    .await?;

    println!(
        "✅ Cube.js configuration generated successfully at {}",
        cube_config_dir.display()
    );
    Ok(())
}

async fn handle_prepare_semantic_engine_command(
    prepare_args: PrepareSemanticEngineArgs,
) -> Result<(), OxyError> {
    sentry_config::add_operation_context("prepare-semantic-engine", None);

    let cube_config_dir = prepare_args
        .output_dir
        .unwrap_or_else(|| get_cube_config_dir().unwrap());

    let config_manager = ConfigBuilder::new()
        .with_project_path(&resolve_local_project_path()?)?
        .build()
        .await?;

    generate_cube_config(
        cube_config_dir,
        prepare_args.force,
        config_manager.get_globals_registry(),
    )
    .await?;

    println!();
    println!(
        "{}",
        "📦 Cube.js configuration is ready for deployment".text()
    );
    println!(
        "{}",
        "You can now run Cube.js natively or in a container using the generated config".tertiary()
    );

    Ok(())
}
