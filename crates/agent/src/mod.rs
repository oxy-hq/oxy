//! Agent Slice
//!
//! This slice handles all agent-related functionality including:
//! - Agent execution (one-shot and agentic workflows)
//! - OpenAI integration
//! - Tool calling and function execution
//! - State management for multi-step workflows
//!
//! # Architecture
//!
//! - `domain/` - Agent entities and business logic
//! - `use_cases/` - Application services (execute agent, launch workflow)
//! - `infrastructure/` - Technical implementations (contexts, databases, references)

pub mod domain;
pub mod infrastructure;
pub mod use_cases;

// Re-export main types for convenience
pub use domain::{AgentExecutable, AgentInput, OneShotInput, OpenAIExecutableResponse};
pub use infrastructure::AgentReferencesHandler;
pub use use_cases::{AgentLauncher, AgentLauncherExecutable};
