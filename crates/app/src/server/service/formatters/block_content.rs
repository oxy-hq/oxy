use crate::server::service::types::{Block, ContainerKind, Content};
use oxy::{config::constants::MARKDOWN_MAX_FENCES, execute::types::Output};

pub struct ContentProcessor {
    max_artifact_fences: usize,
    container_queue: Vec<String>,
}

impl ContentProcessor {
    pub fn new() -> Self {
        Self {
            max_artifact_fences: MARKDOWN_MAX_FENCES,
            container_queue: vec![],
        }
    }

    pub fn output_to_content(&self, output: &Output) -> Option<Content> {
        match output {
            Output::Text(text) => Some(Content::Text(text.clone())),
            Output::SQL(sql) => Some(Content::SQL(sql.to_string())),
            Output::Table(table) => Some(Content::Table(table.clone())),
            Output::OmniQuery(omni_query_params) => {
                Some(Content::OmniQuery(omni_query_params.clone()))
            }
            Output::SemanticQuery(semantic_query_params) => {
                Some(Content::SemanticQuery(semantic_query_params.clone()))
            }
            _ => None,
        }
    }

    pub fn output_to_text(&self, output: &Output) -> Option<String> {
        match output {
            Output::Text(text) => Some(text.to_string()),
            Output::SQL(sql) => Some(format!("Query:\n```sql\n{sql}\n```\n")),
            Output::Table(table) => Some(format!("Result:\n{}\n", table.to_markdown())),
            Output::SemanticQuery(semantic_query_params) => Some("".to_string()),
            Output::Bool(_) => None,
            Output::Prompt(prompt) => None,
            Output::Documents(documents) => None,
            Output::OmniQuery(omni_query_params) => None,
        }
    }

    pub fn prepare_container(&mut self, kind: &ContainerKind) -> (String, String) {
        let (opener, closer) = Block::container_opener_closer(kind, &mut self.max_artifact_fences);
        self.container_queue.push(closer.clone());
        (opener, closer)
    }

    pub fn get_next_closer(&mut self) -> Option<String> {
        self.container_queue.pop()
    }
}
