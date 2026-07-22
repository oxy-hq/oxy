//! Multi-tenant Slack integration.
//! See internal-docs/2026-04-21-universal-slack-bot-design.md

pub mod blocks;
pub mod chart_render;
/// `client` was extracted to the `oxy-slack-client` crate (shared with
/// `oxy-cameras`). Re-exported here so every in-crate `client::` path is
/// unchanged. See internal-docs/2026-07-19-oxy-app-crate-extraction-roadmap.md.
pub use oxy_slack_client as client;
pub mod config;
pub mod error;
pub mod events;
pub mod home;
pub mod linking;
pub mod oauth;
pub mod pickers;
pub mod render;
pub mod resolution;
pub mod scopes;
pub mod services;
pub mod signature;
pub mod socket_mode;
pub mod types;
pub mod webhooks;
