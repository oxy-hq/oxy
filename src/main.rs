use clap::CommandFactory;
use clap::Parser;
use onyx::vector_search;
use std::error::Error;
use std::path::PathBuf;

use onyx::client::LLMAgent;
use onyx::client::OpenAIAgent;
use onyx::connector::Connector;
use onyx::prompt::PromptBuilder;
use onyx::search::search_files;
use onyx::tools::ToolBox;
use onyx::yaml_parsers::config_parser::{get_config_path, parse_config};
use onyx::{build, init::init, BuildOpts};

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
}

#[derive(Parser, Debug)]
struct AskArgs {
    question: String,
}

#[derive(Parser, Debug)]
struct VecSearchArgs {
    question: String,
}

async fn setup_agent(
    agent_name: Option<&str>,
) -> Result<(Box<dyn LLMAgent>, PathBuf), Box<dyn Error>> {
    let config_path = get_config_path();
    let config = parse_config(config_path)?;
    let parsed_config = config.load_config(agent_name.filter(|s| !s.is_empty()))?;
    let project_path = PathBuf::from(&config.defaults.project_path);
    let mut tools = ToolBox::default();
    let mut prompt_builder = PromptBuilder::new(&parsed_config.agent_config, &project_path);
    prompt_builder.setup(&parsed_config.warehouse).await;
    tools.fill_toolbox(&parsed_config, &prompt_builder).await;
    // Create the agent from the parsed config and entity config
    let agent = OpenAIAgent::new(parsed_config, tools, prompt_builder);
    Ok((Box::new(agent), project_path))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
        None => {
            Args::command().print_help().unwrap();
        }
    }

    Ok(())
}
