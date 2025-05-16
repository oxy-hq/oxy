use serde::Deserialize;

use crate::adapters::vector_store::Document;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ContextHeader {
    pub(super) oxy: OxyHeaderData,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum Embed {
    String(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct OxyHeaderData {
    pub(super) embed: Embed,
    pub(super) database: Option<String>,
}

// example format
// /*
// oxy:
//     embed: |
//         this return fruit with sales
//         fruit including apple, banana, kiwi, cherry and orange
// */
// select 'apple' as name, 325 as sales
// union all
// select 'banana' as name, 2000 as sales
// union all
// select 'cherry' as name, 18 as sales
// union all
// select 'kiwi' as name, 120 as sales
// union all
// select 'orange' as name, 1500 as sales
pub(super) fn parse_embed_document(id: &str, content: &str) -> Vec<Document> {
    let mut documents = vec![];
    let context_regex = regex::Regex::new(r"(?m)^\/\*((?:.|\n)+)\*\/((.|\n)+)$").unwrap();
    let context_match = match context_regex.captures(content) {
        Some(m) => m,
        None => {
            tracing::warn!("No context found in the file: {:?}", id);
            return vec![Document {
                content: content.to_string(),
                source_type: "file".to_string(),
                source_identifier: id.to_string(),
                embedding_content: content.to_string(),
                embeddings: vec![],
            }];
        }
    };
    let comment_content = context_match[1].replace("\n*", "\n");
    let context_content = context_match[2].to_string();
    let header_data: Result<ContextHeader, serde_yaml::Error> =
        serde_yaml::from_str(comment_content.as_str());

    match header_data {
        Ok(header_data) => match &header_data.oxy.embed {
            Embed::String(embed) => {
                let doc = Document {
                    content: format!("{}\n\n{}", embed, context_content),
                    source_type: generate_sql_source_type(&header_data.oxy.database),
                    source_identifier: id.to_string(),
                    embedding_content: embed.to_string(),
                    embeddings: vec![],
                };
                documents.push(doc);
            }
            Embed::Multiple(embeds) => {
                for embed in embeds {
                    let doc = Document {
                        content: format!("{}\n\n{}", embed, context_content),
                        source_type: generate_sql_source_type(&header_data.oxy.database),
                        source_identifier: id.to_string(),
                        embedding_content: embed.to_string(),
                        embeddings: vec![],
                    };
                    documents.push(doc);
                }
            }
        },
        Err(e) => {
            tracing::warn!(
                "Failed to parse header data: {:?}, error: {:?}.\nEmbedding the whole file content",
                comment_content,
                e
            );
            documents.push(Document {
                content: content.to_string(),
                source_type: "file".to_string(),
                source_identifier: id.to_string(),
                embedding_content: content.to_string(),
                embeddings: vec![],
            });
        }
    }
    documents
}

fn generate_sql_source_type(database: &Option<String>) -> String {
    match database {
        Some(db) => format!("sql::{}", db),
        None => "file".to_string(),
    }
}

pub fn parse_sql_source_type(source_type: &str) -> Option<String> {
    if source_type.starts_with("sql::") {
        Some(source_type[5..].to_string())
    } else {
        None
    }
}
