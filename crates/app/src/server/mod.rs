//! HTTP server and API endpoints

pub mod admission;
pub mod api;
pub mod builder_test_runner;
pub mod compile_config_gate;
pub mod compile_maintenance;
pub mod compile_worker;
pub mod default_branch;
pub mod feature_flags;
pub mod health_eval_executor;
pub mod ide_proxy;
pub mod preagg_executor;
pub(super) mod preagg_rebuild;
pub mod preagg_worker;
pub mod role_manifest;
pub mod role_middleware;
pub mod router;
pub mod runtime_artifact;
pub mod serve_mode;
pub mod serve_safety;
pub mod service;
pub mod worker_health;
pub mod worker_metrics;
pub mod worker_runtime;
pub mod workspace_fs;
pub mod worktree_registry;

pub use router::{AppState, WorkspaceExtractor, api_router, openapi_router};
