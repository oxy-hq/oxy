//! Re-exports of state directory helpers from `oxy-shared`.
//!
//! The implementation lives in `oxy_shared::state_dir` so that crates which
//! cannot depend on `oxy` (e.g. `agentic-workflow`) can still resolve the
//! same state and cache directories.
pub use oxy_shared::state_dir::{
    airlayer_cache_key, get_airlayer_cache_dir, get_state_dir, resolve_state_dir_with_fallback,
    state_dir_path,
};
