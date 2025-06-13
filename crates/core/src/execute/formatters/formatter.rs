use crate::{
    errors::OxyError,
    execute::{
        types::{Event, EventKind, Source},
        writer::EventHandler,
    },
};

pub type FormatterResult = Result<(), OxyError>;

#[async_trait::async_trait]
pub trait SourceHandler {
    fn supported_source_kinds(&self) -> Vec<String> {
        vec![] // Default: handle any source kind
    }

    fn excluded_source_kinds(&self) -> Vec<String> {
        vec![] // Default: do not exclude any source kind
    }

    fn supported_source_ids(&self) -> Vec<String> {
        vec![] // Default: handle any source ID
    }

    fn can_handle_source(&self, source: &Source) -> bool {
        // Check source kind
        let supported_kinds = self.supported_source_kinds();
        let kind_match = supported_kinds.iter().any(|kind| kind.eq(&source.kind));

        if self.excluded_source_kinds().contains(&source.kind) {
            return false; // Exclude this source kind
        }

        // If no specific kinds are supported, accept any kind
        if !kind_match && !supported_kinds.is_empty() {
            return false;
        }

        // Check specific source IDs if specified
        let supported_ids = self.supported_source_ids();
        if supported_ids.is_empty() {
            true // Accept any source ID
        } else {
            supported_ids.contains(&source.id)
        }
    }

    async fn handle(&mut self, event: &Event) -> FormatterResult {
        let Event {
            source,
            kind: event_kind,
        } = event;

        if !self.can_handle_source(source) {
            return Ok(()); // Skip if the source is not supported
        }

        self.handle_event(source, event_kind).await
    }

    async fn handle_event(&mut self, source: &Source, event_kind: &EventKind) -> FormatterResult;
}

#[async_trait::async_trait]
impl<T> EventHandler for T
where
    T: SourceHandler + Send + Sync,
{
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        self.handle(&event).await
    }
}
