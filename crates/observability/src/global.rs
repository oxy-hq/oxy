//! Process-global singleton for the active [`ObservabilityStore`].
//!
//! Certain subsystems (metric context finalization, intent classification,
//! telemetry) need access to the store without explicit plumbing. This module
//! holds a `OnceLock<Arc<dyn ObservabilityStore>>` that the application
//! entrypoint sets during startup.

use std::sync::{Arc, OnceLock};

use crate::store::ObservabilityStore;

static GLOBAL_STORE: OnceLock<Arc<dyn ObservabilityStore>> = OnceLock::new();

/// Register a global ObservabilityStore instance. Subsequent calls are no-ops
/// (first one wins).
pub fn set_global(store: Arc<dyn ObservabilityStore>) {
    let _ = GLOBAL_STORE.set(store);
}

/// Retrieve the global ObservabilityStore, if one has been registered.
pub fn get_global() -> Option<&'static Arc<dyn ObservabilityStore>> {
    GLOBAL_STORE.get()
}
