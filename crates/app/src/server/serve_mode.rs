//! Mode the server is running in. Chosen at startup, immutable thereafter.
//!
//! Local mode uses a conventional nil-UUID for the workspace id. This is safe
//! because `threads.workspace_id`, `runs.workspace_id`, and `secrets.workspace_id`
//! are `NOT NULL DEFAULT '00000000-...'::uuid` with no foreign-key constraint
//! pointing at `workspaces.id` (see migration
//! `m20260108_000001_drop_fk_runs_project_id` and the comment in
//! `m20260304_000001_create_testing_tables.rs`). Inserts from local-mode
//! handlers therefore do not require a real `workspaces` row.

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
