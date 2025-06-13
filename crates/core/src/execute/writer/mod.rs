use crate::errors::OxyError;

use super::types::Event;

mod buf_writer;
mod ordered_writer;

pub use buf_writer::BufWriter;
pub use ordered_writer::OrderedWriter;
use tokio::sync::mpsc::Sender;

#[async_trait::async_trait]
pub trait Writer {
    async fn write(&self, event: Event) -> Result<(), OxyError>;
}

#[async_trait::async_trait]
impl<T> Writer for &T
where
    T: Writer + Sync,
{
    async fn write(&self, event: Event) -> Result<(), OxyError> {
        (*self).write(event).await
    }
}

#[async_trait::async_trait]
pub trait EventHandler {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError>;
}

#[async_trait::async_trait]
impl EventHandler for Sender<Event> {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        self.send(event)
            .await
            .map_err(|_| OxyError::RuntimeError("Failed to send event".to_string()))
    }
}

pub struct NoopHandler;

#[async_trait::async_trait]
impl EventHandler for NoopHandler {
    async fn handle_event(&mut self, _event: Event) -> Result<(), OxyError> {
        Ok(())
    }
}
