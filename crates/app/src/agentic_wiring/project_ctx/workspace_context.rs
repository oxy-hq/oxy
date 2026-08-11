//! [`WorkspaceContext`] implementation for [`OxyProjectContext`].

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use agentic_automation::workspace::IntegrationConfig;
use agentic_automation::{ContextRoot, WorkspaceContext};
use agentic_connector::DatabaseConnector;
use async_trait::async_trait;
use oxy::config::model::IntegrationType;

use super::{OxyProjectContext, resolve_workspace_relative};

impl OxyProjectContext {
    /// Branch to hand `compiled_reader` as its `branch_hint`.
    ///
    /// `Some(branch)` only on a role that OWNS a working copy (`Ide` / `All`):
    /// there a non-default branch is a draft whose edits were never compiled,
    /// and `open_compiled_revision`'s branch gate turns that hint into "read
    /// the FS" — which is what keeps the IDE's edit-then-run loop working.
    ///
    /// `None` on the stateless roles (`Serve` / `Worker`): they have no working
    /// copy and therefore no branch, and must read the promoted revision. Also
    /// `None` when git can't answer (not a repo, working copy absent) — the
    /// boundary is the only thing left to try.
    async fn working_copy_branch(&self) -> Option<String> {
        use crate::server::role_manifest::{Role, current_process_role};
        use oxy_git::GitClient;

        if matches!(current_process_role(), Role::Serve | Role::Worker) {
            return None;
        }
        oxy::github::default_git_client()
            .get_current_branch(self.workspace_path())
            .await
            .ok()
    }
}

#[async_trait]
impl WorkspaceContext for OxyProjectContext {
    fn workspace_path(&self) -> &Path {
        self.workspace_manager.config_manager.workspace_path()
    }

    /// On the stateless fleet roles (`Serve` and `Worker`) there's no working
    /// copy, so resolve an agent's `context:` globs from the compile boundary
    /// (materialised into a tempdir) instead of globbing an absent filesystem —
    /// otherwise context resolution finds nothing and the run fails "no
    /// databases configured". `Ide` / `All` keep the FS path (they hold the
    /// working copy), so context the boundary doesn't serve yet (verified
    /// `.sql`, `.md`, automations) isn't lost there.
    async fn context_root(&self) -> ContextRoot {
        use crate::server::role_manifest::{Role, current_process_role};
        if matches!(current_process_role(), Role::Serve | Role::Worker) {
            let workspace_id = self.workspace_manager.workspace_id;
            match crate::server::api::semantic_scan::materialise_agent_context(workspace_id).await {
                Ok(Some(materialised)) => {
                    let root = materialised.root.clone();
                    return ContextRoot::materialised(root, Box::new(materialised));
                }
                // On a stateless node the FS fall-through below is doomed (no
                // working copy), so make the miss LOUD and specific rather than
                // letting it surface later as a confusing "no databases
                // configured". Control flow is unchanged — we still fall
                // through, which stays correct for Ide/All and any edge case.
                Ok(None) => {
                    tracing::error!(
                        workspace_id = %workspace_id,
                        role = ?current_process_role(),
                        "context_root: no compiled agent context on a stateless node — workspace not promoted / not yet compiled; the run will have no usable context. Recompile the workspace."
                    );
                }
                Err(e) => {
                    tracing::error!(
                        workspace_id = %workspace_id,
                        error = ?e,
                        "context_root: compile-boundary materialise failed on a stateless node; the run will fall through to an absent FS and fail"
                    );
                }
            }
        }
        ContextRoot::fs(self.workspace_path().to_path_buf())
    }

    fn refresh_key_cache(
        &self,
    ) -> Option<Arc<RwLock<agentic_semantic::refresh_key_cache::RefreshKeyCache>>> {
        self.preagg_cache.clone()
    }

    fn preagg_renewal_threshold_secs(&self) -> u64 {
        self.preagg_renewal_threshold_secs
    }

    fn database_configs(&self) -> Vec<airlayer::DatabaseConfig> {
        self.workspace_manager
            .config_manager
            .list_databases()
            .iter()
            .map(|db| airlayer::DatabaseConfig {
                name: db.name.clone(),
                db_type: db.database_type.to_string(),
            })
            .collect()
    }

    async fn get_connector(&self, name: &str) -> Result<Arc<dyn DatabaseConnector>, String> {
        self.build_connector_lazy(name).await
    }

    // Forward the `http_request` task's secret read/write to the real secret
    // manager (the `WorkspaceContext` trait defaults to None/Err). Mirrors the
    // pipeline `ProjectContext::resolve_secret`/`persist_secret` impls above so a
    // workflow and a pipeline resolve and rotate the same secret store. (Named
    // `fetch_secret`/`store_secret` to avoid colliding with that trait.)
    async fn fetch_secret(&self, var_name: &str) -> Option<String> {
        match self
            .workspace_manager
            .secrets_manager
            .resolve_secret(var_name)
            .await
        {
            Ok(Some(v)) => return Some(v),
            Ok(None) => {}
            Err(e) => tracing::warn!(
                key_var = %var_name,
                error = %e,
                "secrets_manager.resolve_secret failed; falling back to std::env::var"
            ),
        }
        std::env::var(var_name).ok()
    }

    async fn store_secret(&self, var_name: &str, value: &str) -> Result<(), String> {
        self.workspace_manager
            .secrets_manager
            .upsert_secret(
                var_name,
                value,
                self.subject.unwrap_or_else(uuid::Uuid::nil),
            )
            .await
            .map_err(|e| format!("persist secret `{var_name}`: {e}"))
    }

    async fn get_integration(&self, name: &str) -> Result<IntegrationConfig, String> {
        let integration = self
            .workspace_manager
            .config_manager
            .get_integration_by_name(name)
            .ok_or_else(|| format!("integration '{name}' not found"))?;

        match &integration.integration_type {
            IntegrationType::Omni(omni_cfg) => {
                let api_key = self
                    .workspace_manager
                    .secrets_manager
                    .resolve_secret(&omni_cfg.api_key_var)
                    .await
                    .map_err(|e| format!("failed to resolve omni api_key: {e}"))?
                    .ok_or_else(|| {
                        format!("omni api_key_var '{}' not found", omni_cfg.api_key_var)
                    })?;
                Ok(IntegrationConfig::Omni {
                    base_url: omni_cfg.base_url.clone(),
                    api_key,
                })
            }
            IntegrationType::Looker(looker_cfg) => {
                let client_id = self
                    .workspace_manager
                    .secrets_manager
                    .resolve_secret(&looker_cfg.client_id_var)
                    .await
                    .map_err(|e| format!("failed to resolve looker client_id: {e}"))?
                    .ok_or_else(|| {
                        format!(
                            "looker client_id_var '{}' not found",
                            looker_cfg.client_id_var
                        )
                    })?;
                let client_secret = self
                    .workspace_manager
                    .secrets_manager
                    .resolve_secret(&looker_cfg.client_secret_var)
                    .await
                    .map_err(|e| format!("failed to resolve looker client_secret: {e}"))?
                    .ok_or_else(|| {
                        format!(
                            "looker client_secret_var '{}' not found",
                            looker_cfg.client_secret_var
                        )
                    })?;
                Ok(IntegrationConfig::Looker {
                    base_url: looker_cfg.base_url.clone(),
                    client_id,
                    client_secret,
                })
            }
            // World-model "Apps" integrations (Toast, OpenWeatherMap, BestTime,
            // UniFi) are pure HTTP integrations consumed by the world-model
            // dashboard. They don't participate in the agentic pipeline, so
            // there's no corresponding `IntegrationConfig` variant — surface a
            // clear error if a pipeline ever asks for one by name.
            IntegrationType::Toast(_)
            | IntegrationType::ToastAnalytics(_)
            | IntegrationType::OpenWeatherMap(_)
            | IntegrationType::BestTime(_)
            | IntegrationType::Unifi(_) => Err(format!(
                "integration '{name}' is a world-model app integration and is not exposed to the agentic pipeline"
            )),
        }
    }

    async fn list_automation_files(&self) -> Result<Vec<PathBuf>, String> {
        // Boundary-first (mirrors `resolve_automation_yaml` just below): a stateless
        // fleet replica has no working copy, so list runnable automations from
        // `procedure_definitions` instead of globbing an absent filesystem —
        // otherwise `/agentic-workflows/files` returns `[]` and the automations
        // sidebar is empty. Fall through to the FS walk on a miss (workspace not
        // promoted / non-default branch). Paths are returned workspace-absolute to
        // preserve the FS walk's contract — the HTTP handler and the subrun runner
        // relativise against `workspace_path()` themselves.
        match crate::server::api::compiled_reader::list_automations(
            self.workspace_manager.workspace_id,
            None,
        )
        .await
        {
            Ok(Some(rows)) => {
                let root = self.workspace_manager.config_manager.workspace_path();
                tracing::debug!(
                    workspace_id = %self.workspace_manager.workspace_id,
                    count = rows.len(),
                    "list_automation_files served from compile boundary"
                );
                return Ok(rows.into_iter().map(|r| root.join(r.file_path)).collect());
            }
            // Workspace not promoted / non-default branch — fall through to FS.
            Ok(None) => {}
            Err(e) => tracing::warn!(
                workspace_id = %self.workspace_manager.workspace_id,
                error = ?e,
                "compile boundary automation list error; falling through to FS"
            ),
        }
        self.workspace_manager
            .config_manager
            .list_workflows()
            .await
            .map_err(|e| format!("{e}"))
    }

    async fn list_airway_files(&self) -> Result<Vec<PathBuf>, String> {
        self.workspace_manager
            .config_manager
            .list_pipelines()
            .await
            .map_err(|e| format!("{e}"))
    }

    /// Serve a `.airway.yml` body from `airway_pipelines`. `Ok(None)` = "read
    /// the FS", which the caller (`pipeline_ref::load_pipeline_yaml`) then does
    /// under its containment guard; `Err` = "I could not look", which the
    /// caller turns into a retryable `Unavailable` instead of an FS read.
    ///
    /// That last distinction is why this returns a `Result`. Reporting a
    /// lookup error as `Ok(None)` sent the caller to a filesystem that does not
    /// exist on a stateless replica, where the failure resurfaced as
    /// `Invalid("workspace root is not accessible")` — a caller-input shape for
    /// a database blip, and terminal on the automation dispatch path that
    /// retries only transient errors.
    ///
    /// This is what makes an airway run executable on the durable worker
    /// fleet: the executor claims a queued `TaskSpec::Airway` on a stateless
    /// replica with no working copy, so an FS read there is the
    /// instance-affinity failure the compile boundary exists to close.
    ///
    /// The branch hint comes from the working copy when this process HAS one,
    /// so `open_compiled_revision`'s existing gate routes a draft branch back
    /// to the filesystem and the IDE's edit-then-run loop is unchanged.
    async fn resolve_pipeline_yaml(&self, pipeline_ref: &str) -> Result<Option<String>, String> {
        let branch = self.working_copy_branch().await;
        match crate::server::api::compiled_reader::resolve_pipeline(
            self.workspace_manager.workspace_id,
            branch.as_deref(),
            pipeline_ref,
        )
        .await
        {
            // Round-trip JSONB → YAML so the downstream parser (which renders
            // `variables` with minijinja and then deserialises) is unchanged.
            Ok(Some(artifact)) => match serde_yaml::to_string(&artifact.definition) {
                Ok(yaml) => {
                    tracing::debug!(
                        workspace_id = %self.workspace_manager.workspace_id,
                        pipeline_ref,
                        "resolve_pipeline_yaml served from compile boundary"
                    );
                    Ok(Some(yaml))
                }
                // `Ok(None)`, not `Err`: the row was found and is unusable, so
                // this is a content problem with that revision rather than a
                // question we failed to ask. A host with a working copy has a
                // legitimately better answer, and on a host without one the
                // caller's own no-working-copy branch reports it as
                // `Unavailable` anyway.
                Err(e) => {
                    tracing::warn!(
                        workspace_id = %self.workspace_manager.workspace_id,
                        pipeline_ref,
                        error = ?e,
                        "compile boundary pipeline YAML re-serialise failed; falling through to FS"
                    );
                    Ok(None)
                }
            },
            // Draft branch, workspace not promoted, local workspace, or no
            // matching row — fall through to FS.
            Ok(None) => Ok(None),
            // The lookup itself failed. Propagated rather than laundered into
            // `Ok(None)`: this is "unknown", and the caller must not read it as
            // "not compiled here" and go to a filesystem this node may not have.
            Err(e) => {
                tracing::warn!(
                    workspace_id = %self.workspace_manager.workspace_id,
                    pipeline_ref,
                    error = ?e,
                    "compile boundary pipeline lookup error; reporting unavailable"
                );
                Err(e.to_string())
            }
        }
    }

    async fn resolve_automation_yaml(&self, workflow_ref: &str) -> Result<String, String> {
        // Serve the automation YAML from `procedure_definitions`; falls through
        // to the filesystem read below on any miss. Round-trips JSONB →
        // strict-typed Workflow → YAML so the downstream parser (which expects
        // YAML) is unchanged.
        match crate::server::api::compiled_reader::resolve_automation(
            self.workspace_manager.workspace_id,
            None,
            workflow_ref,
        )
        .await
        {
            Ok(Some(artifact)) => match serde_yaml::to_string(&artifact.definition) {
                Ok(yaml) => {
                    tracing::debug!(
                        workspace_id = %self.workspace_manager.workspace_id,
                        workflow_ref,
                        "resolve_automation_yaml served from compile boundary"
                    );
                    return Ok(yaml);
                }
                Err(e) => tracing::warn!(
                    workspace_id = %self.workspace_manager.workspace_id,
                    workflow_ref,
                    error = ?e,
                    "compile boundary automation YAML re-serialise failed; falling through to FS"
                ),
            },
            Ok(None) => {
                // Branch non-default, workspace not promoted, or no matching
                // row — fall through to FS.
            }
            Err(e) => tracing::warn!(
                workspace_id = %self.workspace_manager.workspace_id,
                workflow_ref,
                error = ?e,
                "compile boundary automation lookup error; falling through to FS"
            ),
        }

        // Authenticated callers can supply an arbitrary `workflow_ref` via
        // the `path_b64` route param (and the queued workflow spec), so we
        // must contain it to the workspace root before reading. A raw
        // `workspace_path().join(workflow_ref)` would happily resolve
        // `../../etc/passwd` or replace the prefix entirely with an
        // absolute path — `ConfigManager::resolve_file` runs
        // `validate_path_within_project` which canonicalises and rejects
        // anything outside the workspace.
        let resolved = resolve_workspace_relative(&self.workspace_manager, workflow_ref).await?;
        std::fs::read_to_string(&resolved)
            .map_err(|e| format!("failed to read workflow {workflow_ref:?}: {e}"))
    }
}
