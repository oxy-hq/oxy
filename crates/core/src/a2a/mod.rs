//! A2A (Agent-to-Agent) Protocol Integration for Oxy
//!
//! This module provides Oxy-specific implementation of the A2A protocol,
//! enabling external agents to communicate with Oxy agents.
//!
//! # Architecture
//!
//! The A2A implementation is split into two layers:
//!
//! 1. **Protocol Layer** (`a2a` crate): Standalone, Oxy-agnostic implementation
//!    - Core A2A types (Message, Task, AgentCard, etc.)
//!    - Protocol handlers (JSON-RPC, HTTP+JSON)
//!    - Server abstractions (A2aHandler trait, routers)
//!    - Storage abstraction (TaskStorage trait)
//!
//! 2. **Integration Layer** (this module): Oxy-specific implementations
//!    - Configuration loading and validation
//!    - Handler implementation with Oxy agent execution
//!    - Task storage backed by Oxy database
//!    - Multi-agent routing and management
//!
//! # Multi-Agent Support
//!
//! The A2A server can expose multiple Oxy agents as independent A2A agents.
//! Each agent gets its own endpoint:
//!
//! - JSON-RPC: `https://example.com/a2a/agents/{agent_name}/v1/jsonrpc`
//! - HTTP: `https://example.com/a2a/agents/{agent_name}/v1`
//! - Agent Card: `https://example.com/a2a/agents/{agent_name}/.well-known/agent-card.json`
//!
//! # Configuration
//!
//! A2A agents are configured in `config.yml`:
//!
//! ```yaml
//! a2a:
//!   agents:
//!     - ref: agents/sales-assistant.agent.yml
//!       name: sales-assistant
//!     - ref: agents/data-analyst.agent.yml
//!       name: data-analyst
//! ```
//!
//! # Request Flow
//!
//! 1. Client sends request to agent-specific endpoint
//! 2. Core crate routes to handler instance for that agent
//! 3. Handler loads Oxy agent config and executes agent
//! 4. Response is converted to A2A format and returned
//! 5. Task is stored in database with agent name for isolation

pub mod agent_card;
pub mod chat_integration;
pub mod config;
pub mod handler;
pub mod mapper;
pub mod methods;
pub mod storage;

// Re-export key types from a2a crate for convenience
pub use a2a::{
    error::A2aError,
    server::{A2aContext, A2aHandler},
    storage::TaskStorage,
    types::{AgentCard, Artifact, Message, Part, Task, TaskState, TaskStatus},
};

// Re-export config types
pub use config::{A2aAgentConfig, A2aConfig};

// Re-export handler
pub use handler::OxyA2aHandler;

// Re-export agent card service
pub use agent_card::AgentCardService;
