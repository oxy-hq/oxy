use std::sync::{Arc, Mutex};

use crate::{
    errors::OxyError,
    execute::{
        types::{DataAppReference, Event, EventKind, ReferenceKind},
        writer::EventHandler,
    },
};

pub struct AgentReferencesHandler<H> {
    handler: H,
    source_id: Option<String>,
    pub references: Arc<Mutex<Vec<ReferenceKind>>>,
}

impl<H> AgentReferencesHandler<H> {
    pub fn new(handler: H, source_id: Option<String>) -> Self {
        AgentReferencesHandler {
            handler,
            source_id,
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
        if let Some(source_id) = &self.source_id {
            if &event.source.id != source_id
                && event
                    .source
                    .parent_id
                    .clone()
                    .is_none_or(|parent_id| &parent_id != source_id)
            {
                self.handler.handle_event(event).await?;
                return Ok(());
            }
        }

        if let EventKind::Updated { chunk } = &event.kind {
            if let Ok(reference) = TryInto::<ReferenceKind>::try_into(chunk.delta.clone()) {
                let mut references = self.references.lock().unwrap();
                references.push(reference);
            }
        }
        if let EventKind::DataAppCreated { data_app } = &event.kind {
            let mut references = self.references.lock().unwrap();
            references.push(ReferenceKind::DataApp(DataAppReference {
                file_path: data_app.file_path.clone(),
            }));
        }
        self.handler.handle_event(event).await?;
        Ok(())
    }
}
