//! Compile errors.
//!
//! Two layers: per-file failures (one file fails to parse but the rest
//! succeed) and run-level failures (DB unreachable, workspace missing,
//! everything fails). Per-file failures end up in the `revisions`
//! row's `error_summary` JSONB; run-level failures bubble up.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("workspace path does not exist: {0}")]
    WorkspaceNotFound(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("database: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("file walk failed: {0}")]
    Walk(String),

    #[error("internal: {0}")]
    Internal(String),
}
