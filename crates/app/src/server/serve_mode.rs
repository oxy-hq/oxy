//! Mode the server is running in. Chosen at startup, immutable thereafter.
//!
//! Local mode uses a conventional nil-UUID for the workspace id. This is safe
//! because `threads.workspace_id`, `runs.workspace_id`, and `secrets.workspace_id`
//! are `NOT NULL DEFAULT '00000000-...'::uuid` with no foreign-key constraint
//! pointing at `workspaces.id` (see migration
//! `m20260108_000001_drop_fk_runs_project_id` and the comment in
//! `m20260304_000001_create_testing_tables.rs`). Inserts from local-mode
//! handlers therefore do not require a real `workspaces` row.

use std::sync::atomic::{AtomicU8, Ordering};

use uuid::Uuid;

/// Conventional workspace id used by local mode. `00000000-0000-0000-0000-000000000000`.
pub const LOCAL_WORKSPACE_ID: Uuid = Uuid::nil();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServeMode {
    Local,
    Cloud,
}

impl ServeMode {
    pub fn is_local(&self) -> bool {
        matches!(self, ServeMode::Local)
    }

    pub fn label(&self) -> &'static str {
        match self {
            ServeMode::Local => "local",
            ServeMode::Cloud => "cloud",
        }
    }
}

/// Process-wide serve mode, captured once at startup by [`set_process_mode`].
/// 0 = unset (`oxy run`, tests), 1 = local, 2 = cloud. A process runs in exactly
/// one mode for its whole life, so a global is the right shape: it lets deep,
/// request-agnostic code read the mode without threading a process constant
/// through every call (e.g. the app email sender defaults to a browser preview
/// in local mode instead of hitting SES).
static PROCESS_MODE: AtomicU8 = AtomicU8::new(0);

/// Record the mode the server started in. Called once from `oxy serve`.
pub fn set_process_mode(mode: ServeMode) {
    PROCESS_MODE.store(
        match mode {
            ServeMode::Local => 1,
            ServeMode::Cloud => 2,
        },
        Ordering::Relaxed,
    );
}

/// Whether this process started in local mode, as captured at startup. `None`
/// before the server sets it (`oxy run`, unit tests).
pub fn process_is_local() -> Option<bool> {
    match PROCESS_MODE.load(Ordering::Relaxed) {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_mode_reports_label_and_is_local() {
        let mode = ServeMode::Local;
        assert!(mode.is_local());
        assert_eq!(mode.label(), "local");
    }

    #[test]
    fn cloud_mode_is_not_local() {
        let mode = ServeMode::Cloud;
        assert!(!mode.is_local());
        assert_eq!(mode.label(), "cloud");
    }

    #[test]
    fn local_workspace_id_is_nil() {
        assert_eq!(LOCAL_WORKSPACE_ID, Uuid::nil());
    }
}
