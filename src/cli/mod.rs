mod init;
mod search;

use clap::CommandFactory;
use clap::Parser;
use std::error::Error;

use init::init;
use search::search_files;

use crate::ai::setup_agent;
use crate::connector::Connector;
use crate::yaml_parsers::config_parser::get_config_path;
use crate::yaml_parsers::config_parser::parse_config;
use crate::{build, vector_search, BuildOpts};

use include_dir::{include_dir, Dir};
use std::{convert::Infallible, net::SocketAddr};
use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get_service,
    Router,
};
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
    Serve
}

#[derive(Parser, Debug)]
struct AskArgs {
    question: String,
}

#[derive(Parser, Debug)]
struct VecSearchArgs {
    question: String,
}

pub async fn cli() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args = Args::parse();

    match args.command {
        Some(SubCommand::Init) => match init() {
            Ok(_) => println!("Initialization complete"),
            Err(e) => eprintln!("Initialization failed: {}", e),
        },
        Some(SubCommand::ListTables) => {
            let config_path = get_config_path();
            let config = parse_config(config_path)?;
            let parsed_config = config.load_config(None)?;
            let ddls = Connector::new(parsed_config.warehouse).get_schemas().await;
            print!("{:?}", ddls);
        }
        Some(SubCommand::ListDatasets) => {
            let config_path = get_config_path();
            let config = parse_config(config_path)?;
            let parsed_config = config.load_config(None)?;
            let datasets = Connector::new(parsed_config.warehouse)
                .list_datasets()
                .await;
            print!("{:?}", datasets);
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
            let config = parse_config(config_path)?;
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
            let config = parse_config(config_path)?;
            let parsed_config = config.load_config(None)?;
            vector_search(
                &config.defaults.agent,
                &parsed_config.retrieval,
                &search_args.question,
            )
            .await?;
        }
        Some(SubCommand::Serve) => {
            let app = Router::new().fallback_service(get_service(serve_embedded()));

            let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
            println!("Axum server running at http://{}", addr);
        
            axum::Server::bind(&addr)
                .serve(app.into_make_service())
                .await
                .unwrap();
        }
        None => {
            Args::command().print_help().unwrap();
        }
    }

    Ok(())
}


fn serve_embedded() -> ServeDir {
    ServeDir::new("dist")
}

async fn handle_error(err: std::io::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("Unhandled error: {}", err))
}