use std::{fs, path::PathBuf};

use embedding::{Document, LanceDBStore, VectorStore};
use fastembed::{EmbeddingModel, RerankerModel};

use crate::{
    utils::collect_files_recursively,
    yaml_parsers::{self, config_parser::Config},
};

pub mod embedding;

fn get_documents_from_files(data_path: &str) -> anyhow::Result<Vec<Document>> {
    let files = collect_files_recursively(data_path, data_path)?;
    println!("Found: {:?}", files);
    let documents = files
        .iter()
        .map(|file| (file, std::fs::read_to_string(file)))
        .filter(|(_file, content)| !content.as_ref().unwrap().is_empty())
        .map(|(file, content)| Document {
            content: content.unwrap(),
            source_type: "file".to_string(),
            source_identifier: file.to_string(),
            embeddings: vec![],
        })
        .collect();
    Ok(documents)
}

pub async fn build_embeddings(config: &Config, data_path: &str) -> anyhow::Result<()> {
    let agent_dirs = fs::read_dir(data_path)?
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<PathBuf>>();
    for agent_dir in agent_dirs {
        println!("Building embeddings for agent: {:?}", agent_dir);
        let agent_name = agent_dir.file_name().unwrap().to_str().unwrap();
        let agent = config.load_config(Some(agent_name))?;
        let retrieval = config.find_retrieval(agent.retrieval.as_ref().unwrap())?;
        let db = get_vector_store(agent_name, &retrieval)?;
        let documents = get_documents_from_files(data_path)?;
        db.embed(&documents).await?;
    }
    Ok(())
}

pub async fn search(
    query: &str,
    db: &Box<dyn VectorStore + Sync + Send>,
) -> anyhow::Result<Vec<Document>> {
    let documents = db.search(query).await?;
    Ok(documents)
}

pub fn get_vector_store(
    agent: &str,
    retrieval: &yaml_parsers::config_parser::Retrieval,
) -> anyhow::Result<Box<dyn VectorStore + Send + Sync>> {
    let embed_model = embedding_model_from_str(&retrieval.embed_model)?;
    let rerank_model = rerank_model_from_str(&retrieval.rerank_model)?;

    let db = LanceDBStore::new(
        format!(".db-{}", agent).as_str(),
        embed_model,
        rerank_model,
        retrieval.top_k,
        retrieval.factor,
    );
    Ok(Box::new(db))
}

fn embedding_model_from_str(s: &str) -> anyhow::Result<EmbeddingModel> {
    match s {
        "bge-small-en-v1.5" => Ok(EmbeddingModel::BGESmallENV15),
        _ => Err(anyhow::Error::msg(format!("Unknown model: {}", s))),
    }
}

fn rerank_model_from_str(s: &str) -> anyhow::Result<RerankerModel> {
    match s {
        "jina-reranker-v1-turbo-en" => Ok(RerankerModel::JINARerankerV1TurboEn),
        "jina-reranker-v2-base-multiligual" => Ok(RerankerModel::JINARerankerV2BaseMultiligual),
        _ => Err(anyhow::Error::msg(format!("Unknown model: {}", s))),
    }
}
