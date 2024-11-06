mod init;
mod search;
mod theme;

use clap::CommandFactory;
use clap::Parser;
use std::error::Error;
use theme::*;
use tower_http::services::ServeFile;

use init::init;
use search::search_files;

use crate::ai::setup_agent;
use crate::api::server;
use crate::connector::Connector;
use crate::workflow::run_workflow;
use crate::yaml_parsers::config_parser::get_config_path;
use crate::yaml_parsers::config_parser::parse_config;
use crate::{build, vector_search, BuildOpts};

use axum::Router;
use include_dir::{include_dir, Dir};
use std::net::SocketAddr;
use tower_http::services::ServeDir;

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
    /// Search through SQL in your project path. Execute and pass through agent postscript step on selection
    Search,
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
struct VecSearchArgs {
    question: String,
}

#[derive(Parser, Debug)]
struct ExecuteArgs {
    workflow_name: String,
}

pub async fn cli() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    match args.command {
        Some(SubCommand::Init) => match init() {
            Ok(_) => println!("Initialization complete"),
            Err(e) => eprintln!("Initialization failed: {}", e),
        },
        Some(SubCommand::ListTables) => {
            let config_path = get_config_path();
            let config = parse_config(&config_path)?;
            for warehouse in &config.warehouses {
                let tables = Connector::new(warehouse).get_schemas().await;
                for table in tables {
                    println!("{}", table);
                }
            }
        }
        Some(SubCommand::ListDatasets) => {
            let config_path = get_config_path();
            let config = parse_config(&config_path)?;
            for warehouse in &config.warehouses {
                let datasets = Connector::new(warehouse).list_datasets().await;
                print!("{:?}", datasets);
            }
        }
        Some(SubCommand::Search) => {
            let (agent, project_path) = setup_agent(args.agent.as_deref()).await?;
            match search_files(&project_path)? {
                Some(content) => {
                    agent.request(&content).await?;
                }
                None => println!("No files found or selected."),
            }
        }
        Some(SubCommand::Ask(ask_args)) => {
            let (agent, _) = setup_agent(args.agent.as_deref()).await?;
            agent.request(&ask_args.question).await?;
        }
        Some(SubCommand::Build) => {
            let config_path = get_config_path();
            let config = parse_config(&config_path)?;
            let project_path = &config.defaults.project_path;
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
            match run_workflow(&execute_args.workflow_name).await {
                Ok(_) => println!("\n\x1b[1;32mWorkflow executed successfully\x1b[0m"),
                Err(e) => eprintln!("\x1b[1;31mError executing workflow: \x1b[0m\n{}", e),
            };
        }
        Some(SubCommand::Validate) => {
            let config_path = get_config_path();
            let result = parse_config(&config_path);
            if result.is_err() {
                eprintln!("Error: {:?}", result.err().unwrap());
            } else {
                println!("Config file is valid");
            }
        }
        Some(SubCommand::Serve) => {
            let server_task = tokio::spawn(async move {
                let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
                println!("Axum server running at http://{}", addr);
                server::serve(&addr).await;
            });

            let web_task = tokio::spawn(async move {
                let serve_dir =
                    ServeDir::new("dist").not_found_service(ServeFile::new("dist/index.html"));
                let web_app = Router::new()
                    .nest_service("/", serve_dir.clone())
                    .fallback_service(serve_dir);

                let web_addr = SocketAddr::from(([127, 0, 0, 1], 3000));
                println!("Axum web server running at http://{}", web_addr);
                let listener = tokio::net::TcpListener::bind(web_addr).await.unwrap();
                axum::serve(listener, web_app).await.unwrap();
            });

            let _ = tokio::try_join!(server_task, web_task);
        }

        Some(SubCommand::TestTheme) => {
            println!("Initial theme mode: {:?}", get_current_theme_mode());
            println!("{}", "analysis".primary());
            println!("{}", "success".success());
            println!("{}", "warning".warning());
            println!("{}", "error".error());
            println!("{}", "https://github.com/onyx-hq/onyx-sample-repo/".secondary());
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
