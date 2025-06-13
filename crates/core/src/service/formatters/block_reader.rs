use std::sync::Arc;

use sea_orm::Set;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    config::constants::MARKDOWN_MAX_FENCES, errors::OxyError, service::types::Block,
    utils::try_unwrap_arc_tokio_mutex,
};

pub struct BlockHandlerReader {
    blocks: Arc<Mutex<Vec<Block>>>,
    artifacts: Arc<Mutex<Vec<entity::artifacts::ActiveModel>>>,
}

impl BlockHandlerReader {
    pub fn new(
        blocks: Arc<Mutex<Vec<Block>>>,
        artifacts: Arc<Mutex<Vec<entity::artifacts::ActiveModel>>>,
    ) -> Self {
        BlockHandlerReader { blocks, artifacts }
    }

    pub async fn into_active_models(
        self,
    ) -> Result<
        (
            entity::messages::ActiveModel,
            Vec<entity::artifacts::ActiveModel>,
        ),
        OxyError,
    > {
        let blocks = try_unwrap_arc_tokio_mutex(self.blocks).await?;
        let content = blocks.into_iter().fold(String::new(), |mut acc, block| {
            acc.push_str(block.to_markdown(MARKDOWN_MAX_FENCES).as_str());
            acc.push('\n');
            acc
        });
        let message = entity::messages::ActiveModel {
            id: Set(Uuid::new_v4()),
            content: Set(content),
            is_human: Set(false),
            ..Default::default()
        };
        let artifacts = try_unwrap_arc_tokio_mutex(self.artifacts).await?;
        Ok((message, artifacts))
    }
}
