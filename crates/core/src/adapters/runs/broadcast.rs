use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    errors::OxyError,
    execute::{types::Event, writer::EventHandler},
    service::types::event::EventKind,
};

pub trait Mergeable {
    fn merge(&mut self, other: Self) -> bool;
}

#[derive(Debug)]
pub struct Subscribed<T> {
    pub items: Vec<T>,
    pub receiver: tokio::sync::broadcast::Receiver<T>,
}

#[derive(Debug)]
pub struct Closed<T> {
    pub items: Vec<T>,
    pub sender: tokio::sync::broadcast::Sender<T>,
}

#[derive(Debug)]
pub enum TopicActorMessage<T> {
    Subscribe(tokio::sync::oneshot::Sender<Subscribed<T>>),
}

impl<T> TopicActorMessage<T> {
    pub fn subscribe() -> (Self, tokio::sync::oneshot::Receiver<Subscribed<T>>) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        (Self::Subscribe(sender), receiver)
    }
}

#[derive(Debug)]
pub struct TopicRef<T> {
    tx: tokio::sync::mpsc::Sender<T>,
}

impl<T> TopicRef<T> {
    fn new(tx: tokio::sync::mpsc::Sender<T>) -> Self {
        Self { tx }
    }

    pub async fn send_event(&self, item: T) -> Result<(), OxyError> {
        self.tx
            .send(item)
            .await
            .map_err(|_| OxyError::RuntimeError("Failed to send event".to_string()))
    }
}

#[async_trait::async_trait]
impl EventHandler for TopicRef<EventKind> {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        tracing::info!(?event, "Received event for topic");
        if let Ok(event_kind) = TryInto::<EventKind>::try_into(event) {
            return self.send_event(event_kind.clone()).await;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct TopicActorRef<T> {
    tx: tokio::sync::mpsc::Sender<TopicActorMessage<T>>,
    close_rx: tokio::sync::oneshot::Receiver<Closed<T>>,
}

impl<T> TopicActorRef<T> {
    fn new(
        tx: tokio::sync::mpsc::Sender<TopicActorMessage<T>>,
        close_rx: tokio::sync::oneshot::Receiver<Closed<T>>,
    ) -> Self {
        Self { tx, close_rx }
    }

    fn subscribe_ref(&self) -> SubscribeRef<T> {
        SubscribeRef::new(self.tx.clone())
    }

    async fn close(self) -> Result<Closed<T>, OxyError> {
        self.close_rx
            .await
            .map_err(|_| OxyError::RuntimeError("Failed to receive close signal".to_string()))
    }
}

pub struct SubscribeRef<T> {
    tx: tokio::sync::mpsc::Sender<TopicActorMessage<T>>,
}

impl<T> SubscribeRef<T> {
    pub fn new(tx: tokio::sync::mpsc::Sender<TopicActorMessage<T>>) -> Self {
        Self { tx }
    }

    pub async fn subscribe(&self) -> Result<Subscribed<T>, OxyError> {
        let (event, rx) = TopicActorMessage::subscribe();
        self.tx
            .send(event)
            .await
            .map_err(|_| OxyError::RuntimeError("Failed to subscribe".to_string()))?;
        rx.await
            .map_err(|_| OxyError::RuntimeError("Failed to receive subscription".to_string()))
    }
}

pub struct TopicActor<T> {
    mailbox: Vec<T>,
    inbound: tokio::sync::mpsc::Receiver<T>,
    sys_inbound: tokio::sync::mpsc::Receiver<TopicActorMessage<T>>,
    outbound: tokio::sync::broadcast::Sender<T>,
    close_signal: tokio::sync::oneshot::Sender<Closed<T>>,
}

impl<T> TopicActor<T>
where
    T: Mergeable + std::fmt::Debug + Clone + Send + Sync + 'static,
{
    fn new(
        sys_inbound: tokio::sync::mpsc::Receiver<TopicActorMessage<T>>,
        inbound: tokio::sync::mpsc::Receiver<T>,
        close_signal: tokio::sync::oneshot::Sender<Closed<T>>,
    ) -> Self {
        Self {
            mailbox: Vec::new(),
            inbound,
            sys_inbound,
            close_signal,
            outbound: tokio::sync::broadcast::channel(1024).0,
        }
    }

    pub fn pair(
        broadcast_buffer: usize,
        sys_buffer: usize,
    ) -> (Self, TopicActorRef<T>, TopicRef<T>) {
        let (close_tx, close_rx) = tokio::sync::oneshot::channel();
        let (sys_tx, sys_rx) = tokio::sync::mpsc::channel(sys_buffer);
        let (tx, rx) = tokio::sync::mpsc::channel(broadcast_buffer);
        (
            Self::new(sys_rx, rx, close_tx),
            TopicActorRef::new(sys_tx, close_rx),
            TopicRef::new(tx),
        )
    }

    pub async fn run(mut self) -> Result<(), OxyError> {
        tracing::info!("Topic actor started");
        loop {
            tokio::select! {
                sys_event = self.sys_inbound.recv() => {
                    tracing::info!("Topic actor received system event: {:?}", sys_event);
                    match sys_event {
                        Some(TopicActorMessage::Subscribe(sender)) => {
                            let (items, receiver) = (self.mailbox.clone(), self.outbound.subscribe());
                            let _ = sender.send(Subscribed { items, receiver });
                        }
                        None => {
                            tracing::warn!("Received None from sys_inbound channel, exiting topic actor");
                            break;
                        }
                    }
                }
                event = self.inbound.recv() => {
                    tracing::debug!("Topic actor received event: {:?}", event);
                    match event {
                        Some(item) => {
                            if let Some(last) = self.mailbox.last_mut() {
                                if !last.merge(item.clone()) {
                                    self.mailbox.push(item.clone());
                                }
                            } else {
                                self.mailbox.push(item.clone());
                            }
                            let res = self.outbound.send(item);
                            tracing::debug!("Broadcasted event result: {:?}", res);
                            if res.is_err() {
                                tracing::error!("Failed to send event to topic subscribers: {res:?}");
                            }
                        }
                        None => {
                            tracing::warn!("Received None from inbound channel, exiting topic actor");
                            break;
                        }
                    }

                }
            }
        }
        tracing::info!("Topic actor finished running");
        self.close_signal
            .send(Closed {
                items: self.mailbox,
                sender: self.outbound,
            })
            .map_err(|_| OxyError::RuntimeError("Failed to send close signal".to_string()))?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct Broadcaster<T> {
    topics: RwLock<HashMap<String, TopicActorRef<T>>>,
    broadcast_buffer: usize,
    sys_buffer: usize,
}

impl<T> Broadcaster<T>
where
    T: Mergeable + std::fmt::Debug + Clone + Send + Sync + 'static,
{
    pub fn new(broadcast_buffer: usize, sys_buffer: usize) -> Self {
        Self {
            topics: RwLock::new(HashMap::new()),
            broadcast_buffer,
            sys_buffer,
        }
    }

    pub async fn create_topic(self: &Arc<Self>, topic: &str) -> Result<TopicRef<T>, OxyError> {
        let mut topics = self.topics.write().await;
        // Check if the topic already exists and throw an error if it does
        if topics.contains_key(topic) {
            return Err(OxyError::RuntimeError(format!(
                "Topic '{topic}' already exists"
            )));
        }

        let (topic_actor, topic_sys_ref, topic_ref) =
            TopicActor::<T>::pair(self.broadcast_buffer, self.sys_buffer);
        tokio::spawn(async move { topic_actor.run().await });
        topics.insert(topic.to_string(), topic_sys_ref);

        Ok(topic_ref)
    }

    pub async fn has_topic(&self, topic: &str) -> bool {
        let topics = self.topics.read().await;
        topics.contains_key(topic)
    }

    pub async fn list_topics<I: FromIterator<String>>(&self) -> I {
        let topics = self.topics.read().await;
        topics.keys().cloned().collect()
    }

    pub async fn subscribe(&self, topic: &str) -> Result<Subscribed<T>, OxyError> {
        let subscription = {
            let topics = self.topics.read().await;
            let topic = topics
                .get(topic)
                .ok_or_else(|| OxyError::RuntimeError(format!("Topic '{topic}' does not exist")))?;
            topic.subscribe_ref()
        };
        subscription.subscribe().await
    }

    /// Manually remove a topic and drop its retained state
    pub async fn remove_topic(&self, topic: &str) -> Option<Closed<T>> {
        let topic_ref = {
            let mut topics = self.topics.write().await;
            topics.remove(topic)
        };

        match topic_ref {
            Some(topic_ref) => {
                let closed = topic_ref.close().await.ok();
                tracing::info!("Removed topic: {topic}");
                closed
            }
            None => {
                tracing::warn!("Attempted to remove non-existent topic: {topic}");
                None
            }
        }
    }
}
