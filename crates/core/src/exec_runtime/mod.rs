//! Execution runtime: the live utilities the pre-agentic executor left behind
//! (its dead pipeline was removed across old-executor-retirement Phases 4a/4b).
//! The type vocabulary lives in the sibling `crate::exec_types`.
mod context;
pub mod formatters;
pub mod renderer;
pub mod writer;

pub use context::{ExecutionContext, ExecutionContextBuilder};
