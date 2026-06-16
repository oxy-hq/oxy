//! `ProjectContext` implementation backed by Oxy's [`WorkspaceManager`].
//!
//! All `oxy::*` imports for connector/model resolution live in this file.
//! This is the concrete adapter — the agentic stack sees only the trait.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use agentic_analytics::config::{LlmVendor, ResolvedModelInfo};
use agentic_connector::{
    BigQueryConfig, ClickHouseConfig, ConnectorConfig, DatabaseConnector, DomoConfig, DuckDbConfig,
    DuckDbLoadStrategy, DuckDbRawConfig, DuckDbUrlConfig, MysqlConfig, PostgresConfig,
    SnowflakeAuth, SnowflakeConfig,
};
use agentic_pipeline::SharedMetricSink;
use agentic_pipeline::platform::{MonitorScanPort, ProjectContext, ResolvedPipelineDestination};
use agentic_workflow::workspace::IntegrationConfig;
use agentic_workflow::{ContextRoot, WorkspaceContext};
use async_trait::async_trait;
use entity::workspace_members::WorkspaceRole;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::config::model::{DatabaseType, DuckDBOptions, IntegrationType, Model, SnowflakeAuthType};
use oxy_shared::errors::OxyError;

/// Adapter that exposes a [`WorkspaceManager`] as a [`ProjectContext`] and
/// (for the workflow runner) an [`agentic_workflow::WorkspaceContext`].
///
/// Built connectors are cached via a [`tokio::sync::OnceCell`] so the workflow
/// runner's `get_connector` calls don't re-resolve/re-build per step. The
/// cache is per-instance; callers that want to share it should wrap in `Arc`
/// before handing the context around.
pub struct OxyProjectContext {
    workspace_manager: WorkspaceManager,
    /// Oxy user id of the requester, when known. Threaded into
    /// `airhouse_managed` credential resolution so each query runs as that
    /// user's provisioned Airhouse role. `None` falls back to single-row
    /// resolution (local-mode only).
    subject: Option<uuid::Uuid>,
    /// Effective workspace role of the requester. Determines the Airhouse
    /// role any minted credential carries (Owner→admin, Admin→writer,
    /// Member/Viewer→reader). `None` outside HTTP-handled paths.
    role: Option<WorkspaceRole>,
    /// Per-database lazy connector cache. Each name maps to its own
    /// `OnceCell` so building a slow connector (Snowflake handshake,
    /// BigQuery auth, …) does not block requests that only need an
    /// unrelated database. Concurrent `get_connector(name)` callers for
    /// the same name share the in-flight build.
    connectors:
        tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::OnceCell<Arc<dyn DatabaseConnector>>>>>,
    /// Shared Layer-1 refresh-key cache. The same Arc is held by the
    /// background preagg worker so both layers observe each other's writes.
    /// `None` in CLI, tests, and builder contexts (no background worker).
    preagg_cache: Option<Arc<RwLock<agentic_semantic::refresh_key_cache::RefreshKeyCache>>>,
    /// Renewal threshold (seconds) for the preagg refresh-key freshness
    /// check on the query read-path. Mirrors the worker's
    /// `pre_aggregations.refresh_worker.renewal_threshold` so operators who
    /// change one see the matching read-side behaviour.
    preagg_renewal_threshold_secs: u64,
    /// Database connection. Set from `AgenticState.db` by the HTTP workspace
    /// middleware AND the in-process global driver (`recovery.rs`). Powers
    /// anomaly-store queries and `compile_dispatcher()` — so any path that
    /// executes `TaskSpec::Compile` (the global driver) MUST set it, or every
    /// compile fails with "compile_dispatcher() returned None". Pure CLI / test
    /// paths may leave it unset; anomaly tools + compile are then disabled.
    db: Option<Arc<sea_orm::DatabaseConnection>>,
}

impl OxyProjectContext {
    pub fn new(workspace_manager: WorkspaceManager) -> Self {
        Self {
            workspace_manager,
            subject: None,
            role: None,
            connectors: tokio::sync::Mutex::new(HashMap::new()),
            preagg_cache: None,
            preagg_renewal_threshold_secs: 120,
            db: None,
        }
    }

    /// Attach the database connection. The HTTP workspace middleware and the
    /// in-process global driver (`recovery.rs`) set this from `AgenticState.db`
    /// so anomaly tools can query `metric_anomalies` and `compile_dispatcher()`
    /// can drive `TaskSpec::Compile`. Pure CLI / test paths leave it unset.
    pub fn with_db(mut self, db: Arc<sea_orm::DatabaseConnection>) -> Self {
        self.db = Some(db);
        self
    }

    /// Attach the requesting user's id. The HTTP workspace middleware sets
    /// this from `AuthenticatedUserExtractor`; background paths (scheduler,
    /// schema crawl) leave it unset.
    pub fn with_subject(mut self, subject: uuid::Uuid) -> Self {
        self.subject = Some(subject);
        self
    }

    /// Attach the requesting user's effective workspace role. The HTTP
    /// workspace middleware sets this from `EffectiveWorkspaceRole`;
    /// background paths leave it unset (the airhouse path then mints with
    /// admin via `mint_for_system`).
    pub fn with_role(mut self, role: WorkspaceRole) -> Self {
        self.role = Some(role);
        self
    }

    /// Attach the shared preagg refresh-key cache. Both the HTTP middleware
    /// and the background worker must use the same Arc so Layer 1 and Layer 2
    /// observe each other's inserts and invalidations.
    pub fn with_preagg_cache(
        mut self,
        cache: Arc<RwLock<agentic_semantic::refresh_key_cache::RefreshKeyCache>>,
    ) -> Self {
        self.preagg_cache = Some(cache);
        self
    }

    /// Attach the preagg renewal threshold (seconds). Must match the
    /// background worker's `refresh_worker.renewal_threshold`.
    pub fn with_preagg_renewal_threshold_secs(mut self, secs: u64) -> Self {
        self.preagg_renewal_threshold_secs = secs;
        self
    }

    pub fn workspace_manager(&self) -> &WorkspaceManager {
        &self.workspace_manager
    }

    pub fn subject(&self) -> Option<uuid::Uuid> {
        self.subject
    }

    pub fn role(&self) -> Option<WorkspaceRole> {
        self.role.clone()
    }

    /// True if `name` resolves to a database configured in this workspace
    /// (including runtime-overlay databases registered after a run).
    pub fn is_database_configured(&self, name: &str) -> bool {
        // `resolve_database`'s only error is the not-found case (a poisoned
        // overlay lock falls back to the static-config lookup rather than
        // erroring), so `is_ok()` here cleanly means "configured".
        self.workspace_manager
            .config_manager
            .resolve_database(name)
            .is_ok()
    }

    /// Resolve and build a `DatabaseConnector` for `db_name`, surfacing the
    /// real reason on failure (config missing, secret unresolved, host
    /// unreachable, …) instead of collapsing every failure into `None`.
    ///
    /// This is the API non-agentic callers (e.g. the Dev Portal SQL IDE)
    /// should reach for. Agentic-pipeline internals stay on
    /// `agentic_pipeline::platform::resolve_connectors`, which uses the
    /// `ProjectContext` trait's `Option`-returning methods and silently
    /// skips databases that fail to resolve — fine for batch resolution
    /// where individual failures shouldn't kill the whole pipeline, but
    /// useless when a user typed a SQL query against a specific database
    /// and wants to know why it didn't work.
    ///
    /// Dispatches the same way `resolve_pre_built_airhouse` +
    /// `database_to_connector_config` would (so the routing stays in one
    /// place), but propagates errors through `OxyError` instead of
    /// dropping them on the floor.
    pub async fn build_connector_for(
        &self,
        db_name: &str,
    ) -> Result<Arc<dyn DatabaseConnector>, OxyError> {
        let db = self
            .workspace_manager
            .config_manager
            .resolve_database(db_name)
            .map_err(|e| {
                OxyError::ConfigurationError(format!(
                    "database '{db_name}' not found in config.yml: {e}"
                ))
            })?;

        // Airhouse types live outside `agentic-connector`'s vendor-neutral
        // dispatcher; the helper builds them directly with full error
        // propagation. Other types go through the config + dispatcher path.
        match &db.database_type {
            DatabaseType::Airhouse(_) | DatabaseType::AirhouseManaged(_) => {
                build_airhouse_connector(
                    &db,
                    &self.workspace_manager,
                    self.subject,
                    self.role.clone(),
                )
                .await
            }
            _ => {
                let cfg = database_to_connector_config(&db, &self.workspace_manager)
                    .await
                    .ok_or_else(|| {
                        OxyError::ConfigurationError(format!(
                            "could not build connector config for database '{db_name}' (type {})",
                            db.database_type_name()
                        ))
                    })?;
                let built = agentic_connector::build_connector_async(cfg)
                    .await
                    .map_err(|e| {
                        OxyError::DBError(format!("failed to build connector for '{db_name}': {e}"))
                    })?;
                Ok(Arc::from(built))
            }
        }
    }

    /// Build (and cache) a single connector by name. Other databases in
    /// the workspace config are not touched, so a slow Snowflake handshake
    /// no longer delays a request that only needs `local` (DuckDB).
    async fn build_connector_lazy(&self, name: &str) -> Result<Arc<dyn DatabaseConnector>, String> {
        let cell = {
            let mut guard = self.connectors.lock().await;
            guard
                .entry(name.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::OnceCell::new()))
                .clone()
        };

        cell.get_or_try_init(|| async {
            if let Some(conn) = self.resolve_pre_built_connector(name).await {
                return Ok(conn);
            }
            let cfg = self
                .resolve_connector(name)
                .await
                .ok_or_else(|| format!("database '{name}' not found in workspace config"))?;
            agentic_connector::build_connector_async(cfg)
                .await
                .map(Arc::from)
                .map_err(|e| {
                    tracing::warn!(
                        target: "workspace_context",
                        db = %name,
                        error = %e,
                        "failed to build connector"
                    );
                    format!("failed to build connector for '{name}': {e}")
                })
        })
        .await
        .cloned()
    }
}

#[async_trait]
impl ProjectContext for OxyProjectContext {
    fn workspace_id(&self) -> uuid::Uuid {
        self.workspace_manager.workspace_id
    }

    /// Project timezone from `config.yml`, parsed to a [`chrono_tz::Tz`].
    ///
    /// Deliberately lenient: an unrecognized IANA name warns and falls back to
    /// UTC rather than failing. `config.yml` is project-wide, so a typo here
    /// should not take down every agent and the server. (The per-agent
    /// `.agentic.yml` `timezone:` is validated strictly instead — a bad value
    /// fails just that agent at build time via `ConfigError::InvalidTimezone`.)
    fn timezone(&self) -> Option<chrono_tz::Tz> {
        let raw = self.workspace_manager.config_manager.timezone()?;
        match raw.parse::<chrono_tz::Tz>() {
            Ok(tz) => Some(tz),
            Err(_) => {
                tracing::warn!(
                    timezone = raw,
                    "config.yml `timezone:` is not a valid IANA timezone name \
                     (expected e.g. `America/Los_Angeles`); falling back to UTC. \
                     Relative dates like \"yesterday\" may resolve a day off until \
                     the value is corrected in config.yml."
                );
                None
            }
        }
    }

    async fn resolve_connector(&self, db_name: &str) -> Option<ConnectorConfig> {
        resolve_connector_impl(db_name, &self.workspace_manager).await
    }

    async fn resolve_pre_built_connector(
        &self,
        db_name: &str,
    ) -> Option<Arc<dyn DatabaseConnector>> {
        resolve_pre_built_airhouse(
            db_name,
            &self.workspace_manager,
            self.subject,
            self.role.clone(),
        )
        .await
    }

    async fn resolve_pipeline_destination(
        &self,
        db_name: &str,
    ) -> Option<ResolvedPipelineDestination> {
        let db = self
            .workspace_manager
            .config_manager
            .resolve_database(db_name)
            .ok()?;
        match &db.database_type {
            // Postgres/Redshift: `resolve_connector` already substituted
            // secrets into the connection params.
            DatabaseType::Postgres(_) | DatabaseType::Redshift(_) => {
                match self.resolve_connector(db_name).await? {
                    ConnectorConfig::Postgres(c) | ConnectorConfig::Redshift(c) => {
                        Some(ResolvedPipelineDestination {
                            kind: "postgres".to_string(),
                            connection_string: pg_wire_dsn(
                                &c.user,
                                &c.password,
                                &c.host,
                                c.port,
                                &c.database,
                            ),
                        })
                    }
                    _ => None,
                }
            }
            // Airhouse speaks the Postgres wire protocol; `airhouse_managed`
            // mints an ephemeral per-subject credential here.
            DatabaseType::Airhouse(_) | DatabaseType::AirhouseManaged(_) => {
                match airhouse_wire_params(
                    &db,
                    &self.workspace_manager,
                    self.subject,
                    self.role.clone(),
                )
                .await
                {
                    Ok((host, port, user, password, database)) => {
                        Some(ResolvedPipelineDestination {
                            kind: "airhouse".to_string(),
                            connection_string: pg_wire_dsn(
                                &user, &password, &host, port, &database,
                            ),
                        })
                    }
                    Err(e) => {
                        tracing::warn!(
                            db = %db_name,
                            "airway airhouse destination resolve failed: {e}"
                        );
                        None
                    }
                }
            }
            _ => None,
        }
    }

    async fn resolve_model(
        &self,
        model_ref: Option<&str>,
        has_explicit_model: bool,
    ) -> Option<ResolvedModelInfo> {
        resolve_model_impl(model_ref, has_explicit_model, &self.workspace_manager).await
    }

    async fn resolve_agent_yaml(&self, agent_id: &str) -> Option<String> {
        // Serve the agent YAML from `agent_definitions`; the reader returns
        // None (→ filesystem read) on any miss.
        //
        // `agent_id` may be a path-like reference (`agents/foo.agentic.yml`)
        // or a bare stem (`foo`); `agent_definitions` is keyed by the
        // strict-typed `AgenticAgent.name` (stem), so we normalise.
        let name = agent_id
            .trim_end_matches(".agentic.yml")
            .trim_end_matches(".agentic.yaml")
            .rsplit('/')
            .next()
            .unwrap_or(agent_id);
        match crate::server::api::compiled_reader::resolve_analytics_agent(
            self.workspace_manager.workspace_id,
            None,
            name,
        )
        .await
        {
            Ok(Some(artifact)) => match serde_yaml::to_string(&artifact.definition) {
                Ok(yaml) => {
                    tracing::debug!(
                        workspace_id = %self.workspace_manager.workspace_id,
                        agent_id,
                        "resolve_agent_yaml served from compile boundary"
                    );
                    Some(yaml)
                }
                Err(e) => {
                    tracing::warn!(
                        workspace_id = %self.workspace_manager.workspace_id,
                        agent_id,
                        error = ?e,
                        "compile boundary agent YAML re-serialise failed; falling through to FS"
                    );
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(
                    workspace_id = %self.workspace_manager.workspace_id,
                    agent_id,
                    error = ?e,
                    "compile boundary agent lookup error; falling through to FS"
                );
                None
            }
        }
    }

    async fn resolve_secret(&self, var_name: &str) -> Option<String> {
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

    async fn persist_secret(&self, var_name: &str, value: &str) -> Result<(), String> {
        // Atomic upsert: a single UPDATE when the secret exists (no
        // remove/recreate window), create when it doesn't. On a fresh
        // env-only token this writes a DB secret that then takes
        // precedence (DB-first fallback storage).
        self.workspace_manager
            .secrets_manager
            .upsert_secret(var_name, value, uuid::Uuid::nil())
            .await
            .map_err(|e| format!("persist secret `{var_name}`: {e}"))
    }

    fn metric_sink(&self) -> Option<SharedMetricSink> {
        // Only hand back a sink when an observability store is actually
        // registered. Non-enterprise runs leave `get_global()` as `None`
        // and the adapter's first call would just log-and-skip anyway —
        // returning `None` here keeps the pipeline hot path free of the
        // atomic load + tracing::warn on every query.
        oxy_observability::global::get_global()?;
        Some(Arc::new(super::metric_sink::OxyAnalyticsMetricSink::new()))
    }

    fn metric_tree_runner(&self) -> Option<Arc<dyn agentic_analytics::MetricTreeRunner>> {
        // Background paths (scheduler, recovery) leave subject + role unset;
        // the metric-tree ops need both to mint Airhouse credentials, so we
        // only expose the runner for HTTP-driven runs that carried them in.
        let user_id = self.subject?;
        let role = self.role.clone()?;
        Some(super::metric_tree_runner::make_runner(
            self.workspace_manager.clone(),
            user_id,
            role,
            self.preagg_cache.clone(),
            self.preagg_renewal_threshold_secs,
        ))
    }

    fn metric_tree_runner_system(&self) -> Option<Arc<dyn agentic_analytics::MetricTreeRunner>> {
        // Cron-driven scans don't have a user — mint with nil UUID + Owner
        // so the Airhouse credential resolves with admin read access.
        // Real per-user scoping is enforced upstream when a human triggers
        // a scan via the HTTP endpoint.
        //
        // THREAT-MODEL: anomaly rows produced here (observed, expected,
        // period_*) are visible to all workspace members including Viewers.
        // If warehouse-level RLS is part of the deployment threat model,
        // consider restricting the inbox payload for non-admin roles or
        // running the system scan with a read-only service account that
        // matches the most restrictive tenant RLS policy.
        Some(super::metric_tree_runner::make_runner(
            self.workspace_manager.clone(),
            uuid::Uuid::nil(),
            WorkspaceRole::Owner,
            None,
            120,
        ))
    }

    fn anomaly_store(&self) -> Option<Arc<dyn agentic_analytics::anomaly_store::AnomalyStore>> {
        let db = self.db.clone()?;
        Some(Arc::new(oxy_metric_monitoring::store::OxyAnomalyStore {
            db,
        }))
    }

    fn as_monitor_scan_port(&self) -> Option<&dyn agentic_pipeline::platform::MonitorScanPort> {
        Some(self)
    }

    fn compile_dispatcher(
        &self,
    ) -> Option<std::sync::Arc<dyn agentic_pipeline::platform::CompileDispatcher>> {
        // The dispatcher only needs the DB; the rest comes from the
        // TaskSpec::Compile payload. Background / CLI paths that leave
        // `db` unset get None and Compile fails with a clear message.
        let db = self.db.clone()?;
        Some(std::sync::Arc::new(
            crate::agentic_wiring::compile_dispatcher::OxyCompileDispatcher::new(db),
        ))
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
    /// `.sql`, `.md`, procedures) isn't lost there.
    async fn context_root(&self) -> ContextRoot {
        use crate::server::role_manifest::{Role, current_process_role};
        if matches!(current_process_role(), Role::Serve | Role::Worker) {
            let workspace_id = self.workspace_manager.workspace_id;
            match crate::server::api::semantic_scan::materialise_agent_context(workspace_id).await {
                Ok(Some(materialised)) => {
                    let root = materialised.root.clone();
                    return ContextRoot::materialised(root, Box::new(materialised));
                }
                // Not promoted / no semantic rows — fall through to the FS path.
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        workspace_id = %workspace_id,
                        error = ?e,
                        "context_root: compile-boundary materialise failed; falling through to FS"
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
            | IntegrationType::OpenWeatherMap(_)
            | IntegrationType::BestTime(_)
            | IntegrationType::Unifi(_) => Err(format!(
                "integration '{name}' is a world-model app integration and is not exposed to the agentic pipeline"
            )),
        }
    }

    async fn list_workflow_files(&self) -> Result<Vec<PathBuf>, String> {
        // Boundary-first (mirrors `resolve_workflow_yaml` just below): a stateless
        // fleet replica has no working copy, so list runnable procedures from
        // `procedure_definitions` instead of globbing an absent filesystem —
        // otherwise `/agentic-workflows/files` returns `[]` and the procedures
        // sidebar is empty. Fall through to the FS walk on a miss (workspace not
        // promoted / non-default branch). Paths are returned workspace-absolute to
        // preserve the FS walk's contract — the HTTP handler and the subrun runner
        // relativise against `workspace_path()` themselves.
        match crate::server::api::compiled_reader::list_procedures(
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
                    "list_workflow_files served from compile boundary"
                );
                return Ok(rows.into_iter().map(|r| root.join(r.file_path)).collect());
            }
            // Workspace not promoted / non-default branch — fall through to FS.
            Ok(None) => {}
            Err(e) => tracing::warn!(
                workspace_id = %self.workspace_manager.workspace_id,
                error = ?e,
                "compile boundary procedure list error; falling through to FS"
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

    async fn resolve_workflow_yaml(&self, workflow_ref: &str) -> Result<String, String> {
        // Serve the procedure YAML from `procedure_definitions`; falls through
        // to the filesystem read below on any miss. Round-trips JSONB →
        // strict-typed Workflow → YAML so the downstream parser (which expects
        // YAML) is unchanged.
        match crate::server::api::compiled_reader::resolve_procedure(
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
                        "resolve_workflow_yaml served from compile boundary"
                    );
                    return Ok(yaml);
                }
                Err(e) => tracing::warn!(
                    workspace_id = %self.workspace_manager.workspace_id,
                    workflow_ref,
                    error = ?e,
                    "compile boundary procedure YAML re-serialise failed; falling through to FS"
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
                "compile boundary procedure lookup error; falling through to FS"
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

#[async_trait]
impl MonitorScanPort for OxyProjectContext {
    async fn run_monitor_scan(
        &self,
        db: &sea_orm::DatabaseConnection,
        workspace_id: uuid::Uuid,
        granularity: &str,
    ) -> Result<String, String> {
        use oxy_metric_monitoring::Granularity;

        let runner = self
            .metric_tree_runner_system()
            .ok_or_else(|| "no metric tree runner available".to_string())?;
        let gran = match granularity {
            "day" => Granularity::Day,
            "week" => Granularity::Week,
            "month" => Granularity::Month,
            other => return Err(format!("unknown granularity: {other:?}")),
        };
        let config_path = oxy_metric_monitoring::default_config_path(self.workspace_path());
        let scan = oxy_metric_monitoring::scan_workspace(
            runner,
            &config_path,
            chrono::Utc::now(),
            Some(gran),
        )
        .await
        .map_err(|e| e.to_string())?;
        let persisted = oxy_metric_monitoring::upsert_anomalies(db, workspace_id, &scan)
            .await
            .map_err(|e| e.to_string())?;
        let summary = format!(
            "scanned={} failed={} persisted={}",
            scan.outcomes.len(),
            scan.failures.len(),
            persisted
        );
        if scan.failures.is_empty() {
            Ok(summary)
        } else {
            Err(summary)
        }
    }
}

/// Validate that `workflow_ref` is a single relative path that stays under
/// the workspace root, then return the absolute path to read from.
///
/// Rejects: absolute paths, parent-dir traversal, empty refs. Errors do
/// **not** quote the resolved absolute path — only the caller-supplied
/// `workflow_ref` — so a failed traversal attempt cannot be used to probe
/// workspace layout.
async fn resolve_workspace_relative(
    workspace_manager: &WorkspaceManager,
    workflow_ref: &str,
) -> Result<PathBuf, String> {
    validate_workflow_ref_syntax(workflow_ref)?;
    let resolved = workspace_manager
        .config_manager
        .resolve_file(workflow_ref)
        .await
        .map_err(|e| format!("invalid workflow_ref {workflow_ref:?}: {e}"))?;
    Ok(PathBuf::from(resolved))
}

/// Cheap syntactic reject for obviously-unsafe `workflow_ref`s. The
/// canonicalisation-based check in `validate_path_within_project` would
/// catch absolute paths and `..` traversal too, but its error messages
/// vary by OS and by whether the path happens to exist; failing here
/// keeps the diagnostics stable and avoids touching the filesystem at all
/// for the easy cases.
fn validate_workflow_ref_syntax(workflow_ref: &str) -> Result<(), String> {
    if workflow_ref.is_empty() {
        return Err("workflow_ref is empty".to_string());
    }
    let candidate = Path::new(workflow_ref);
    if candidate.is_absolute() {
        return Err(format!(
            "workflow_ref {workflow_ref:?} must be relative to the workspace"
        ));
    }
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!(
            "workflow_ref {workflow_ref:?} must not contain `..` segments"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_workflow_ref_syntax;

    #[test]
    fn rejects_empty_ref() {
        assert!(validate_workflow_ref_syntax("").is_err());
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        for ref_str in [
            "../etc/passwd",
            "workflows/../../../etc/passwd",
            "..",
            "a/../b",
        ] {
            assert!(
                validate_workflow_ref_syntax(ref_str).is_err(),
                "should reject {ref_str:?}"
            );
        }
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(validate_workflow_ref_syntax("/etc/passwd").is_err());
    }

    #[test]
    fn accepts_relative_ref() {
        for ref_str in [
            "foo.workflow.yml",
            "workflows/foo.workflow.yml",
            "./foo.workflow.yml",
        ] {
            assert!(
                validate_workflow_ref_syntax(ref_str).is_ok(),
                "should accept {ref_str:?}"
            );
        }
    }

    /// Regression guard for the compile-boundary global-driver bug: a
    /// `TaskSpec::Compile` runs through an `OxyProjectContext`, and
    /// `compile_dispatcher()` only resolves when `db` is wired. The HTTP
    /// workspace middleware sets it; the in-process global driver
    /// (`server::router::recovery`) once did NOT, so every queued compile
    /// failed with "compile_dispatcher() returned None". This asserts the
    /// invariant both ways so a future refactor can't silently drop `db`
    /// from a compile-executing path again.
    #[tokio::test]
    async fn compile_dispatcher_resolves_only_when_db_is_wired() {
        use agentic_pipeline::platform::ProjectContext;
        use oxy::adapters::workspace::builder::WorkspaceBuilder;
        use oxy_test_utils::fixtures::TestFixture;
        use std::sync::Arc;

        let fixture = TestFixture::new().expect("tempdir");
        // Same builder path the global driver uses — falls back to a default
        // config, so an empty workspace dir is enough for this unit test.
        let wm = WorkspaceBuilder::new(uuid::Uuid::new_v4())
            .with_workspace_path_and_fallback_config(fixture.path())
            .await
            .expect("config builder")
            .build()
            .await
            .expect("workspace manager");

        // No db (the bug): the global driver would get None and the compile
        // executor fails before dispatching.
        let ctx_no_db = super::OxyProjectContext::new(wm.clone());
        assert!(
            ctx_no_db.compile_dispatcher().is_none(),
            "a db-less context must not resolve a compile dispatcher"
        );

        // db wired (the fix): the dispatcher resolves so compiles can run.
        // `Disconnected` is fine — the dispatcher only stores the handle here,
        // it never opens a connection.
        let ctx_db = super::OxyProjectContext::new(wm)
            .with_db(Arc::new(sea_orm::DatabaseConnection::Disconnected));
        assert!(
            ctx_db.compile_dispatcher().is_some(),
            "a db-wired context (HTTP middleware or global driver) must resolve a compile dispatcher"
        );
    }
}

// ── Connector translation ───────────────────────────────────────────────────

async fn resolve_connector_impl(
    db_name: &str,
    workspace_manager: &WorkspaceManager,
) -> Option<ConnectorConfig> {
    let db = match workspace_manager.config_manager.resolve_database(db_name) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                db = %db_name,
                "databases: '{}' not found in config.yml: {}",
                db_name,
                e
            );
            return None;
        }
    };
    database_to_connector_config(&db, workspace_manager).await
}

/// Translate an already-resolved [`oxy::config::model::Database`] into an
/// [`agentic_connector::ConnectorConfig`].
///
/// This is the secret-resolving counterpart of [`ConnectorConfig`]'s
/// constructors: it reads the per-type host / port / user / password /
/// database_var / developer_token_var values off `workspace_manager`'s
/// `SecretsManager`, and for DuckDB + BigQuery also walks the
/// workspace-relative file paths through `config_manager.resolve_file`.
///
/// Returns `None` for database shapes that [`agentic-connector`] does not yet
/// handle — today, Snowflake browser-auth and Snowflake private-key auth
/// both fall into this bucket. The test-connection handler uses that as a
/// signal to fall back to the legacy `oxy::connector::Connector` path.
pub async fn database_to_connector_config(
    db: &oxy::config::model::Database,
    workspace_manager: &WorkspaceManager,
) -> Option<ConnectorConfig> {
    match &db.database_type {
        // Stateless fleet only (Serve/Worker): the local DuckDB data file is
        // absent here. The compile worker mirrored this local-file warehouse to
        // S3 and recorded an `s3_mirror` block; attach from S3 via the same
        // httpfs SQL the core connector uses, instead of resolving the
        // (nonexistent) local path. Without this the connector silently drops
        // the DB and the agent run fails "no databases configured" on cloud
        // while working locally. (DuckLake / MotherDuck are already remote and
        // carry no mirror.)
        //
        // Gated on the stateless roles: a node WITH the working copy (ide /
        // all-in-one) reads the SAME compiled config (which carries `s3_mirror`)
        // but DOES have the files, so it must fall through to the `Local` arm
        // below. The S3-mirror connector exposes data as views only and has no
        // `file_search_path`, so it can't resolve authored file-path SQL
        // (`FROM 'oxymart.csv'`) — which workflow/procedure steps emit and which
        // is exactly what runs on the ide once /agentic-workflows is ide-pinned.
        DatabaseType::DuckDB(duck)
            if duck.s3_mirror.is_some()
                && matches!(
                    crate::server::role_manifest::current_process_role(),
                    crate::server::role_manifest::Role::Serve
                        | crate::server::role_manifest::Role::Worker
                ) =>
        {
            let mirror = duck.s3_mirror.as_ref().expect("guarded by is_some()");
            Some(ConnectorConfig::DuckDbRaw(DuckDbRawConfig {
                init_statements: oxy::connector::build_s3_mirror_sql(mirror),
            }))
        }
        DatabaseType::DuckDB(duck) => match &duck.options {
            DuckDBOptions::Local { file_search_path } => {
                let path = match workspace_manager
                    .config_manager
                    .resolve_file(file_search_path)
                    .await
                {
                    Ok(p) => std::path::PathBuf::from(p),
                    Err(e) => {
                        tracing::warn!(db = %db.name, "DuckDB: cannot resolve path: {e}");
                        return None;
                    }
                };
                Some(ConnectorConfig::DuckDb(DuckDbConfig {
                    data_dir: path,
                    load_strategy: DuckDbLoadStrategy::View,
                }))
            }
            DuckDBOptions::File { path } => {
                let resolved = match workspace_manager.config_manager.resolve_file(path).await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(db = %db.name, "DuckDB file: cannot resolve path: {e}");
                        return None;
                    }
                };
                Some(ConnectorConfig::DuckDbUrl(DuckDbUrlConfig {
                    url: resolved,
                }))
            }
            DuckDBOptions::DuckLake(ducklake_config) => {
                let stmts = match ducklake_config
                    .to_duckdb_attach_stmt(&workspace_manager.secrets_manager)
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(db = %db.name, "DuckLake attach: {e}");
                        return None;
                    }
                };
                Some(ConnectorConfig::DuckDbRaw(DuckDbRawConfig {
                    init_statements: stmts,
                }))
            }
        },

        DatabaseType::Postgres(pg) => {
            let host = pg
                .get_host(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "localhost".into());
            let port: u16 = pg
                .get_port(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "5432".into())
                .parse()
                .unwrap_or(5432);
            let user = pg
                .get_user(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let password = pg
                .get_password(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let database = pg
                .get_database(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            Some(ConnectorConfig::Postgres(PostgresConfig {
                host,
                port,
                user,
                password,
                database,
            }))
        }

        // `Airhouse` and `AirhouseManaged` are handled by the host's
        // `resolve_pre_built_connector` path — see `resolve_pre_built_airhouse`
        // below — because the connector lives in the standalone `airhouse`
        // crate, not in `agentic-connector::ConnectorConfig`.
        DatabaseType::Airhouse(_) | DatabaseType::AirhouseManaged(_) => None,

        DatabaseType::Redshift(rds) => {
            let host = rds
                .get_host(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "localhost".into());
            let port: u16 = rds
                .get_port(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "5439".into())
                .parse()
                .unwrap_or(5439);
            let user = rds
                .get_user(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let password = rds
                .get_password(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let database = rds
                .get_database(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            Some(ConnectorConfig::Redshift(PostgresConfig {
                host,
                port,
                user,
                password,
                database,
            }))
        }

        DatabaseType::ClickHouse(ch) => {
            let host = ch
                .get_host(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "localhost".into());
            let user = ch
                .get_user(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let password = ch
                .get_password(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let database = ch
                .get_database(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let url = if host.contains("://") {
                // Caller already provided a full URL (scheme + host + optional
                // port + optional path).  Pass it through verbatim — common for
                // ClickHouse Cloud (HTTPS, port 8443) and port-forwarded hosts
                // where the user enters the full address.
                host
            } else if host.contains(':') {
                // Bare host:port — synthesize the scheme only.
                format!("http://{host}")
            } else {
                // Bare hostname — fall back to the default ClickHouse HTTP port.
                format!("http://{host}:8123")
            };
            Some(ConnectorConfig::ClickHouse(ClickHouseConfig {
                url,
                user,
                password,
                database,
            }))
        }

        DatabaseType::Snowflake(sf) => {
            let auth = match &sf.auth_type {
                SnowflakeAuthType::BrowserAuth {
                    browser_timeout_secs,
                    cache_dir,
                } => SnowflakeAuth::Browser {
                    timeout_secs: *browser_timeout_secs,
                    cache_dir: cache_dir.clone(),
                    sso_url_callback: None,
                },
                SnowflakeAuthType::PrivateKey { .. } => {
                    tracing::warn!(
                        db = %db.name,
                        "Snowflake: private-key auth not yet supported in agentic connector"
                    );
                    return None;
                }
                _ => {
                    let password = match sf.get_password(&workspace_manager.secrets_manager).await {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::warn!(db = %db.name, "Snowflake: cannot resolve password: {e}");
                            return None;
                        }
                    };
                    SnowflakeAuth::Password { password }
                }
            };
            Some(ConnectorConfig::Snowflake(SnowflakeConfig {
                account: sf.account.clone(),
                username: sf.username.clone(),
                auth,
                role: sf.role.clone(),
                warehouse: sf.warehouse.clone(),
                database: Some(sf.database.clone()),
                schema: sf.schema.clone(),
            }))
        }

        DatabaseType::Bigquery(bq) => {
            let key_path = match bq.get_key_path(&workspace_manager.secrets_manager).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(db = %db.name, "BigQuery: {e}");
                    return None;
                }
            };
            let key_path = workspace_manager
                .config_manager
                .resolve_file(&key_path)
                .await
                .unwrap_or(key_path);
            let project_id = extract_project_id_from_key(&key_path).unwrap_or_default();
            {
                // Merge legacy single `dataset` + multi `datasets` map keys.
                let mut datasets: Vec<String> = bq.datasets.keys().cloned().collect();
                if let Some(ref ds) = bq.dataset
                    && !datasets.contains(ds)
                {
                    datasets.push(ds.clone());
                }
                Some(ConnectorConfig::BigQuery(BigQueryConfig {
                    key_path,
                    project_id,
                    datasets,
                }))
            }
        }

        DatabaseType::MotherDuck(md) => {
            let token = match md.get_token(&workspace_manager.secrets_manager).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(db = %db.name, "MotherDuck token: {e}");
                    return None;
                }
            };
            let url = match &md.database {
                Some(db) => format!("md:{}?motherduck_token={}", db, token),
                None => format!("md:?motherduck_token={}", token),
            };
            Some(ConnectorConfig::DuckDbUrl(DuckDbUrlConfig { url }))
        }

        DatabaseType::Mysql(my) => {
            let host = my
                .get_host(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "localhost".into());
            let port: u16 = my
                .get_port(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "3306".into())
                .parse()
                .unwrap_or(3306);
            let user = my
                .get_user(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let password = my
                .get_password(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            let database = my
                .get_database(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_default();
            Some(ConnectorConfig::Mysql(MysqlConfig {
                host,
                port,
                user,
                password,
                database,
            }))
        }

        DatabaseType::DOMO(d) => {
            let developer_token = match workspace_manager
                .secrets_manager
                .resolve_secret(&d.developer_token_var)
                .await
            {
                Ok(Some(t)) => t,
                Ok(None) => {
                    tracing::warn!(
                        db = %db.name,
                        var = %d.developer_token_var,
                        "DOMO developer-token secret not found"
                    );
                    return None;
                }
                Err(e) => {
                    tracing::warn!(db = %db.name, "DOMO developer-token resolution failed: {e}");
                    return None;
                }
            };
            Some(ConnectorConfig::Domo(DomoConfig::from_instance(
                &d.instance,
                developer_token,
                &d.dataset_id,
            )))
        }
    }
}

fn extract_project_id_from_key(key_path: &str) -> Option<String> {
    let contents = std::fs::read_to_string(key_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&contents).ok()?;
    v.get("project_id")?.as_str().map(|s| s.to_string())
}

/// Pre-build an `airhouse::AirhouseConnector` for `airhouse` /
/// `airhouse_managed` databases. Errors are swallowed into a warn log and
/// returned as `None` because this is the agentic-pipeline path: a single
/// failing database shouldn't kill batch resolution. Non-agentic callers
/// (the SQL IDE, the builder bridge) use [`OxyProjectContext::build_connector_for`]
/// instead, which propagates the same errors so the user sees what's wrong.
async fn resolve_pre_built_airhouse(
    db_name: &str,
    workspace_manager: &WorkspaceManager,
    subject: Option<uuid::Uuid>,
    role: Option<WorkspaceRole>,
) -> Option<Arc<dyn DatabaseConnector>> {
    let db = workspace_manager
        .config_manager
        .resolve_database(db_name)
        .ok()?;
    if !matches!(
        db.database_type,
        DatabaseType::Airhouse(_) | DatabaseType::AirhouseManaged(_)
    ) {
        return None;
    }
    match build_airhouse_connector(&db, workspace_manager, subject, role).await {
        Ok(conn) => Some(conn),
        Err(e) => {
            tracing::warn!(db = %db.name, "airhouse connector build failed: {e}");
            None
        }
    }
}

/// Error-propagating airhouse connector builder. Used by both
/// [`resolve_pre_built_airhouse`] (which discards the error) and
/// [`OxyProjectContext::build_connector_for`] (which surfaces it). Keeps
/// the per-`DatabaseType` arms in one place.
async fn build_airhouse_connector(
    db: &oxy::config::model::Database,
    workspace_manager: &WorkspaceManager,
    subject: Option<uuid::Uuid>,
    role: Option<WorkspaceRole>,
) -> Result<Arc<dyn DatabaseConnector>, OxyError> {
    let (host, port, user, password, database) =
        airhouse_wire_params(db, workspace_manager, subject, role).await?;

    let conn = airhouse::AirhouseConnector::new(&host, port, &user, &password, &database)
        .await
        .map_err(|e| {
            OxyError::DBError(format!(
                "airhouse connector failed to connect to {host}:{port}/{database} as {user}: {e}"
            ))
        })?;
    Ok(Arc::new(conn) as Arc<dyn DatabaseConnector>)
}

/// Build a `postgresql://` URL, percent-encoding the userinfo and path
/// so credentials containing `@ : / ?` survive parsing by airway's
/// Postgres/airhouse destination.
fn pg_wire_dsn(user: &str, password: &str, host: &str, port: u16, database: &str) -> String {
    format!(
        "postgresql://{}:{}@{}:{}/{}",
        urlencoding::encode(user),
        urlencoding::encode(password),
        host,
        port,
        urlencoding::encode(database),
    )
}

/// Resolve the Postgres-wire connection parameters for an `airhouse` /
/// `airhouse_managed` database. For `airhouse_managed` this mints an
/// ephemeral per-subject credential via the SA broker. Shared by the
/// connector builder and the airway destination resolver.
async fn airhouse_wire_params(
    db: &oxy::config::model::Database,
    workspace_manager: &WorkspaceManager,
    subject: Option<uuid::Uuid>,
    role: Option<WorkspaceRole>,
) -> Result<(String, u16, String, String, String), OxyError> {
    let params = match &db.database_type {
        DatabaseType::Airhouse(ah) => {
            let host = ah
                .get_host(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "localhost".into());
            let port: u16 = ah
                .get_port(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "5445".into())
                .parse()
                .unwrap_or(5445);
            let user = ah
                .get_user(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "admin".into());
            let password = ah
                .get_password(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "airhouse".into());
            let database = ah
                .get_database(&workspace_manager.secrets_manager)
                .await
                .unwrap_or_else(|_| "airhouse".into());
            (host, port, user, password, database)
        }
        DatabaseType::AirhouseManaged(_) => {
            mint_airhouse_managed_creds(workspace_manager, subject, role).await?
        }
        other => {
            return Err(OxyError::ConfigurationError(format!(
                "build_airhouse_connector: unexpected database_type {other:?}"
            )));
        }
    };

    Ok(params)
}

/// Mint an ephemeral wire-protocol credential for an `airhouse_managed`
/// database via the SA-backed broker. Returns the
/// (host, port, username, password, dbname) tuple ready for the connector
/// constructor.
///
/// HTTP-handled paths arrive here with both `subject` (the requesting user
/// id, threaded by the workspace middleware) and `role` (the effective
/// workspace role) populated; the broker mints with the matching airhouse
/// role per the standard mapping. Background paths (scheduler, schema
/// crawl, agentic background runs) leave `subject = None` and fall through
/// to `mint_for_system` with a `system:workspace:<uuid>:agentic-bg`
/// audit subject.
async fn mint_airhouse_managed_creds(
    workspace_manager: &WorkspaceManager,
    subject: Option<uuid::Uuid>,
    role: Option<WorkspaceRole>,
) -> Result<(String, u16, String, String, String), OxyError> {
    let workspace_id = workspace_manager.workspace_id;
    let endpoint = airhouse::wire_endpoint().ok_or_else(|| {
        OxyError::ConfigurationError(
            "airhouse_managed: AIRHOUSE_WIRE_HOST is not configured; the airhouse \
             integration must be enabled (set AIRHOUSE_BASE_URL, AIRHOUSE_ADMIN_TOKEN, \
             AIRHOUSE_WIRE_HOST, AIRHOUSE_WIRE_PORT)"
                .into(),
        )
    })?;
    let broker = airhouse::token_broker().ok_or_else(|| {
        OxyError::ConfigurationError(
            "airhouse_managed: token broker not initialised; check the airhouse env \
             vars (AIRHOUSE_BASE_URL / AIRHOUSE_ADMIN_TOKEN / AIRHOUSE_WIRE_HOST)"
                .into(),
        )
    })?;

    let cred = match subject {
        Some(uid) => {
            let airhouse_role = role
                .map(airhouse::airhouse_role_for)
                // No effective role in scope (e.g. handler that didn't extract
                // EffectiveWorkspaceRole). Default to reader — least-privilege.
                .unwrap_or(airhouse::UserRole::Reader);
            broker
                .mint_for_user(
                    workspace_id,
                    uid,
                    airhouse_role,
                    airhouse::DEFAULT_INTERNAL_TTL,
                )
                .await
                .map_err(OxyError::from)?
        }
        None => broker
            .mint_for_system(
                workspace_id,
                airhouse::SystemPurpose::AgenticBackground,
                airhouse::UserRole::Admin,
                airhouse::DEFAULT_INTERNAL_TTL,
            )
            .await
            .map_err(OxyError::from)?,
    };

    Ok((
        endpoint.host,
        endpoint.port,
        cred.username,
        cred.password,
        cred.tenant,
    ))
}

// ── Model translation ───────────────────────────────────────────────────────

async fn resolve_model_impl(
    model_ref: Option<&str>,
    has_explicit_model: bool,
    workspace_manager: &WorkspaceManager,
) -> Option<ResolvedModelInfo> {
    let (name, is_explicit_ref) = if let Some(ref_name) = model_ref {
        (ref_name, true)
    } else if !has_explicit_model {
        let n = workspace_manager.config_manager.default_model()?;
        (n, false)
    } else {
        return None;
    };

    match workspace_manager.config_manager.resolve_model(name) {
        Ok(model) => {
            let model_name = model.model_name().to_string();
            let key_var = model.key_var().map(|s| s.to_string());

            let (vendor, base_url, extra_api_key, azure_deployment_id, azure_api_version) =
                match model {
                    Model::Anthropic { config: m } => {
                        (LlmVendor::Anthropic, m.api_url.clone(), None, None, None)
                    }
                    Model::OpenAI { config: m } => {
                        let (dep_id, api_ver) = m
                            .azure
                            .as_ref()
                            .map(|a| {
                                (
                                    Some(a.azure_deployment_id.clone()),
                                    Some(a.azure_api_version.clone()),
                                )
                            })
                            .unwrap_or((None, None));
                        (LlmVendor::OpenAi, m.api_url.clone(), None, dep_id, api_ver)
                    }
                    Model::Ollama { config: m } => (
                        LlmVendor::OpenAiCompat,
                        Some(m.api_url.clone()),
                        Some(m.api_key.clone()),
                        None,
                        None,
                    ),
                    Model::Google { .. } => {
                        tracing::warn!(
                            model = name,
                            "Google/Gemini models are not yet supported in analytics agents"
                        );
                        return None;
                    }
                };

            // Resolve api_key via secrets_manager first, env fallback. Ollama
            // carries its key inline via the config — honor that.
            let api_key = if let Some(inline) = extra_api_key {
                Some(inline)
            } else if let Some(kv) = key_var.as_deref() {
                workspace_manager
                    .secrets_manager
                    .resolve_secret(kv)
                    .await
                    .ok()
                    .flatten()
                    .or_else(|| std::env::var(kv).ok())
            } else {
                None
            };

            Some(ResolvedModelInfo {
                model: model_name,
                vendor,
                api_key,
                base_url,
                is_explicit_ref,
                azure_deployment_id,
                azure_api_version,
            })
        }
        Err(e) => {
            tracing::warn!(model = name, "could not resolve model from config.yml: {e}");
            None
        }
    }
}
