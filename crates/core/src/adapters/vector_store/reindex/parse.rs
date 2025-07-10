use serde::Deserialize;

use crate::adapters::vector_store::{
    types::{Document, RetrievalContent},
    utils::build_inclusion_exclusion_summary,
};

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
pub(super) struct RetrievalConfig {
    #[serde(default)]
    pub(super) include: Vec<String>,
    #[serde(default)]
    pub(super) exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct OxyHeaderData {
    pub(super) embed: Option<Embed>,
    pub(super) retrieval: Option<RetrievalConfig>,
    pub(super) database: Option<String>,
}

// example formats:
//
// New format:
// /*
// oxy:
//   retrieval:
//     include:
//       - this returns fruit with sales
//     exclude:
//       - "sensitive data"
//       - confidential information
// */
//
// Legacy format (backwards compatibility):
// /*
// oxy:
//     embed: |
//         this returns fruit with sales
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
pub(super) fn parse_embed_document(id: &str, content: &str) -> Document {
    let context_regex = regex::Regex::new(r"(?m)^\/\*((?:.|\n)+)\*\/((.|\n)+)$").unwrap();
    let context_match = match context_regex.captures(content) {
        Some(m) => m,
        None => {
            tracing::warn!("No context found in the file: {:?}", id);
            return Document {
                content: content.to_string(),
                source_type: "file".to_string(),
                source_identifier: id.to_string(),
                retrieval_inclusions: vec![RetrievalContent {
                    embedding_content: content.to_string(),
                    embeddings: vec![],
                }],
                retrieval_exclusions: vec![],
                inclusion_midpoint: vec![],
                inclusion_radius: 0.0,
            };
        }
    };
    let comment_content = context_match[1].replace("\n*", "\n");
    let context_content = context_match[2].to_string();
    let header_data: Result<ContextHeader, serde_yaml::Error> =
        serde_yaml::from_str(comment_content.as_str());

    match header_data {
        Ok(header_data) => {
            let (inclusions, exclusions) = extract_retrieval_data(&header_data.oxy);

            let (retrieval_inclusions, retrieval_exclusions) =
                create_retrieval_objects(&inclusions, &exclusions, &context_content);
            let content = create_document_content(&inclusions, &exclusions, &context_content);
            let source_type = generate_sql_source_type(&header_data.oxy.database);

            Document {
                content,
                source_type,
                source_identifier: id.to_string(),
                retrieval_inclusions,
                retrieval_exclusions,
                inclusion_midpoint: vec![],
                inclusion_radius: 0.0,
            }
        }
        Err(e) => {
            tracing::warn!(
                "Failed to parse header data: {:?}, error: {:?}.\nEmbedding the whole file content",
                comment_content,
                e
            );
            Document {
                content: content.to_string(),
                source_type: "file".to_string(),
                source_identifier: id.to_string(),
                retrieval_inclusions: vec![RetrievalContent {
                    embedding_content: content.to_string(),
                    embeddings: vec![],
                }],
                retrieval_exclusions: vec![],
                inclusion_midpoint: vec![],
                inclusion_radius: 0.0,
            }
        }
    }
}

pub fn parse_sql_source_type(source_type: &str) -> Option<String> {
    if source_type.starts_with("sql::") {
        Some(source_type.strip_prefix("sql::").unwrap().to_string())
    } else {
        None
    }
}

fn extract_retrieval_data(oxy_data: &OxyHeaderData) -> (Vec<String>, Vec<String>) {
    if let Some(retrieval) = &oxy_data.retrieval {
        return (retrieval.include.clone(), retrieval.exclude.clone());
    }

    if let Some(embed) = &oxy_data.embed {
        let inclusions = match embed {
            Embed::String(embed_str) => vec![embed_str.clone()],
            Embed::Multiple(embeds) => embeds.clone(),
        };
        return (inclusions, vec![]);
    }

    (vec![], vec![])
}

fn create_retrieval_objects(
    inclusions: &[String],
    exclusions: &[String],
    context_content: &str,
) -> (Vec<RetrievalContent>, Vec<RetrievalContent>) {
    let retrieval_inclusions = if inclusions.is_empty() {
        vec![RetrievalContent {
            embedding_content: context_content.to_string(),
            embeddings: vec![],
        }]
    } else {
        inclusions
            .iter()
            .map(|inclusion| RetrievalContent {
                embedding_content: inclusion.clone(),
                embeddings: vec![],
            })
            .collect()
    };

    let retrieval_exclusions = exclusions
        .iter()
        .map(|exclusion| RetrievalContent {
            embedding_content: exclusion.clone(),
            embeddings: vec![],
        })
        .collect();

    (retrieval_inclusions, retrieval_exclusions)
}

fn create_document_content(
    inclusions: &[String],
    exclusions: &[String],
    context_content: &str,
) -> String {
    let summary = build_inclusion_exclusion_summary(inclusions, exclusions);

    if !summary.is_empty() {
        format!("{summary}\n\n{context_content}")
    } else {
        context_content.to_string()
    }
}

fn generate_sql_source_type(database: &Option<String>) -> String {
    match database {
        Some(db) => format!("sql::{db}"),
        None => "file".to_string(),
    }
}
