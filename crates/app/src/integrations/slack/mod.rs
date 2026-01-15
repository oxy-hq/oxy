//! Slack integration module for Oxy
//!
//! This module implements Slack HTTP Events API integration, allowing Oxy to:
//! - Respond to @mentions in Slack channels
//! - Handle slash commands (/oxy)
//! - Manage channel-to-project bindings
//! - Stream Oxy agent responses back to Slack
//!
//! Architecture:
//! - Uses HTTP Events API (NOT Socket Mode)
//! - Verifies Slack request signatures (signing secret loaded from config.yml)
//! - Maps Slack channels to Oxy projects/agents
//! - Reuses existing Oxy chat/prompt APIs and RBAC/filter logic
//!
//! HTTP handlers are in `crate::api::slack`.

pub mod client;
pub mod commands;
pub mod events;
pub mod mrkdwn;
pub mod services;
pub mod signature;
pub mod types;
pub mod utils;
