use std::sync::atomic::{AtomicBool, Ordering};

// Global readonly mode state
static READONLY_MODE: AtomicBool = AtomicBool::new(false);

/// Set the global readonly mode
pub fn set_readonly_mode(readonly: bool) {
    READONLY_MODE.store(readonly, Ordering::Relaxed);
}

/// Check if the application is in readonly mode
pub fn is_readonly_mode() -> bool {
    READONLY_MODE.load(Ordering::Relaxed)
}
