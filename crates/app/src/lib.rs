//! Oxy Application - CLI and HTTP Server
//!
//! This crate provides both command-line interface and HTTP server functionality
//! for Oxy, integrating domain crates (oxy-auth, agentic-*, oxy-workflow, etc.)

// The deeply-nested async futures in the HTTP layer (Axum handlers + the
// customer-app function host's chained `with_db_timeout`/connector futures)
// exceed the default type-layout query depth of 128. rustc's own suggestion.
#![recursion_limit = "256"]

pub mod agentic_wiring;
pub mod cli;
pub mod customer_app_template;
pub mod emails;
pub mod integrations;
pub mod observability_boot;
pub mod observability_setup;
pub mod server;

// Re-export commonly used items
pub use server::{api, service};
