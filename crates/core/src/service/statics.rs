use std::sync::Arc;

use crate::{adapters::runs::Broadcaster, service::types::event::EventKind};

lazy_static::lazy_static! {
    pub static ref BROADCASTER: Arc<Broadcaster<EventKind>> = Arc::new(Broadcaster::new(1024 * 1024, 64));
}
