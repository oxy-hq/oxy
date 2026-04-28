//! CLI command-line interface

pub mod commands;
pub mod render;
pub mod types;

pub use commands::cli;
pub use types::{A2aArgs, ServeArgs, StartArgs};
