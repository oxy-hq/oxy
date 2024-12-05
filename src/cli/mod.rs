mod init;

use crate::config::*;
use crate::utils::print_colored_sql;
use arrow::util::pretty::pretty_format_batches;
use axum::handler::Handler;
use clap::builder::ValueParser;
use clap::CommandFactory;
use clap::Parser;
use colored::Colorize;
use minijinja::{Environment, Value};
use model::AgentConfig;
use model::FileFormat;
use model::{Config, Workflow};
use std::collections::BTreeMap;
use std::error::Error;
use std::process::exit;
use std::process::Command;

use init::init;

use crate::ai::setup_agent;
use crate::api::server;
use crate::connector::Connector;
use crate::theme::*;
use crate::workflow::run_workflow;
use crate::{build, vector_search, BuildOpts};
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
fn parse_variable(env: &str) -> Result<Variable, std::io::Error> {
    if let Some((var, value)) = env.split_once('=') {
        Ok((var.to_owned(), value.to_owned()))
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid variable format",
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
    /// Ask a question to the specified agent. If no agent is specified, the default agent is used
    Ask(AskArgs),
    Build,
    VecSearch(VecSearchArgs),
    Execute(ExecuteArgs),
    Validate,
    Serve,
    TestTheme,
    GenConfigSchema(GenConfigSchemaArgs),
}

#[derive(Parser, Debug)]
struct GenConfigSchemaArgs {
    #[clap(long)]
    check: bool,
}

#[derive(Parser, Debug)]
struct AskArgs {
    question: String,
}

#[derive(Parser, Debug)]
struct RunArgs {
    warehouse: Option<String>,
    file: Option<String>,

    #[clap(long, short = 'v', value_parser=ValueParser::new(parse_variable), num_args = 1..)]
    variables: Vec<(String, String)>,
}

#[derive(Parser, Debug)]
struct VecSearchArgs {
    question: String,
}

#[derive(Parser, Debug)]
struct ExecuteArgs {
    workflow_name: Option<String>,
}

pub async fn cli() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

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
                let output = Command::new("git").args(&["status", "--short"]).output()?;

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
            let config_path = get_config_path();
            let config = parse_config(&config_path)?;
            for warehouse in &config.warehouses {
                let tables = Connector::new(warehouse).get_schemas().await;
                for table in tables {
                    println!("{}", table.text());
                }
            }
        }
        Some(SubCommand::ListDatasets) => {
            let config_path = get_config_path();
            let config = parse_config(&config_path)?;
            for warehouse in &config.warehouses {
                let datasets = Connector::new(warehouse).list_datasets().await;
                for dataset in datasets {
                    println!("{}", dataset.text());
                }
            }
        }
        Some(SubCommand::Run(run_args)) => {
            let config_path = get_config_path();
            let config = parse_config(&config_path)?;
            let project_path = &config.project_path;

            let file_path = if let Some(file) = run_args.file {
                // Use specific SQL file from data directory
                project_path.join("data").join(file)
            } else {
                eprintln!("Error: Missing required filename argument");
                return Ok(());
            };

            match run_args.warehouse {
                Some(warehouse) => match std::fs::read_to_string(&file_path) {
                    Ok(content) => {
                        let wh_config = config.find_warehouse(&warehouse);
                        let mut env = Environment::new();
                        let mut query = content.clone();

                        // render template if needed
                        if !run_args.variables.is_empty() {
                            env.add_template("query", &query)?;
                            let tmpl = env.get_template("query").unwrap();
                            let ctx = Value::from({
                                let mut m = BTreeMap::new();
                                for var in &run_args.variables {
                                    m.insert(var.0.clone(), var.1.clone());
                                }
                                m
                            });
                            query = tmpl.render(ctx)?
                        }
                        match wh_config {
                            Ok(wh) => {
                                print_colored_sql(&query);
                                let results =
                                    Connector::new(&wh).run_query_and_load(&query).await?;
                                let batches_display = pretty_format_batches(&results)?;
                                println!("\n\x1b[1;32mResults:\x1b[0m");
                                println!("{}", batches_display);
                                return Ok(());
                            }
                            Err(_) => {
                                eprintln!("Error: Warehouse not found in config");
                                return Ok(());
                            }
                        }
                    }
                    Err(e) => eprintln!("{}", format!("Error reading file: {}", e).error()),
                },
                None => {
                    eprintln!("Error: Missing required warehouse argument");
                    return Ok(());
                }
            }
        }
        Some(SubCommand::Ask(ask_args)) => {
            let (agent, _) = setup_agent(args.agent.as_deref(), &FileFormat::Markdown).await?;
            agent.request(&ask_args.question).await?;
        }
        Some(SubCommand::Build) => {
            let config_path = get_config_path();
            let config = parse_config(&config_path)?;
            let project_path = &config.project_path;
            let data_path = project_path.join("data");
            build(
                &config,
                BuildOpts {
                    force: true,
                    data_path: data_path.to_str().unwrap().to_string(),
                },
            )
            .await?;
        }
        Some(SubCommand::VecSearch(search_args)) => {
            let config_path = get_config_path();
            let config = parse_config(&config_path)?;
            let agent_config = config.load_config(None)?;
            let retrieval = config.find_retrieval(&agent_config.retrieval.unwrap())?;
            vector_search(&config.defaults.agent, &retrieval, &search_args.question).await?;
        }
        Some(SubCommand::Execute(execute_args)) => {
            if let Some(workflow_name) = execute_args.workflow_name {
                match run_workflow(&workflow_name).await {
                    Ok(_) => println!("{}", "\n✅Workflow executed successfully".success()),
                    Err(e) => eprintln!("{}", format!("Error executing workflow: \n{}", e).error()),
                }
            } else {
                eprintln!("Error: Missing required filename argument");
                return Ok(());
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
                        eprintln!("{}", e.to_string().error().red());
                    }
                },
                Err(e) => eprintln!("{}", e.to_string().error().red()),
            }
        }
        Some(SubCommand::Serve) => {
            let server_task = tokio::spawn(async move {
                let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
                println!(
                    "{} {}",
                    "Axum server running at".text(),
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
                    "Web app server running at".text(),
                    format!("http://{}", web_addr).secondary()
                );
                axum::serve(listener, web_app).await.unwrap();
            });

            let _ = tokio::try_join!(server_task, web_task);
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
