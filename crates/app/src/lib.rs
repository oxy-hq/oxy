//! Oxy Application - CLI and HTTP Server
//!
//! This crate provides both command-line interface and HTTP server functionality
//! for Oxy, integrating domain crates (oxy-agent, oxy-auth, oxy-workflow, etc.)

pub mod cli;
pub mod emails;
pub mod integrations;
pub mod server;

// Re-export commonly used items
pub use server::{api, service};
