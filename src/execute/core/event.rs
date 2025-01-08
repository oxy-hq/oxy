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
