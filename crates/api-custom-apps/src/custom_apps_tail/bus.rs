//! This process's subscribers, by `(app, environment)`.
//!
//! The listener (`super::listener`) is the only caller of
//! [`deliver_local`]: every envelope reaches a process through `LISTEN`, the
//! producing process's own included, so a subscriber on the producer's
//! replica is not delivered each event twice (chat's lesson,
//! `chat::delivery::wake_local`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::broadcast;
use uuid::Uuid;

use super::event::{Envelope, TailEvent};

/// Events one `(app, environment)` ring holds for its slowest subscriber.
/// Past it the oldest are dropped and the subscriber is told how many — a
/// slow reader costs itself events, never the process memory.
pub const SUBSCRIBER_BUFFER: usize = 256;

type Key = (Uuid, String);
type Registry = Mutex<HashMap<Key, broadcast::Sender<Arc<TailEvent>>>>;

fn registry() -> &'static Registry {
    static SUBS: OnceLock<Registry> = OnceLock::new();
    SUBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// What a subscriber reads next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Event(Arc<TailEvent>),
    /// This subscriber fell [`SUBSCRIBER_BUFFER`] behind and lost the
    /// `events` oldest; the stream goes on from the oldest still held.
    Dropped {
        events: u64,
    },
    /// Nothing will arrive any more.
    Closed,
}

/// One subscriber of one environment.
pub struct Subscription {
    rx: broadcast::Receiver<Arc<TailEvent>>,
}

impl Subscription {
    pub async fn next(&mut self) -> Frame {
        match self.rx.recv().await {
            Ok(event) => Frame::Event(event),
            Err(broadcast::error::RecvError::Lagged(events)) => Frame::Dropped { events },
            Err(broadcast::error::RecvError::Closed) => Frame::Closed,
        }
    }
}

/// Subscribe to `environment` of `app_id` in this process.
pub fn subscribe(app_id: Uuid, environment: &str) -> Subscription {
    let mut map = registry().lock().unwrap_or_else(|p| p.into_inner());
    let tx = map
        .entry((app_id, environment.to_string()))
        .or_insert_with(|| broadcast::channel(SUBSCRIBER_BUFFER).0);
    Subscription { rx: tx.subscribe() }
}

/// Hand an envelope to this process's subscribers of its environment, and
/// to no other environment's. An environment nobody here watches costs a
/// map lookup, and its entry is dropped once its last subscriber has gone.
pub fn deliver_local(envelope: Envelope) {
    let key = (envelope.app_id, envelope.environment);
    let mut map = registry().lock().unwrap_or_else(|p| p.into_inner());
    let Some(tx) = map.get(&key) else {
        return;
    };
    for event in envelope.events {
        if tx.send(Arc::new(event)).is_err() {
            map.remove(&key);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(app_id: Uuid, environment: &str, seqs: std::ops::RangeInclusive<u32>) -> Envelope {
        Envelope {
            app_id,
            environment: environment.to_string(),
            events: seqs
                .map(|seq| TailEvent::log(Uuid::nil(), "probe", seq, "log", "line"))
                .collect(),
        }
    }

    fn seq(frame: &Frame) -> Option<u32> {
        match frame {
            Frame::Event(event) => match event.as_ref() {
                TailEvent::Log { seq, .. } => Some(*seq),
                _ => None,
            },
            _ => None,
        }
    }

    /// A subscriber of one environment reads that environment's events, and
    /// never another environment's of the same app.
    #[tokio::test]
    async fn an_environment_reaches_its_own_subscribers_only() {
        let app = Uuid::new_v4();
        let mut a = subscribe(app, "dev-a1");
        let mut b = subscribe(app, "dev-b2");
        deliver_local(envelope(app, "dev-a1", 1..=1));
        assert_eq!(seq(&a.next().await), Some(1));
        assert!(b.rx.try_recv().is_err(), "dev-b2 saw dev-a1's line");
        deliver_local(envelope(app, "dev-b2", 2..=2));
        assert_eq!(seq(&b.next().await), Some(2));
    }

    /// A subscriber that stops reading loses the oldest events past the
    /// buffer, reads one marker saying how many, then the newest
    /// `SUBSCRIBER_BUFFER` in order — the buffer never grows.
    #[tokio::test]
    async fn a_slow_subscriber_is_told_what_it_lost_and_the_buffer_does_not_grow() {
        let app = Uuid::new_v4();
        let mut slow = subscribe(app, "staging");
        let total = SUBSCRIBER_BUFFER as u32 + 44;
        deliver_local(envelope(app, "staging", 1..=total));
        assert_eq!(slow.next().await, Frame::Dropped { events: 44 });
        for expected in 45..=total {
            assert_eq!(seq(&slow.next().await), Some(expected));
        }
        assert!(slow.rx.try_recv().is_err(), "nothing beyond the buffer");
    }

    /// An environment nobody in this process watches is a lookup, not an
    /// entry; one whose last subscriber left is forgotten on its next event.
    #[test]
    fn an_unwatched_environment_leaves_no_entry() {
        let app = Uuid::new_v4();
        deliver_local(envelope(app, "production", 1..=1));
        let len = || {
            registry()
                .lock()
                .unwrap()
                .keys()
                .filter(|(id, _)| *id == app)
                .count()
        };
        assert_eq!(len(), 0);
        drop(subscribe(app, "production"));
        assert_eq!(len(), 1);
        deliver_local(envelope(app, "production", 2..=2));
        assert_eq!(len(), 0, "a dead entry is removed");
    }
}
