mod agent;
mod client;
mod connector;
mod init;
mod prompt;
mod search;
mod tools;
mod yaml_parsers;

use crate::client::OpenAIAgent;
use crate::prompt::PromptBuilder;
use crate::tools::ToolBox;
use clap::CommandFactory;
use clap::Parser;
use client::LLMAgent;
use connector::Connector;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use crate::search::search_files;
use crate::yaml_parsers::config_parser::{get_config_path, parse_config};
use crate::yaml_parsers::entity_parser::parse_entity_config_from_scope;

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
}

#[derive(Parser, Debug)]
struct AskArgs {
    question: String,
}

async fn setup_agent(
    agent_name: Option<&str>,
) -> Result<(Box<dyn LLMAgent>, PathBuf), Box<dyn Error>> {
    let config_path = get_config_path();
    let config = parse_config(config_path)?;
    let parsed_config = config.load_config(agent_name.filter(|s| !s.is_empty()))?;
    let entity_config = parse_entity_config_from_scope(
        &parsed_config.agent_config.scope,
        &config.defaults.project_path,
    )?;
    let mut tools = ToolBox::default();
    let mut prompt_builder = PromptBuilder::new(&parsed_config.agent_config);
    prompt_builder.setup(&parsed_config.warehouse).await;
    tools.fill_toolbox(&parsed_config, &prompt_builder).await;
    // Create the agent from the parsed config and entity config
    let agent = OpenAIAgent::new(parsed_config, tools, prompt_builder);
    let project_path = PathBuf::from(&config.defaults.project_path);
    Ok((Box::new(agent), project_path))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args = Args::parse();

    match args.command {
        Some(SubCommand::Init) => match init::init() {
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
        None => {
            Args::command().print_help().unwrap();
        }
    }

    Ok(())
}
