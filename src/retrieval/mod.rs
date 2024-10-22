use std::{fs, path::PathBuf};

use embedding::{Document, LanceDBStore, VectorStore};
use fastembed::{EmbeddingModel, RerankerModel};

use crate::yaml_parsers::{self, config_parser::Config};

pub mod embedding;

fn collect_files_recursively(
    dir: &str,
    base_path: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let manifest: &mut Vec<String> = &mut Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let mut paths = collect_files_recursively(path.to_str().unwrap(), base_path)?;
            let paths = paths.as_mut();
            manifest.append(paths);
        } else if path.is_file() {
            if let Some(path_str) = path.to_str() {
                manifest.push(path_str.to_string());
            }
        }
    }
    Ok(manifest.clone())
}

fn get_documents_from_files(data_path: &str) -> Result<Vec<Document>, Box<dyn std::error::Error>> {
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

pub async fn build_embeddings(
    config: &Config,
    data_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let agent_dirs = fs::read_dir(data_path)?
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<PathBuf>>();
    for agent_dir in agent_dirs {
        println!("Building embeddings for agent: {:?}", agent_dir);
        let agent = agent_dir.file_name().unwrap().to_str().unwrap();
        let retrieval = config.load_config(Some(agent))?.retrieval;
        let db: Box<dyn VectorStore> = get_vector_store(agent, &retrieval)?;
        let documents = get_documents_from_files(data_path)?;
        db.embed(&documents).await?;
    }
    Ok(())
}

pub async fn search(
    query: &str,
    db: &Box<dyn VectorStore>,
) -> Result<Vec<Document>, Box<dyn std::error::Error>> {
    let documents = db.search(query).await?;
    Ok(documents)
}

pub fn get_vector_store(
    agent: &str,
    retrieval: &yaml_parsers::config_parser::Retrieval,
) -> Result<Box<dyn VectorStore>, Box<dyn std::error::Error>> {
    let embed_model = embedding_model_from_str(&retrieval.embed_model)?;
    let rerank_model = rerank_model_from_str(&retrieval.rerank_model)?;

    let db = Box::new(LanceDBStore::new(
        format!(".db-{}", agent).as_str(),
        embed_model,
        rerank_model,
        retrieval.top_k,
        retrieval.factor,
    ));
    Ok(db)
}

fn embedding_model_from_str(s: &str) -> Result<EmbeddingModel, Box<dyn std::error::Error>> {
    match s {
        "bge-small-en-v1.5" => Ok(EmbeddingModel::BGESmallENV15),
        _ => Err(format!("Unknown model: {}", s).into()),
    }
}

fn rerank_model_from_str(s: &str) -> Result<RerankerModel, Box<dyn std::error::Error>> {
    match s {
        "jina-reranker-v1-turbo-en" => Ok(RerankerModel::JINARerankerV1TurboEn),
        "jina-reranker-v2-base-multiligual" => Ok(RerankerModel::JINARerankerV2BaseMultiligual),
        _ => Err(format!("Unknown model: {}", s).into()),
    }
}
