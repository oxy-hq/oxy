//! The operating graph below the HTTP layer: a caller's reach, binding a semantic
//! query to it, the request and row types, and the reads other contexts ask.
//! The handlers stay in `oxy-app` (`server::api::operating_graph`).

pub mod binding;
pub mod dto;
pub mod reach;
pub mod readers;
