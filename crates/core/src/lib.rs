mod adapters;
mod agent;
pub mod ai;
pub mod api;
pub mod cli;
pub mod db;
pub mod errors;
mod eval;
pub mod execute;
pub mod mcp;
pub mod service;
pub mod theme;
mod tools;
pub mod utils;
pub mod workflow;

use ai::retrieval::{build_embeddings, get_vector_store};
use theme::*;
pub mod config;

use config::{ConfigManager, model::RetrievalConfig};

pub async fn build(config: &ConfigManager) -> anyhow::Result<()> {
    println!("{}", "Building...".text());
    build_embeddings(config).await?;
    Ok(())
}

pub async fn vector_search(
    agent: &str,
    retrieval: &RetrievalConfig,
    query: &str,
    config: &ConfigManager,
) -> anyhow::Result<()> {
    println!(
        "{}",
        format!(
            "Searching using agent {} tool {} ...",
            agent, retrieval.name
        )
        .as_str()
        .text()
    );
    let db_path = config
        .resolve_file(format!(".db-{}-{}", agent, retrieval.name))
        .await?;
    let db = get_vector_store(retrieval, &db_path)?;
    let documents = db.search(query).await?;
    for document in documents {
        println!("{}", format!("{}\n", document.content).text());
        println!("____________________________________________________");
    }
    Ok(())
}
