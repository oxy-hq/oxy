//! HTTP server and API endpoints

pub mod api;
pub mod builder_test_runner;
pub mod default_branch;
pub mod feature_flags;
pub mod preagg_executor;
pub(super) mod preagg_rebuild;
pub mod preagg_worker;
pub mod role_manifest;
pub mod role_middleware;
pub mod router;
pub mod serve_mode;
pub mod service;
pub mod worker_health;
pub mod worker_metrics;
pub mod worker_runtime;

pub use router::{AppState, WorkspaceExtractor, api_router, openapi_router};
