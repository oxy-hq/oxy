pub mod ai;
mod api;
pub mod cli;
pub mod connector;
pub mod db;
pub mod theme;
pub mod utils;
pub mod workflow;

use ai::retrieval::{build_embeddings, get_vector_store, search};
use theme::*;
pub mod config;

use config::model::{Config, Retrieval};

pub struct BuildOpts {
    pub force: bool,
    pub data_path: String,
}

pub async fn build(config: &Config, opts: BuildOpts) -> anyhow::Result<()> {
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
