//! Host impl of [`agentic_pipeline::platform::CompileDispatcher`].
//!
//! Owns the bridge between the runtime executor's `TaskSpec::Compile` arm
//! (which lives in agentic-pipeline, no entity dep) and the actual compile
//! worker (which lives in the host, calls `oxy_compile::*` + `entity`).
//!
//! The worker resolves the workspace path from the DB rather than from the
//! pipeline's bound `PlatformContext`: compile tasks are claimed `Global` by
//! any worker, so the bound platform's workspace would silently override the
//! compile's intended target.

use std::sync::Arc;

use agentic_pipeline::platform::CompileDispatcher;
use agentic_runtime::worker::ExecutingTask;
use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::server::compile_worker;

pub struct OxyCompileDispatcher {
    db: Arc<DatabaseConnection>,
}

impl OxyCompileDispatcher {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CompileDispatcher for OxyCompileDispatcher {
    async fn dispatch(
        &self,
        workspace_id: Uuid,
        git_sha: Option<String>,
        branch: Option<String>,
        promote: bool,
        kind: Option<String>,
        owner_user_id: Option<Uuid>,
    ) -> Result<ExecutingTask, String> {
        let workspace_path = oxy_compile::resolve_workspace_path(&self.db, workspace_id)
            .await
            .map_err(|e| format!("compile: {e}"))?;
        if !workspace_path.is_dir() {
            return Err(format!(
                "compile: workspace {workspace_id} path {} does not exist on this worker — \
                 compiles run only on a node that already holds the workspace working copy \
                 (the IDE/build singleton with OXY_INPROC_GLOBAL_WORKER=1). Per-worker \
                 clone-on-demand is NOT planned \
                 (see internal-docs/multi-instance-fleet.md).",
                workspace_path.display()
            ));
        }

        let spec = compile_worker::spec_from_taskspec(
            workspace_id,
            workspace_path,
            git_sha,
            branch,
            promote,
            kind.as_deref(),
            owner_user_id,
        )?;
        let worker = compile_worker::CompileWorker::new(self.db.clone());
        Ok(worker.execute(spec))
    }
}
