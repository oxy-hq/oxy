use super::{event::Handler, value::ContextValue};

pub trait Write<Event>: Send + Sync {
    fn write(&mut self, value: ContextValue);
    fn notify(&self, event: Event);
}

pub struct OutputCollector<'handler, Event> {
    pub output: Option<ContextValue>,
    handler: &'handler (dyn Handler<Event = Event> + 'handler),
}

impl<'handler, Event> OutputCollector<'handler, Event> {
    pub fn new(handler: &'handler (dyn Handler<Event = Event> + 'handler)) -> Self {
        Self {
            output: None,
            handler,
        }
    }
}
impl<Event> Write<Event> for OutputCollector<'_, Event> {
    fn write(&mut self, value: ContextValue) {
        self.output = Some(value);
    }
    fn notify(&self, event: Event) {
        self.handler.handle(&event);
    }
}
