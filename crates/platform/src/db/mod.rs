//! Database connection: pool initialization, auth-mode dispatch, RDS IAM
//! token refresher, and the task-router listener factory.
//!
//! Public surface is intentionally tiny — most users only need
//! [`establish_connection`] (the main pool) and
//! [`listener_factory_from_env`] (the agentic task router's dedicated
//! LISTEN connection). The [`auth_mode`] and [`iam`] submodules are
//! kept `pub(crate)` because they're implementation details of the
//! connection setup; nothing outside this crate has historically
//! called them directly.

pub(crate) mod auth_mode;
mod client;
pub(crate) mod iam;
mod listener;

pub use client::establish_connection;
pub use listener::listener_factory_from_env;
