use crate::{
    config::constants::MARKDOWN_MAX_FENCES,
    execute::types::Output,
    service::types::{Block, ContainerKind, Content},
};

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
            _ => None,
        }
    }

    pub fn output_to_text(&self, output: &Output) -> Option<String> {
        match output {
            Output::Text(text) => Some(text.to_string()),
            Output::SQL(sql) => Some(format!("Query:\n```sql\n{}\n```\n", sql)),
            Output::Table(table) => Some(format!("Result:\n{}\n", table.to_markdown())),
            _ => None,
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
