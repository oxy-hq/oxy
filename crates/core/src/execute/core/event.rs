use tokio::sync::mpsc::{Receiver, Sender};

pub trait Handler: Send + Sync {
    type Event;
    fn handle(&self, event: &Self::Event);
}

pub struct Dispatcher<Event> {
    handlers: Vec<Box<dyn Handler<Event = Event>>>,
}

impl<Event> Dispatcher<Event> {
    pub fn new(handlers: Vec<Box<dyn Handler<Event = Event>>>) -> Self {
        Dispatcher { handlers }
    }
}

impl<Event> Handler for Dispatcher<Event> {
    type Event = Event;

    fn handle(&self, event: &Self::Event) {
        for handler in &self.handlers {
            handler.handle(event);
        }
    }
}

pub fn propagate<EventOrigin, EventDest>(
    mut rx: Receiver<EventOrigin>,
    tx: Sender<EventDest>,
    map_event: impl Fn(EventOrigin) -> EventDest + Send + 'static,
) -> tokio::task::JoinHandle<()>
where
    EventOrigin: Send + 'static,
    EventDest: Send + 'static,
{
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            let event = map_event(event);
            let _ = tx.send(event).await;
        }
    })
}

pub fn consume<Event>(
    mut rx: Receiver<Event>,
    handler: impl Handler<Event = Event> + 'static,
) -> tokio::task::JoinHandle<()>
where
    Event: Send + 'static,
{
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            handler.handle(&event);
        }
    })
}
