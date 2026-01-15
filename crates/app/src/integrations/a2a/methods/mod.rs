//! A2A method handler implementations.
//!
//! This module contains the implementation logic for A2A protocol methods.
//! The handlers are split into separate files for better organization:
//!
//! - `message.rs` - Message sending logic (synchronous)
//! - `streaming.rs` - Streaming message logic (SSE)
//! - `agent.rs` - Agent card generation logic

pub mod agent;
pub mod message;
pub mod streaming;
