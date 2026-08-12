//! `revisions` — root table of the compile boundary (Phase 1.6a).
//!
//! One row per compile attempt. A `ready` revision is a fully
//! materialised snapshot of the workspace's YAML/SQL at a particular
//! git SHA. A `failed` revision still has a row so the operator can
//! see what went wrong; per-entity tables only get rows for files
//! that compiled successfully.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "revisions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: Uuid,
    pub workspace_id: Uuid,
    pub git_sha: String,
    pub branch: Option<String>,
    pub schema_version: i32,
    /// `compiling` | `ready` | `failed`. Plain text rather than an
    /// enum so adding states doesn't require a migration.
    pub status: String,
    /// `main` | `draft`. Phase 1.6a only writes `main`; drafts arrive
    /// in Phase 1.6e (IDE light-mode).
    pub kind: String,
    /// Owner of a draft revision; NULL for `main` revisions.
    pub owner_user_id: Option<Uuid>,
    /// Identifies the compiler binary that produced this revision so
    /// operator dashboards can spot version-skew across rolling
    /// deploys. Matches `CARGO_PKG_VERSION` of the `oxy` binary.
    pub compiler_version: String,
    pub started_at: DateTimeWithTimeZone,
    pub finished_at: Option<DateTimeWithTimeZone>,
    pub file_count_seen: i32,
    pub file_count_compiled: i32,
    pub file_count_failed: i32,
    /// Structured per-file failure report; NULL on success.
    pub error_summary: Option<Json>,
    #[sea_orm(
        belongs_to,
        from = "workspace_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    #[serde(skip)]
    pub workspaces: BelongsTo<super::workspaces::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
