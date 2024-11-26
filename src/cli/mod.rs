mod init;
mod search;

use crate::config::*;
use axum::handler::Handler;
use clap::CommandFactory;
use clap::Parser;
use colored::Colorize;
use log::debug;
use std::error::Error;

use init::init;
use search::search_files;

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

static DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/dist");

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
}

#[derive(Parser, Debug)]
struct AskArgs {
    question: String,
}

#[derive(Parser, Debug)]
struct RunArgs {
    file: Option<String>,
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
            let (agent, config_path) = setup_agent(args.agent.as_deref()).await?;
            let config = parse_config(&config_path)?;
            let project_path = &config.project_path;

            let file_path = if let Some(file) = run_args.file {
                // Use specific SQL file from data directory
                project_path.join("data").join(file)
            } else {
                // Interactive file search mode
                let subdirectory_name = "data";
                match search_files(project_path, subdirectory_name)? {
                    Some(file_name) => project_path.join("data").join(file_name),
                    None => {
                        eprintln!("{}", "No files found or selected.".error());
                        return Ok(());
                    }
                }
            };

            match std::fs::read_to_string(&file_path) {
                Ok(content) => {
                    agent.request(&content).await?;
                }
                Err(e) => eprintln!("{}", format!("Error reading file: {}", e).error()),
            }
        }
        Some(SubCommand::Ask(ask_args)) => {
            let (agent, _) = setup_agent(args.agent.as_deref()).await?;
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
                let config_path = get_config_path();
                let config = parse_config(&config_path)?;
                let project_path = &config.project_path;
                let subdirectory_name = "workflows";
                match search_files(project_path, subdirectory_name)? {
                    Some(workflow_file) => {
                        let workflow_name =
                            workflow_file.strip_suffix(".yml").unwrap_or(&workflow_file);
                        debug!("Executing workflow: {}", workflow_name);
                        match run_workflow(workflow_name).await {
                            Ok(_) => println!("{}", "\n✅Workflow executed successfully".success()),
                            Err(e) => {
                                eprintln!(
                                    "{}",
                                    format!("Error executing workflow: \n{}", e).error()
                                )
                            }
                        }
                    }
                    None => eprintln!("{}", "No workflow files found or selected.".error()),
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
