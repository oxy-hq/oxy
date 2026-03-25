mod a2a;
pub mod clean;
pub mod export_chart;
mod init;
mod intent;
mod looker;
mod make;
mod mcp;
mod migrate;
pub mod run;
mod seed;
mod serve;
mod start;
mod status;

use crate::cli::commands::mcp::{start_mcp_sse_server, start_mcp_stdio};
use crate::cli::commands::migrate::migrate;
use crate::cli::commands::run::{RunArgs, handle_run_command};
use crate::server::service::agent::AgentCLIHandler;
use crate::server::service::agent::run_agent;
use crate::server::service::eval::EvalEventsHandler;
use crate::server::service::eval::run_eval_with_tag;
use crate::server::service::retrieval::{ReindexInput, SearchInput, reindex, search};
use crate::server::service::sync::sync_databases;
use ::oxy::adapters::project::builder::ProjectBuilder;
use ::oxy::adapters::runs::RunsManager;
use ::oxy::adapters::secrets::SecretsManager;
use ::oxy::config::model::AppConfig;
use ::oxy::config::test_config::TestFileConfig;
use ::oxy::config::*;
use ::oxy::sentry_config;
use ::oxy::theme::StyledText;
use ::oxy::theme::detect_true_color_support;
use ::oxy::theme::get_current_theme_mode;
use clap::CommandFactory;
use clap::Parser;
use make::handle_make_command;
use model::AgentConfig;
use model::{Config, Semantics, Workflow};
use oxy_shared::errors::OxyError;
use serve::start_server_and_web_app;
use std::backtrace;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::process::exit;
use uuid::Uuid;

use init::init;

use dotenv;
use tracing::{debug, error};

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
    /// Execute procedure (.procedure.yml), workflow (.workflow.yml or .automation.yml), agent (.agent.yml), or SQL (.sql) files
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
    /// configured integrations like Omni and Looker metadata.
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
    Validate(ValidateArgs),
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
    Start(StartArgs),
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
    /// Start A2A (Agent-to-Agent) protocol server
    ///
    /// Launch an A2A server that exposes configured Oxy agents for
    /// external agent communication using JSON-RPC or HTTP+JSON protocols.
    A2a(A2aArgs),
    /// Manage Looker integration metadata
    ///
    /// Synchronize, list, and test Looker integrations configured in your project.
    /// Use subcommands to sync metadata, list explores, or test connections.
    Looker(looker::LookerArgs),
    /// Intent classification and clustering
    ///
    /// Discover and classify user intents from agent questions using
    /// unsupervised clustering techniques (HDBSCAN) and LLM labeling.
    Intent(intent::IntentArgs),
    /// Export ECharts configuration to PNG image
    ///
    /// Render ECharts charts to PNG images using server-side rendering.
    /// Requires Node.js to be installed on the system.
    ExportChart(export_chart::ExportChartArgs),
}

#[derive(Parser, Debug)]
pub struct MakeArgs {
    /// Path to the workflow file to execute
    file: String,
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
    /// Path to test/workflow/agent file. If omitted, discovers all *.test.yml files.
    file: Option<String>,
    /// Filter test cases by tag
    #[clap(long)]
    tag: Option<String>,
    /// Suppress detailed output and show only results summary
    #[clap(long, short = 'q', default_value_t = false)]
    quiet: bool,
    /// Show full detail including agent steps, actual output, and judge reasoning
    #[clap(long, short = 'v', default_value_t = false)]
    verbose: bool,
    /// Output format (pretty or json)
    #[clap(long, value_enum, default_value = "pretty")]
    format: OutputFormat,
    /// Minimum accuracy threshold (0.0-1.0). Exit with code 1 if accuracy is below this value
    #[clap(long, value_name = "THRESHOLD")]
    min_accuracy: Option<f32>,
    /// Threshold mode: 'average' checks average of all tests, 'all' checks each test individually
    #[clap(long, value_enum, default_value = "average")]
    threshold_mode: ThresholdMode,
    /// Write full JSON results to a file (derived from test file name, e.g. sales.agent.test.results.json)
    #[clap(long)]
    output_json: bool,
    /// Run only a specific test case by 0-based index, name, or prompt string (name/prompt lookup
    /// requires a .test.yml file). Requires a file to be specified.
    /// If --tag is also set, both filters apply: the case must match both the index/name/prompt and the tag.
    #[clap(long, value_name = "CASE")]
    case: Option<String>,
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

pub use crate::cli::{A2aArgs, ServeArgs, StartArgs};

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
struct ValidateArgs {
    /// Validate a specific file instead of all configuration files
    ///
    /// Provide a path to a workflow (.workflow.yml), agent (.agent.yml),
    /// or app (.app.yml) file to validate just that file.
    #[clap(long, short)]
    file: Option<std::path::PathBuf>,
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

/// Validates a single file based on its extension.
fn validate_single_file(file_path: &PathBuf, config: &Config) -> Result<(), String> {
    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    match () {
        _ if file_name.ends_with(".procedure.yml")
            || file_name.ends_with(".workflow.yml")
            || file_name.ends_with(".automation.yml") =>
        {
            let workflow = config.load_workflow(file_path).map_err(|e| e.to_string())?;
            config
                .validate_workflow(&workflow)
                .map_err(|e| e.to_string())
        }
        _ if file_name.ends_with(".agent.yml") => {
            let (agent, path) = config
                .load_agent_config(Some(file_path))
                .map_err(|e| e.to_string())?;
            config
                .validate_agent(&agent, path)
                .map_err(|e| e.to_string())
        }
        _ if file_name.ends_with(".app.yml") => {
            let app = config.load_app(file_path).map_err(|e| e.to_string())?;
            config.validate_app(&app).map_err(|e| e.to_string())
        }
        _ => Err(format!(
            "Unknown file type: {}. Expected .workflow.yml, .automation.yml, .agent.yml, or .app.yml",
            file_path.display()
        )),
    }
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
            SubCommand::Validate(_) => "validate",
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
            SubCommand::A2a(_) => "a2a",
            SubCommand::Looker(_) => "looker",
            SubCommand::Intent(_) => "intent",
            SubCommand::ExportChart(_) => "export-chart",
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
                (
                    "agent-test.json",
                    serde_json::to_string_pretty(&schemars::schema_for!(TestFileConfig))?,
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
            sentry_config::add_operation_context("test", test_args.file.as_deref());
            handle_test_command(test_args).await?;
        }
        Some(SubCommand::Build(build_args)) => {
            sentry_config::add_operation_context("build", None);

            // Synchronize Omni integration if configured
            handle_omni_sync().await?;

            // Synchronize Looker metadata if configured
            handle_looker_auto_sync().await?;

            // Setup
            let project_path = resolve_local_project_path()?.to_string_lossy().to_string();
            let config_manager = ConfigBuilder::new()
                .with_project_path(project_path)?
                .build()
                .await?;
            let secrets_manager = SecretsManager::from_environment()?;

            // Build vector embeddings
            reindex(ReindexInput {
                config: config_manager.clone(),
                secrets_manager,
                drop_all_tables: build_args.drop_all_tables,
            })
            .await?;

            println!("✅ Build complete");
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
        Some(SubCommand::Validate(args)) => {
            let config = ConfigBuilder::new()
                .with_project_path(&resolve_local_project_path()?)?
                .build()
                .await?;

            if let Some(file_path) = args.file {
                let validation_result = validate_single_file(&file_path, config.get_config());
                match validation_result {
                    Ok(_) => println!("{}", format!("{} is valid", file_path.display()).success()),
                    Err(e) => {
                        println!("{}", e.error());
                        exit(1)
                    }
                }
            } else {
                // Validate all files, collecting all errors
                let cfg = config.get_config();
                let mut errors: Vec<String> = Vec::new();
                let mut valid_count = 0;

                // Validate workflows
                for workflow_file in cfg.list_workflows(&cfg.project_path) {
                    match validate_single_file(&workflow_file, cfg) {
                        Ok(_) => valid_count += 1,
                        Err(e) => errors.push(format!("{}: {}", workflow_file.display(), e)),
                    }
                }

                // Validate agents
                for agent_file in cfg.list_agents(&cfg.project_path) {
                    match validate_single_file(&agent_file, cfg) {
                        Ok(_) => valid_count += 1,
                        Err(e) => errors.push(format!("{}: {}", agent_file.display(), e)),
                    }
                }

                // Validate apps
                for app_file in cfg.list_apps(&cfg.project_path) {
                    match validate_single_file(&app_file, cfg) {
                        Ok(_) => valid_count += 1,
                        Err(e) => errors.push(format!("{}: {}", app_file.display(), e)),
                    }
                }

                if errors.is_empty() {
                    println!(
                        "{}",
                        format!("All {} config files are valid", valid_count).success()
                    );
                } else {
                    for err in &errors {
                        println!("{}", err.error());
                    }
                    println!(
                        "{}",
                        format!(
                            "\n{} file(s) failed validation, {} file(s) valid",
                            errors.len(),
                            valid_count
                        )
                        .error()
                    );
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
        Some(SubCommand::Start(start_args)) => {
            if let Err(e) = start::start_database_and_server(start_args).await {
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
                None, // No data_app_file_path from CLI
            )
            .await?;
        }

        Some(SubCommand::Seed(seed_args)) => {
            handle_seed_command(seed_args).await?;
        }

        Some(SubCommand::Clean(clean_args)) => {
            handle_clean_command(clean_args).await?;
        }

        Some(SubCommand::Looker(looker_args)) => {
            looker::handle_looker_command(looker_args).await?;
        }
        Some(SubCommand::Intent(intent_args)) => {
            intent::handle_intent_command(intent_args).await?;
        }

        Some(SubCommand::ExportChart(export_chart_args)) => {
            export_chart::handle_export_chart_command(export_chart_args).await?;
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
            _ => None,
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

async fn handle_looker_auto_sync() -> Result<(), OxyError> {
    let project_path = resolve_local_project_path()?;

    let project = ProjectBuilder::new(Uuid::nil())
        .with_project_path(&project_path)
        .await?
        .with_runs_manager(RunsManager::default(Uuid::nil(), Uuid::nil()).await?)
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;

    let looker_integrations: Vec<_> = project
        .config_manager
        .get_config()
        .integrations
        .iter()
        .filter_map(|integration| match &integration.integration_type {
            ::oxy::config::model::IntegrationType::Looker(_) => Some(integration.name.clone()),
            _ => None,
        })
        .collect();

    if looker_integrations.is_empty() {
        return Ok(());
    }

    looker::handle_looker_sync(looker::LookerSyncArgs {
        integration: None,
        model: None,
        explore: None,
        force: false,
    })
    .await
}

pub async fn handle_test_command(test_args: TestArgs) -> Result<(), OxyError> {
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

    // Determine which files to test
    let file_paths: Vec<std::path::PathBuf> = match &test_args.file {
        Some(file) => {
            let current_dir = std::env::current_dir().expect("Could not get current directory");
            let file_path = current_dir.join(file);
            if !file_path.exists() {
                return Err(OxyError::ConfigurationError(format!(
                    "File not found: {file_path:?}"
                )));
            }
            vec![file_path]
        }
        None => {
            // Discover all *.test.yml files
            let test_files = project_manager.config_manager.list_tests().await?;
            if test_files.is_empty() {
                return Err(OxyError::ConfigurationError(
                    "No .test.yml files found in the project".to_string(),
                ));
            }
            test_files
        }
    };

    use crate::integrations::eval::{JsonReporter, MetricKind, PrettyReporter, Reporter};
    use crate::server::service::eval::{SharedTokenStats, TokenStats};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // Resolve --case to an index
    let case_index: Option<usize> = if let Some(ref case_str) = test_args.case {
        if test_args.file.is_none() {
            return Err(OxyError::ConfigurationError(
                "--case requires a specific file to be specified".to_string(),
            ));
        }
        // Use the explicit file argument to make the invariant clear.
        let file_path = file_paths[0].as_path();
        let path_str = file_path.to_string_lossy();

        if let Ok(idx) = case_str.parse::<usize>() {
            // For .test.yml files, validate the index is in bounds.
            if path_str.ends_with(".test.yml") {
                let test_config = project_manager
                    .config_manager
                    .resolve_test(file_path)
                    .await?;
                if idx >= test_config.cases.len() {
                    return Err(OxyError::ConfigurationError(format!(
                        "Case index {idx} is out of bounds: {:?} has {} case(s) (0-based)",
                        file_path,
                        test_config.cases.len()
                    )));
                }
            }
            Some(idx)
        } else {
            // Name/prompt lookup: only valid for .test.yml files
            if !path_str.ends_with(".test.yml") {
                return Err(OxyError::ConfigurationError(
                    "--case <name|prompt> is only supported for .test.yml files; use a 0-based integer index for agent/workflow files".to_string(),
                ));
            }
            let test_config = project_manager
                .config_manager
                .resolve_test(file_path)
                .await?;
            let mut matching = test_config
                .cases
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    c.name.as_deref() == Some(case_str.as_str()) || c.prompt == case_str.as_str()
                })
                .map(|(i, _)| i);
            let idx = matching.next().ok_or_else(|| {
                OxyError::ConfigurationError(format!(
                    "No test case with name or prompt {:?} found in {:?}",
                    case_str, file_path
                ))
            })?;
            if matching.next().is_some() {
                tracing::warn!(
                    "Multiple cases with name or prompt {:?} found in {:?}; using the first (index {})",
                    case_str,
                    file_path,
                    idx
                );
            }
            Some(idx)
        }
    } else {
        None
    };

    let token_stats: SharedTokenStats = Arc::new(Mutex::new(TokenStats::default()));
    let start_time = std::time::Instant::now();

    let mut all_results = Vec::new();
    for file_path in &file_paths {
        let file_name = file_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string_lossy().to_string());

        // For .test.yml files, load case labels and runs count so the progress bar can
        // show which case is currently being worked on.
        let (case_labels, runs_per_case) = if file_path.to_string_lossy().ends_with(".test.yml") {
            match project_manager.config_manager.resolve_test(file_path).await {
                Ok(test_config) => {
                    let labels = test_config
                        .cases
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| case_index.is_none_or(|i| *idx == i))
                        .filter(|(_, c)| test_args.tag.as_ref().is_none_or(|t| c.tags.contains(t)))
                        .map(|(_, c)| {
                            c.name.clone().unwrap_or_else(|| {
                                let p = c.prompt.trim();
                                let truncated: String = p.chars().take(60).collect();
                                if truncated.len() < p.chars().count() {
                                    format!("{truncated}…")
                                } else {
                                    truncated
                                }
                            })
                        })
                        .collect::<Vec<_>>();
                    (labels, test_config.settings.runs)
                }
                Err(_) => (vec![], 0),
            }
        } else {
            (vec![], 0)
        };

        let handler = EvalEventsHandler::new(test_args.quiet, Arc::clone(&token_stats))
            .with_test_label(file_name.clone())
            .with_case_info(case_labels, runs_per_case);
        let mut results = run_eval_with_tag(
            project_manager.clone(),
            file_path,
            case_index,
            test_args.tag.clone(),
            handler,
        )
        .await?;
        for result in &mut results {
            result.test_name = Some(file_name.clone());
        }
        all_results.extend(results);
    }

    let duration_ms = start_time.elapsed().as_millis() as f64;
    let tokens = token_stats.lock().await.clone();

    // Report results to stdout
    let reporter: Box<dyn Reporter> = match test_args.format {
        OutputFormat::Pretty => Box::new(PrettyReporter {
            quiet: test_args.quiet,
            verbose: test_args.verbose,
            total_input_tokens: tokens.total_input_tokens,
            total_output_tokens: tokens.total_output_tokens,
            duration_ms,
        }),
        OutputFormat::Json => Box::new(JsonReporter),
    };
    let mut stdout = std::io::stdout();
    reporter.report(&all_results, &mut stdout)?;

    // Write full JSON results to file for improvement loops
    if test_args.output_json {
        let output_path = match &test_args.file {
            Some(file) => {
                let stem = file.trim_end_matches(".yml").trim_end_matches(".yaml");
                format!("{stem}.results.json")
            }
            None => "test-results.json".to_string(),
        };
        let file = std::fs::File::create(&output_path).map_err(|e| {
            OxyError::RuntimeError(format!("Failed to create output file '{output_path}': {e}"))
        })?;
        let mut buf_writer = std::io::BufWriter::new(file);
        JsonReporter.report(&all_results, &mut buf_writer)?;
        eprintln!("Results written to {output_path}");
    }

    // Check threshold if provided
    if let Some(min_accuracy) = test_args.min_accuracy {
        // Collect all accuracy scores from all results
        let accuracies: Vec<f32> = all_results
            .iter()
            .flat_map(|r| &r.metrics)
            .filter_map(|m| match m {
                MetricKind::Similarity(s) => Some(s.score),
                MetricKind::Correctness(c) => Some(c.score),
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
