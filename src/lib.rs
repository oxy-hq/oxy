pub mod ai;
mod api;
pub mod cli;
pub mod connector;
pub mod db;
pub mod theme;
pub mod utils;
pub mod workflow;
pub mod yaml_parsers;

use ai::retrieval::{build_embeddings, get_vector_store, search};
use theme::*;
use yaml_parsers::config_parser::Retrieval;

pub struct BuildOpts {
    pub force: bool,
    pub data_path: String,
}

pub async fn build(
    config: &yaml_parsers::config_parser::Config,
    opts: BuildOpts,
) -> anyhow::Result<()> {
    println!("{}", "Building...".text());
    build_embeddings(config, &opts.data_path).await?;
    Ok(())
}

pub async fn vector_search(agent: &str, retrieval: &Retrieval, query: &str) -> anyhow::Result<()> {
    println!("{}", "Searching...".text());
    let db = get_vector_store(agent, retrieval)?;
    search(query, &db).await?;
    Ok(())
}
