//! Storage adapters (port implementations).

pub mod in_memory;
pub mod json_file;

pub use in_memory::InMemoryStorage;
pub use json_file::JsonFileStorage;
