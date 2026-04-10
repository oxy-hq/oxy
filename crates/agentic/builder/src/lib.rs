//! Built-in builder copilot agent.
//!
//! A single-state LLM tool loop that lets an LLM read and propose changes to
//! project files, using the same streaming/HITL infrastructure as `agentic-http`.

pub mod events;
pub mod solver;
pub mod test_runner;
pub mod tools;
pub mod types;
pub mod ui;

pub use events::BuilderEvent;
pub use solver::{BuilderSolver, build_builder_handlers};
pub use test_runner::BuilderTestRunner;
pub use types::{
    BuilderAnswer, BuilderDomain, BuilderError, BuilderIntent, BuilderResult, BuilderSolution,
    BuilderSpec, ConversationTurn, ToolExchange,
};
pub use ui::{builder_step_summary, builder_tool_summary};
