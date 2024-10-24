pub mod ai;
pub mod cli;
pub mod connector;
pub mod yaml_parsers;

use ai::retrieval::{build_embeddings, get_vector_store, search};
use yaml_parsers::config_parser::Retrieval;

pub struct BuildOpts {
    pub force: bool,
    pub data_path: String,
}

pub async fn build(
    config: &yaml_parsers::config_parser::Config,
    opts: BuildOpts,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Building...");
    build_embeddings(config, &opts.data_path).await?;
    Ok(())
}

pub async fn vector_search(
    agent: &str,
    retrieval: &Retrieval,
    query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Searching...");
    let db = get_vector_store(agent, retrieval)?;
    search(query, &db).await?;
    Ok(())
}
