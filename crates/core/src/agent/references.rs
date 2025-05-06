use std::sync::{Arc, Mutex};

use crate::{
    errors::OxyError,
    execute::{
        types::{Event, EventKind, ReferenceKind},
        writer::EventHandler,
    },
};

pub struct AgentReferencesHandler<H> {
    handler: H,
    pub references: Arc<Mutex<Vec<ReferenceKind>>>,
}

impl<H> AgentReferencesHandler<H> {
    pub fn new(handler: H) -> Self {
        AgentReferencesHandler {
            handler,
            references: Arc::new(Mutex::new(vec![])),
        }
    }
}

#[async_trait::async_trait]
impl<H> EventHandler for AgentReferencesHandler<H>
where
    H: EventHandler + Send + 'static,
{
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let EventKind::Updated { chunk } = &event.kind {
            if let Some(reference) = TryInto::<ReferenceKind>::try_into(chunk.delta.clone()).ok() {
                let mut references = self.references.lock().unwrap();
                references.push(reference);
            }
        }
        self.handler.handle_event(event).await?;
        Ok(())
    }
}
