use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Parser;
use clap::builder::ValueParser;
use minijinja::{Environment, Value};
use uuid::Uuid;

use ::oxy::adapters::runs::RunsManager;
use ::oxy::adapters::secrets::SecretsManager;
use ::oxy::adapters::workspace::builder::WorkspaceBuilder;
use ::oxy::checkpoint::types::RetryStrategy;
use ::oxy::config::{ConfigBuilder, ConfigManager, resolve_local_workspace_path};
use ::oxy::connector::Connector;
use ::oxy::execute::types::utils::record_batches_to_table;
use ::oxy::sentry_config;
use ::oxy::utils::print_colored_sql;
use oxy_shared::errors::OxyError;
use oxy_workflow::loggers::cli::WorkflowCLILogger;

use crate::server::service::agent::{
    AgentCLIHandler, ExecutionSource, run_agent, run_agentic_workflow,
};
use crate::server::service::workflow::run_workflow;

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
pub struct RunArgs {
    /// Path to the file to execute (.sql, .procedure.yml, .workflow.yml, .automation.yml, .agent.yml, or .aw.yml)
    pub(super) file: String,

    /// Database connection to use for SQL execution
    ///
    /// Specify which database from your config.yml to use.
    /// If not provided, uses the default database from configuration.
    #[clap(long)]
    pub(super) database: Option<String>,

    /// Template variables in the format VAR=VALUE
    ///
    /// Pass variables to SQL templates using Jinja2 syntax.
    /// Example: --variables user_id=123 --variables status=active
    #[clap(long, short = 'v', value_parser=ValueParser::new(parse_variable), num_args = 1..)]
    pub(super) variables: Vec<(String, String)>,

    /// Question to ask when running agent files
    ///
    /// Required when executing .agent.yml files to provide context
    /// for the AI agent's analysis or response.
    pub(super) question: Option<String>,

    /// Retry failed operations automatically
    ///
    /// Enable automatic retry logic for transient failures
    /// during workflow or query execution.
    #[clap(long, default_value_t = false, group = "named")]
    pub(super) retry: bool,

    /// Retry from a specific step in the workflow
    #[clap(long, group = "unnamed", conflicts_with = "named")]
    pub(super) retry_from: Option<String>,

    /// Preview SQL without executing against the database
    ///
    /// Validate and display the generated SQL query without
    /// actually running it against your database.
    #[clap(long, default_value_t = false)]
    pub(super) dry_run: bool,
}

#[derive(Clone)]
pub struct RunOptions {
    pub database: Option<String>,
    pub variables: Option<Vec<(String, String)>>,
    pub question: Option<String>,
    pub retry: bool,
    pub dry_run: bool,
}

impl RunArgs {
    pub fn new(file: String, options: Option<RunOptions>) -> Self {
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

pub enum RunResult {
    Workflow,
    Agent,
    Sql(String),
}

pub async fn handle_run_command(run_args: RunArgs) -> Result<RunResult, OxyError> {
    let file = &run_args.file;

    let current_dir = std::env::current_dir()
        .map_err(|e| OxyError::RuntimeError(format!("Could not get current directory: {e}")))?;

    let file_path = current_dir.join(file);
    if !file_path.exists() {
        return Err(OxyError::ConfigurationError(format!(
            "File not found: {file_path:?}"
        )));
    }

    let extension = file_path.extension().and_then(std::ffi::OsStr::to_str);

    // Extract the compound extension (the part before the final `.yml`/`.yaml`/`.sql`).
    // For example, `my.workflow.yml` → outer_ext = "yml", stem_ext = "workflow".
    let stem_ext = file_path
        .file_stem()
        .and_then(|stem| std::path::Path::new(stem).extension())
        .and_then(std::ffi::OsStr::to_str);

    match (extension, stem_ext) {
        (Some("yml") | Some("yaml"), Some("procedure" | "workflow" | "automation")) => {
            handle_workflow_file(&file_path, run_args.retry, run_args.retry_from).await?;
            Ok(RunResult::Workflow)
        }
        (Some("yml") | Some("yaml"), Some("agent")) => {
            handle_agent_file(&file_path, run_args.question).await?;
            Ok(RunResult::Agent)
        }
        (Some("yml") | Some("yaml"), Some("aw")) => {
            handle_agentic_workflow_file(&file_path, run_args.question).await?;
            Ok(RunResult::Agent)
        }
        (Some("yml") | Some("yaml"), _) => Err(OxyError::ArgumentError(
            "Invalid YAML file. Must be either *.procedure.yml, *.workflow.yml, *.automation.yml, *.agent.yml, or *.aw.yml".into(),
        )),
        (Some("sql"), _) => {
            let config = ConfigBuilder::new()
                .with_workspace_path(&resolve_local_workspace_path()?)?
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
                database.unwrap(),
                &config,
                &run_args.variables,
                run_args.dry_run,
            )
            .await?;
            Ok(RunResult::Sql(sql_result))
        }
        _ => Err(OxyError::ArgumentError(
            "Invalid file extension. Must be .procedure.yml, .workflow.yml, .automation.yml, .agent.yml, .aw.yml, or .sql"
                .into(),
        )),
    }
}

async fn handle_workflow_file(
    workflow_path: &PathBuf,
    retry: bool,
    retry_from: Option<String>,
) -> Result<(), OxyError> {
    let workspace_path = resolve_local_workspace_path()?;
    // `oxy run` intentionally uses noop storage for normal (non-retry) runs: the CLI
    // is a lightweight execution path that does not require a database. Run history
    // persistence for API-triggered runs is handled by the server. Using noop here
    // means `oxy run` works out-of-the-box without OXY_DATABASE_URL, and runs are
    // not written to the DB even when a DB is configured.
    //
    // For retry/retry-from, a real database is required to look up the previous run;
    // we switch to RunsManager::default() so users get a clear "connection required"
    // error if OXY_DATABASE_URL is not set.
    let runs_manager = if retry || retry_from.is_some() {
        RunsManager::default(Uuid::nil(), Uuid::nil()).await?
    } else {
        RunsManager::noop()
    };
    let project = WorkspaceBuilder::new(Uuid::nil())
        .with_workspace_path(&workspace_path)
        .await?
        .with_runs_manager(runs_manager)
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;
    // Add Sentry context for workflow execution
    let workflow_name_str = workflow_path
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
            workflow_path,
            WorkflowCLILogger,
            RetryStrategy::Retry {
                replay_id: Some(replay_id),
                run_index: run_id,
            },
            project,
            None,
            None,
            None, // No globals override from CLI
            Some(ExecutionSource::Cli),
            None, // No authenticated user in CLI context
        )
        .await?;
    } else if retry {
        tracing::debug!("Running workflow from last failed run");
        sentry_config::add_workflow_context(workflow_name_str, Some("retry"));
        run_workflow(
            workflow_path,
            WorkflowCLILogger,
            RetryStrategy::LastFailure,
            project,
            None,
            None,
            None, // No globals override from CLI
            Some(ExecutionSource::Cli),
            None, // No authenticated user in CLI context
        )
        .await?;
    } else {
        tracing::debug!("Running workflow without retry");
        sentry_config::add_workflow_context(workflow_name_str, Some("normal"));
        run_workflow(
            workflow_path,
            WorkflowCLILogger,
            RetryStrategy::NoRetry { variables: None },
            project,
            None,
            None,
            None, // No globals override from CLI
            Some(ExecutionSource::Cli),
            None, // No authenticated user in CLI context
        )
        .await?;
    }
    Ok(())
}

/// Shared setup for agent and agentic-workflow CLI handlers:
/// registers Sentry context, validates the question is present, and builds a noop project manager.
async fn setup_agent_run(
    file_path: &PathBuf,
    question: Option<String>,
    question_required_msg: &str,
) -> Result<
    (
        String,
        ::oxy::adapters::workspace::manager::WorkspaceManager,
    ),
    OxyError,
> {
    let agent_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    sentry_config::add_agent_context(agent_name, question.as_deref());

    let question =
        question.ok_or_else(|| OxyError::ArgumentError(question_required_msg.to_string()))?;
    let workspace_path = resolve_local_workspace_path()?;

    let workspace_manager = WorkspaceBuilder::new(Uuid::nil())
        .with_workspace_path(&workspace_path)
        .await?
        .with_runs_manager(RunsManager::noop())
        .build()
        .await
        .map_err(|e| OxyError::from(anyhow::anyhow!("Failed to create project: {e}")))?;

    Ok((question, workspace_manager))
}

async fn handle_agent_file(file_path: &PathBuf, question: Option<String>) -> Result<(), OxyError> {
    let (question, workspace_manager) =
        setup_agent_run(file_path, question, "Question is required for agent files").await?;

    let _ = run_agent(
        workspace_manager,
        file_path,
        question,
        AgentCLIHandler::default(),
        vec![],
        None,
        None,
        None, // No globals from CLI
        None, // No variables from CLI (yet)
        Some(ExecutionSource::Cli),
        None, // No sandbox info from CLI
        None, // No data_app_file_path from CLI
    )
    .await?;
    Ok(())
}

async fn handle_agentic_workflow_file(
    file_path: &PathBuf,
    question: Option<String>,
) -> Result<(), OxyError> {
    let (question, workspace_manager) = setup_agent_run(
        file_path,
        question,
        "Question is required for agentic workflow files",
    )
    .await?;

    run_agentic_workflow(
        workspace_manager,
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
    database: String,
    config: &ConfigManager,
    variables: &[(String, String)],
    dry_run: bool,
) -> Result<String, OxyError> {
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
