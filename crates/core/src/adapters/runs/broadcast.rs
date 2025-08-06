use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::sync::{Mutex, RwLock, mpsc};

use crate::{
    errors::OxyError,
    execute::{types::Event, writer::EventHandler},
    service::types::event::EventKind,
};

pub trait Mergeable {
    fn merge(&mut self, other: Self) -> bool;
}

#[derive(Debug)]
pub struct Broadcaster<T> {
    topics: RwLock<HashMap<String, TopicChannel<T>>>,
    channel_capacity: usize,
}

impl<T> Broadcaster<T>
where
    T: Mergeable + Clone + Send + Sync + 'static,
{
    pub fn new(channel_capacity: usize) -> Self {
        Self {
            topics: RwLock::new(HashMap::new()),
            channel_capacity,
        }
    }

    pub async fn create_topic(
        self: &Arc<Self>,
        topic: &str,
    ) -> Result<TopicPublisher<T>, OxyError> {
        let mut topics = self.topics.write().await;
        // Check if the topic already exists and throw an error if it does
        if topics.contains_key(topic) {
            return Err(OxyError::RuntimeError(format!(
                "Topic '{topic}' already exists"
            )));
        }
        let channel = TopicChannel::new(topic);
        topics.insert(topic.to_string(), channel);
        Ok(TopicPublisher::new(topic, Arc::clone(self)))
    }

    pub async fn has_topic(&self, topic: &str) -> bool {
        let topics = self.topics.read().await;
        topics.contains_key(topic)
    }

    pub async fn list_topics<I: FromIterator<String>>(&self) -> I {
        let topics = self.topics.read().await;
        topics.keys().cloned().collect()
    }

    pub async fn publish(&self, topic: &str, event: T) -> Result<(), OxyError> {
        let topics = self.topics.read().await;
        let topic = topics
            .get(topic)
            .ok_or_else(|| OxyError::RuntimeError(format!("Topic '{topic}' does not exist")))?;
        topic.publish(event).await;
        Ok(())
    }

    pub async fn subscribe(&self, topic: &str) -> Result<mpsc::Receiver<T>, OxyError> {
        let (tx, rx) = mpsc::channel(self.channel_capacity);
        let topics = self.topics.read().await;
        let topic = topics
            .get(topic)
            .ok_or_else(|| OxyError::RuntimeError(format!("Topic '{topic}' does not exist")))?;
        topic.subscribe(tx).await;
        Ok(rx)
    }

    /// Manually remove a topic and drop its retained state
    pub async fn remove_topic(&self, topic: &str) -> Option<TopicChannel<T>> {
        let mut topics = self.topics.write().await;
        topics.remove(topic)
    }
}

#[derive(Debug, Clone)]
pub struct TopicPublisher<T> {
    topic: String,
    broadcaster: Arc<Broadcaster<T>>,
}

impl<T> TopicPublisher<T>
where
    T: Mergeable + Clone + Send + Sync + 'static,
{
    pub fn new(topic: impl Into<String>, broadcaster: Arc<Broadcaster<T>>) -> Self {
        Self {
            topic: topic.into(),
            broadcaster,
        }
    }

    pub async fn publish(&self, event: T) -> Result<(), OxyError> {
        self.broadcaster.publish(&self.topic, event).await
    }
}

#[derive(Debug)]
pub struct TopicChannel<T> {
    name: String,
    history: Mutex<VecDeque<T>>,
    subscribers: Mutex<Vec<mpsc::Sender<T>>>,
}

impl<T> TopicChannel<T>
where
    T: Mergeable + Clone + Send + Sync + 'static,
{
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            history: Mutex::new(VecDeque::new()),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub fn finalize(self) -> (Vec<T>, Vec<mpsc::Sender<T>>) {
        (
            self.history.into_inner().into_iter().collect(),
            self.subscribers.into_inner().into_iter().collect(),
        )
    }

    pub async fn subscribe(&self, tx: mpsc::Sender<T>) {
        // Lock history and subscribers together
        let history_items: Vec<T> = {
            let history = self.history.lock().await;
            let mut subs = self.subscribers.lock().await;
            subs.push(tx.clone());
            history.iter().cloned().collect()
        };

        // Send history outside lock
        for event in history_items {
            let _ = tx.send(event).await;
        }
    }

    pub async fn publish(&self, event: T) {
        {
            let mut history = self.history.lock().await;
            let event_clone = event.clone();
            match history.back_mut() {
                Some(last) => {
                    if !last.merge(event_clone) {
                        history.push_back(event.clone());
                    }
                }
                None => {
                    history.push_back(event_clone);
                }
            }
        }

        let mut subs = self.subscribers.lock().await;
        subs.retain(|tx| tx.try_send(event.clone()).is_ok());
    }
}

#[async_trait::async_trait]
impl EventHandler for TopicPublisher<EventKind> {
    async fn handle_event(&mut self, event: Event) -> Result<(), OxyError> {
        if let Ok(event_kind) = TryInto::<EventKind>::try_into(event) {
            let _ = self.publish(event_kind.clone()).await;
        }
        Ok(())
    }
}
