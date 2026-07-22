//! Transport implementations for the coordinator-worker architecture.

pub mod durable;
pub mod local;

pub use durable::{
    DurableTransport, begin_shutdown, is_shutting_down, process_worker_id,
    process_worker_id_if_initialized, shutdown_signal,
};
pub use local::LocalTransport;
