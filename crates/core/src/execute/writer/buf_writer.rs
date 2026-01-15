use tokio::sync::mpsc::{Receiver, Sender, channel};

use crate::execute::types::Event;
use oxy_shared::errors::OxyError;

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

    pub async fn write_when<F: Fn(&Event) -> bool>(
        self,
        sender: Sender<Event>,
        predict: F,
    ) -> Result<Vec<Event>, OxyError> {
        let mut rx = self
            .rx
            .ok_or(OxyError::RuntimeError("Writer not created".to_string()))?;
        let mut buffer: Vec<Event> = vec![];
        let mut is_drained = false;
        while let Some(event) = rx.recv().await {
            // Add the event to the buffer if we are not drained and the prediction fails
            if !is_drained && !predict(&event) {
                buffer.push(event);
                continue;
            }

            // If we are already drained, we can send the event directly
            if is_drained {
                sender.send(event).await?;
                continue;
            }

            // If we are not drained, we need to send the buffered events first
            for buffered_event in buffer.drain(..) {
                sender.send(buffered_event).await?;
            }
            sender.send(event).await?;
            is_drained = true;
        }
        Ok(buffer)
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
