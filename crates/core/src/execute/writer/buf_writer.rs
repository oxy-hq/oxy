use tokio::sync::mpsc::{Receiver, Sender, channel};

use crate::{errors::OxyError, execute::types::Event};

use super::EventHandler;

pub struct BufWriter {
    rx: Option<Receiver<Event>>,
}

impl Default for BufWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BufWriter {
    pub fn new() -> Self {
        BufWriter { rx: None }
    }

    pub fn create_writer(&mut self, buffer: Option<usize>) -> Result<Sender<Event>, OxyError> {
        if self.rx.is_some() {
            return Err(OxyError::RuntimeError("Writer already created".to_string()));
        }

        let (tx, rx) = channel::<Event>(buffer.unwrap_or(100));
        self.rx = Some(rx);
        Ok(tx)
    }

    pub async fn write(self, sender: Sender<Event>) -> Result<(), OxyError> {
        let mut rx = self
            .rx
            .ok_or(OxyError::RuntimeError("Writer not created".to_string()))?;
        while let Some(event) = rx.recv().await {
            sender.send(event).await?;
        }
        Ok(())
    }

    pub async fn write_to_handler<H: EventHandler>(self, handler: H) -> Result<(), OxyError> {
        let mut rx = self
            .rx
            .ok_or(OxyError::RuntimeError("Writer not created".to_string()))?;
        let mut handler = handler;
        while let Some(event) = rx.recv().await {
            handler.handle_event(event).await?;
        }
        Ok(())
    }

    pub async fn write_and_copy(self, sender: Sender<Event>) -> Result<Vec<Event>, OxyError> {
        let mut rx = self
            .rx
            .ok_or(OxyError::RuntimeError("Writer not created".to_string()))?;
        let mut events = vec![];
        while let Some(event) = rx.recv().await {
            events.push(event.clone());
            sender.send(event).await?;
        }
        Ok(events)
    }
}
