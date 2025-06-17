use std::sync::Arc;

use sea_orm::Set;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    config::constants::MARKDOWN_MAX_FENCES, errors::OxyError, execute::types::Usage,
    service::types::Block, utils::try_unwrap_arc_tokio_mutex,
};

pub struct BlockHandlerReader {
    blocks: Arc<Mutex<Vec<Block>>>,
    artifacts: Arc<Mutex<Vec<entity::artifacts::ActiveModel>>>,
    usage: Arc<Mutex<Usage>>,
}

impl BlockHandlerReader {
    pub fn new(
        blocks: Arc<Mutex<Vec<Block>>>,
        artifacts: Arc<Mutex<Vec<entity::artifacts::ActiveModel>>>,
        usage: Arc<Mutex<Usage>>,
    ) -> Self {
        BlockHandlerReader {
            blocks,
            artifacts,
            usage,
        }
    }

    pub async fn usage(&self) -> Result<Usage, OxyError> {
        let usage = try_unwrap_arc_tokio_mutex(self.usage.clone()).await?;
        Ok(usage)
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
        let usage = try_unwrap_arc_tokio_mutex(self.usage).await?;
        let content = blocks.into_iter().fold(String::new(), |mut acc, block| {
            acc.push_str(block.to_markdown(MARKDOWN_MAX_FENCES).as_str());
            acc.push('\n');
            acc
        });
        println!(
            "Usage - input: {}, output: {}",
            usage.input_tokens, usage.output_tokens
        );
        let message = entity::messages::ActiveModel {
            id: Set(Uuid::new_v4()),
            content: Set(content),
            is_human: Set(false),
            input_tokens: Set(usage.input_tokens),
            output_tokens: Set(usage.output_tokens),
            ..Default::default()
        };
        let artifacts = try_unwrap_arc_tokio_mutex(self.artifacts).await?;
        Ok((message, artifacts))
    }
}
