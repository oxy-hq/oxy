use std::sync::Arc;

use crate::{
    adapters::runs::TopicChannel,
    errors::OxyError,
    execute::{
        types::Event,
        writer::{EventHandler, Handler},
    },
    service::{
        block::GroupBlockHandler,
        types::{block::Group, event::EventKind},
    },
};

pub struct WorkflowEventHandler {
    topic: Arc<TopicChannel<EventKind>>,
    group_block_handler: GroupBlockHandler,
}

impl WorkflowEventHandler {
    pub fn new(topic: Arc<TopicChannel<EventKind>>) -> Self {
        Self {
            topic,
            group_block_handler: GroupBlockHandler::new(),
        }
    }

    pub fn collect(self) -> Vec<Group> {
        self.group_block_handler.collect()
    }
}

#[async_trait::async_trait]
impl EventHandler for WorkflowEventHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        tracing::debug!(?event, "Received event");
        if let Ok(event_kind) = TryInto::<EventKind>::try_into(event) {
            self.topic.publish(event_kind.clone()).await;
            self.group_block_handler.handle_event(event_kind).await?;
        }
        Ok(())
    }
}
