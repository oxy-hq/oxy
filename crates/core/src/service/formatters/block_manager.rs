use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{
    errors::OxyError,
    execute::types::{Output, Source},
    service::types::{Block, BlockValue, ContainerKind, Content},
};

pub struct BlockManager {
    blocks: Arc<Mutex<Vec<Block>>>,
    active_blocks: Vec<Block>,
    active_content: Option<Output>,
}

impl BlockManager {
    pub fn new() -> Self {
        Self {
            blocks: Arc::new(Mutex::new(vec![])),
            active_blocks: vec![],
            active_content: None,
        }
    }

    pub fn get_blocks_clone(&self) -> Arc<Mutex<Vec<Block>>> {
        Arc::clone(&self.blocks)
    }

    pub fn last_block(&self) -> Option<&Block> {
        self.active_blocks.last()
    }

    pub async fn add_container_block(
        &mut self,
        source: &Source,
        kind: &ContainerKind,
    ) -> Result<(), OxyError> {
        // Create a new block for the container
        let block = Block::container(source.id.to_string(), kind.clone());
        self.active_blocks.push(block);
        Ok(())
    }

    pub async fn add_content(&mut self, source: &Source, content: Content) -> Result<(), OxyError> {
        // If there's an active block, add the content to it
        if let Some(active_block) = self.active_blocks.last_mut() {
            if let BlockValue::Children { children, .. } = &mut *active_block.value {
                children.push(Block::content(source.id.to_string(), content));
            }
        } else {
            self.blocks
                .lock()
                .await
                .push(Block::content(source.id.to_string(), content));
        }
        Ok(())
    }

    pub async fn finish_block(&mut self, source: &Source) -> Result<bool, OxyError> {
        if !self
            .active_blocks
            .last()
            .map_or(false, |b| b.id.as_str() == source.id.as_str())
        {
            return Ok(false);
        }

        if let Some(active_block) = self.active_blocks.pop() {
            match self.active_blocks.last_mut() {
                Some(parent_block) => {
                    if let BlockValue::Children { children, .. } = &mut *parent_block.value {
                        children.push(active_block);
                    }
                }
                None => {
                    // If there's no parent, push to the main blocks
                    self.blocks.lock().await.push(active_block);
                }
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub fn update_content(&mut self, chunk: &Output) {
        match &self.active_content {
            Some(content) => {
                let merged = content.merge(chunk);
                self.active_content = Some(merged);
            }
            None => {
                self.active_content = Some(chunk.clone());
            }
        }
    }

    pub fn finalize_content(&mut self, chunk: &Output) -> Option<Output> {
        self.update_content(chunk);
        let result = self.active_content.take();
        result
    }

    pub fn is_artifact_active(&self, artifact_id: &str) -> bool {
        self.active_blocks
            .last()
            .map_or(false, |b| b.is_artifact() && b.id.as_str() == artifact_id)
    }
}
