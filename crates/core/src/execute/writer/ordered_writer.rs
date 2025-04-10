use tokio::sync::mpsc::{Receiver, Sender, channel};

use crate::{errors::OxyError, execute::types::Event};

pub struct OrderedWriter {
    receivers: Vec<Receiver<Event>>,
}

impl Default for OrderedWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderedWriter {
    pub fn new() -> Self {
        OrderedWriter { receivers: vec![] }
    }

    pub fn create_writer(&mut self, buffer: Option<usize>) -> Sender<Event> {
        let (tx, rx) = channel::<Event>(buffer.unwrap_or(100));
        self.receivers.push(rx);
        tx
    }

    pub async fn write_sender(self, sender: Sender<Event>) -> Result<(), OxyError> {
        let receivers = self.receivers;
        for mut rx in receivers.into_iter() {
            while let Some(event) = rx.recv().await {
                sender.send(event).await?;
            }
        }
        Ok(())
    }

    pub async fn write_partition(self, idx: usize, sender: Sender<Event>) -> Result<(), OxyError> {
        let mut receivers = self.receivers;
        let rx = receivers
            .get_mut(idx)
            .ok_or(OxyError::RuntimeError("Event Writer not found".to_string()))?;
        while let Some(event) = rx.recv().await {
            sender.send(event).await?;
        }
        Ok(())
    }
}
