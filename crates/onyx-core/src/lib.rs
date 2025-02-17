pub mod ai;
pub mod api;
pub mod cli;
pub mod connector;
pub mod db;
pub mod errors;
pub mod execute;
pub mod service;
pub mod theme;
pub mod utils;
pub mod workflow;

use ai::retrieval::{build_embeddings, get_vector_store};
use theme::*;
pub mod config;

use config::model::{Config, RetrievalTool};

pub async fn build(config: &Config) -> anyhow::Result<()> {
    println!("{}", "Building...".text());
    build_embeddings(config).await?;
    Ok(())
}

pub async fn vector_search(
    agent: &str,
    retrieval: &RetrievalTool,
    query: &str,
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
    let db = get_vector_store(agent, retrieval)?;
    let documents = db.search(query).await?;
    for document in documents {
        println!("{}", format!("{}\n", document.content).text());
        println!("____________________________________________________");
    }
    Ok(())
}
