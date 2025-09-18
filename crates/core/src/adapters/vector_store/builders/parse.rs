use serde::Deserialize;

use crate::adapters::vector_store::types::RetrievalObject;

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
//       - get fruit sales data
//     exclude:
//       - get apple computer sales data
//       - get fruit revenue data
// */
//
// Legacy format (backwards compatibility):
// /*
// oxy:
//     embed: |
//         get fruit sales data
//         fruit includes apple, banana, kiwi, cherry and orange
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
pub(super) fn parse_retrieval_object(id: &str, content: &str) -> RetrievalObject {
    let context_regex = regex::Regex::new(r"(?m)^\/\*((?:.|\n)+)\*\/((.|\n)+)$").unwrap();
    let context_match = match context_regex.captures(content) {
        Some(m) => m,
        None => {
            tracing::warn!("No context found in the file: {:?}", id);
            return RetrievalObject {
                source_identifier: id.to_string(),
                source_type: "file".to_string(),
                context_content: content.to_string(),
                inclusions: vec![content.to_string()],
                ..Default::default()
            };
        }
    };
    let comment_content = context_match[1].replace("\n*", "\n");
    let header_data: Result<ContextHeader, serde_yaml::Error> =
        serde_yaml::from_str(comment_content.as_str());

    match header_data {
        Ok(header_data) => {
            let (inclusions, exclusions) = extract_retrieval_data(&header_data.oxy);
            let source_type = generate_sql_source_type(&header_data.oxy.database);

            RetrievalObject {
                source_identifier: id.to_string(),
                source_type,
                context_content: content.to_string(),
                inclusions,
                exclusions,
                ..Default::default()
            }
        }
        Err(e) => {
            tracing::warn!(
                "Failed to parse header data: {:?}, error: {:?}.\nEmbedding the whole file content",
                comment_content,
                e
            );
            RetrievalObject {
                source_identifier: id.to_string(),
                source_type: "file".to_string(),
                context_content: content.to_string(),
                inclusions: vec![content.to_string()],
                ..Default::default()
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

fn generate_sql_source_type(database: &Option<String>) -> String {
    match database {
        Some(db) => format!("sql::{db}"),
        None => "file".to_string(),
    }
}
