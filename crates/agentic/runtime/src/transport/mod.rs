//! Transport implementations for the coordinator-worker architecture.

pub mod durable;
pub mod local;

pub use durable::DurableTransport;
pub use local::LocalTransport;
