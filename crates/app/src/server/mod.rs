//! HTTP server and API endpoints

pub mod api;
pub mod router;
pub mod service;

pub use router::{AppState, ProjectExtractor, api_router, openapi_router};
