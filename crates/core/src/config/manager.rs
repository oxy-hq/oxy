use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    config::{agent_config::AgenticConfig, constants::DATABASE_SEMANTIC_PATH},
    observability::events,
    storage::{S3BlobStorage, S3BlobStorageConfig, SharedBlobStorage},
};
use oxy_shared::errors::OxyError;

use super::{
    model::{
        AgentConfig, AppConfig, BuilderAgentConfig, Config, Database, Model, Workflow,
        WorkflowWithRawVariables,
    },
    storage::{ConfigSource, ConfigStorage},
    test_config::TestFileConfig,
};

#[derive(Debug, Clone)]
pub struct ConfigManager {
    storage: Arc<ConfigSource>,
    config: Arc<Config>,
}

impl ConfigManager {
    pub(super) fn new(storage: ConfigSource, config: Config) -> Self {
        Self {
            storage: Arc::new(storage),
            config: Arc::new(config),
        }
    }

    pub fn models(&self) -> &[Model] {
        &self.config.models
    }

    pub fn resolve_model(&self, model_name: &str) -> Result<&Model, OxyError> {
        let model = self
            .config
            .models
            .iter()
            .find(|m| m.name() == model_name)
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!("Model '{model_name}' not found in config"))
            })?;
        Ok(model)
    }

    pub fn default_model(&self) -> Option<&str> {
        self.config.models.first().map(|m| m.name())
    }

    pub fn resolve_database(&self, database_name: &str) -> Result<&Database, OxyError> {
        let database = self
            .config
            .databases
            .iter()
            .find(|w| w.name == database_name)
            .ok_or_else(|| {
                OxyError::ConfigurationError(format!(
                    "Database '{database_name}' not found in config"
                ))
            })?;
        Ok(database)
    }

    pub fn default_database_ref(&self) -> Option<&String> {
        self.config.defaults.as_ref().map(|d| d.database.as_ref())?
    }

    /// Path of the agent the workspace's `defaults.agent` points to
    /// (relative to workspace root), when configured. Callers that need to
    /// pick "the" agent without enumerating — primarily the Slack
    /// integration — read this and fall back to alphabetical-first when
    /// it's `None`.
    pub fn default_agent_ref(&self) -> Option<&String> {
        self.config.defaults.as_ref().and_then(|d| d.agent.as_ref())
    }

    /// Returns the configured protected branches, if any.
    pub fn protected_branches(&self) -> Option<&[String]> {
        self.config.protected_branches.as_deref()
    }

    /// Returns the configured fork-point branch for auto-created worktrees, if any.
    pub fn base_branch(&self) -> Option<&str> {
        self.config.base_branch.as_deref()
    }

    pub async fn resolve_file<P: AsRef<Path>>(&self, file_ref: P) -> Result<String, OxyError> {
        self.storage.fs_link(file_ref).await
    }

    pub async fn resolve_glob(&self, paths: &Vec<String>) -> Result<Vec<String>, OxyError> {
        let mut expanded_paths = Vec::new();
        for path in paths {
            expanded_paths.extend(self.storage.glob(path).await?);
        }
        Ok(expanded_paths)
    }

    pub async fn resolve_workflow<P: AsRef<Path>>(
        &self,
        workflow_name: P,
    ) -> Result<Workflow, OxyError> {
        self.storage.load_workflow_config(workflow_name).await
    }

    pub async fn resolve_workflow_with_raw_variables<P: AsRef<Path>>(
        &self,
        workflow_name: P,
    ) -> Result<WorkflowWithRawVariables, OxyError> {
        self.storage
            .load_workflow_config_with_raw_variables(workflow_name)
            .await
    }

    #[tracing::instrument(skip_all, err, fields(
        oxy.name = events::agent::load_agent_config::NAME,
        oxy.span_type = events::agent::load_agent_config::TYPE,
    ))]
    pub async fn resolve_agent<P: AsRef<Path>>(
        &self,
        agent_name: P,
    ) -> Result<AgentConfig, OxyError> {
        let agent_name_str = agent_name.as_ref().display().to_string();
        events::agent::load_agent_config::input(&agent_name_str);
        let output = self.storage.load_agent_config(agent_name).await?;
        events::agent::load_agent_config::output(&output);
        Ok(output)
    }

    pub async fn resolve_agentic_workflow<P: AsRef<Path>>(
        &self,
        agent_name: P,
    ) -> Result<AgenticConfig, OxyError> {
        self.storage.load_agentic_workflow_config(agent_name).await
    }

    pub fn list_databases(&self) -> &[Database] {
        self.config.databases.as_slice()
    }

    pub fn list_looker_integrations(&self) -> Vec<&super::model::Integration> {
        self.config
            .integrations
            .iter()
            .filter(|i| matches!(i.integration_type, super::model::IntegrationType::Looker(_)))
            .collect()
    }

    pub async fn list_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        let agents = self.storage.list_agents().await?;
        tracing::info!("Agents: {:?}", agents);
        tracing::debug!("Builder: {:?}", self.config.builder_agent);
        if let Some(BuilderAgentConfig::Path(ref path)) = self.config.builder_agent {
            // hide the legacy path-based builder agent from the list
            let builder_agent_full_path = self.storage.fs_link(path).await.map_err(|_| {
                OxyError::ConfigurationError("Failed to resolve agent path".to_string())
            })?;
            Ok(agents
                .iter()
                .filter(|agent| agent.display().to_string() != builder_agent_full_path)
                .cloned()
                .collect())
        } else {
            Ok(agents)
        }
    }

    pub async fn list_apps(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_apps().await
    }
    pub async fn list_agentic_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_agentic_workflows().await
    }
    pub async fn list_analytics_agents(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_analytics_agents().await
    }
    pub async fn list_workflows(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_workflows().await
    }

    pub async fn resolve_app<P: AsRef<Path>>(&self, app_path: P) -> Result<AppConfig, OxyError> {
        self.storage.load_app_config(app_path).await
    }

    pub async fn resolve_test<P: AsRef<Path>>(
        &self,
        test_ref: P,
    ) -> Result<TestFileConfig, OxyError> {
        self.storage.load_test_config(test_ref).await
    }

    pub async fn list_tests(&self) -> Result<Vec<PathBuf>, OxyError> {
        self.storage.list_tests().await
    }

    pub async fn get_build_agent(&self) -> Result<AgentConfig, OxyError> {
        match &self.config.builder_agent {
            Some(BuilderAgentConfig::Path(path)) => self.resolve_agent(path).await,
            Some(BuilderAgentConfig::Builtin { .. }) => Err(OxyError::ConfigurationError(
                "Built-in builder agent does not use an agent file. Use get_builder_config() instead.".to_string(),
            )),
            None => Err(OxyError::ConfigurationError(
                "No builder agent specified in config".to_string(),
            )),
        }
    }

    pub async fn get_builder_agent_path(&self) -> Result<PathBuf, OxyError> {
        match &self.config.builder_agent {
            Some(BuilderAgentConfig::Path(path)) => Ok(path.to_owned()),
            Some(BuilderAgentConfig::Builtin { .. }) => Err(OxyError::ConfigurationError(
                "Built-in builder agent does not have a file path".to_string(),
            )),
            None => Err(OxyError::ConfigurationError(
                "No builder agent specified in config".to_string(),
            )),
        }
    }

    /// Returns the full builder agent config, if any.
    pub fn get_builder_config(&self) -> Option<&BuilderAgentConfig> {
        self.config.builder_agent.as_ref()
    }

    /// Returns true when the builder is configured as a built-in copilot
    /// (i.e. `builder_agent: { model: "..." }`).
    pub fn is_builder_builtin(&self) -> bool {
        matches!(
            self.config.builder_agent,
            Some(BuilderAgentConfig::Builtin { .. })
        )
    }

    pub fn get_config(&self) -> &Config {
        &self.config
    }

    pub fn get_model_key_var(&self, model: &Model) -> Option<String> {
        model.key_var().map(|s| s.to_string())
    }

    pub fn get_database_password_var(&self, database: &Database) -> Option<String> {
        match &database.database_type {
            crate::config::model::DatabaseType::Postgres(postgres) => postgres.password_var.clone(),
            crate::config::model::DatabaseType::Airhouse(airhouse) => airhouse.password_var.clone(),
            crate::config::model::DatabaseType::Mysql(mysql) => mysql.password_var.clone(),
            crate::config::model::DatabaseType::Snowflake(snowflake) => {
                snowflake.auth_type.get_password_var().cloned()
            }
            crate::config::model::DatabaseType::ClickHouse(clickhouse) => {
                clickhouse.password_var.clone()
            }
            crate::config::model::DatabaseType::Redshift(redshift) => redshift.password_var.clone(),
            _ => None, // Other database types might not have password_var
        }
    }

    pub async fn resolve_state_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.resolve_state_dir().await
    }

    pub async fn get_charts_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.get_charts_dir().await
    }

    /// Build the remote blob storage backend for assets from environment
    /// variables. Returns `Ok(None)` when `OXY_STORAGE_BACKEND` is unset
    /// or set to `local` — assets stay on local disk in that mode.
    ///
    /// Storage is a deployment-level concern (like `OXY_DATABASE_URL`) and
    /// must not live in `config.yml` which is a per-workspace developer file.
    ///
    /// ## Environment variables
    ///
    /// | Variable | Required | Description |
    /// |---|---|---|
    /// | `OXY_STORAGE_BACKEND` | No | `s3` to enable S3; anything else (or absent) keeps local disk |
    /// | `OXY_S3_BUCKET` | When S3 | S3 bucket name |
    /// | `OXY_S3_REGION` | No | AWS region (uses SDK default when absent) |
    /// | `OXY_S3_PREFIX` | No | Key prefix inside the bucket, e.g. `charts` |
    /// | `OXY_S3_PUBLIC_URL_BASE` | No | CDN base for already-public buckets (e.g. CloudFront). When unset (the default), the app generates **presigned GET URLs** instead — works with private buckets and SSE-KMS. |
    /// | `OXY_S3_PRESIGN_TTL_SECONDS` | No | TTL for presigned URLs in seconds. Default `3600`. Must be a positive integer. Ignored when `OXY_S3_PUBLIC_URL_BASE` is set. |
    ///
    /// AWS credentials follow the standard SDK chain (env vars, shared config, IAM role).
    pub(crate) async fn chart_image_blob_storage(
        &self,
    ) -> Result<Option<SharedBlobStorage>, OxyError> {
        let backend = std::env::var("OXY_STORAGE_BACKEND").unwrap_or_default();
        if backend.to_lowercase() != "s3" {
            return Ok(None);
        }
        let bucket = std::env::var("OXY_S3_BUCKET").map_err(|_| {
            OxyError::ConfigurationError(
                "OXY_STORAGE_BACKEND=s3 requires OXY_S3_BUCKET to be set".to_string(),
            )
        })?;
        let presign_ttl = parse_presign_ttl(std::env::var("OXY_S3_PRESIGN_TTL_SECONDS").ok())?;
        let cfg = S3BlobStorageConfig {
            bucket,
            region: std::env::var("OXY_S3_REGION").ok(),
            prefix: std::env::var("OXY_S3_PREFIX").ok(),
            public_url_base: std::env::var("OXY_S3_PUBLIC_URL_BASE").ok(),
            presign_ttl,
        };
        let storage = S3BlobStorage::new(cfg).await?;
        Ok(Some(Arc::new(storage) as SharedBlobStorage))
    }

    pub async fn get_exported_chart_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.get_exported_chart_dir().await
    }

    pub async fn get_results_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.get_results_dir().await
    }

    pub async fn get_app_results_dir(&self) -> Result<PathBuf, OxyError> {
        self.storage.get_app_results_dir().await
    }

    /// Gets the workspace path from the configuration
    pub fn workspace_path(&self) -> &std::path::Path {
        &self.config.workspace_path
    }

    /// Gets the semantics directory path (workspace_path/semantics).
    /// Used for writing semantic files.
    pub fn semantics_path(&self) -> PathBuf {
        self.config.workspace_path.join("semantics")
    }

    /// Gets the base path for scanning semantic layer files.
    /// Scans the entire project so .view.yml/.topic.yml files can live anywhere.
    pub fn semantics_scan_path(&self) -> PathBuf {
        self.config.workspace_path.clone()
    }

    pub fn database_semantic_path(&self) -> PathBuf {
        self.config.workspace_path.join(DATABASE_SEMANTIC_PATH)
    }

    pub fn get_integration_by_name(
        &self,
        integration_name: &str,
    ) -> Option<&crate::config::model::Integration> {
        self.config
            .integrations
            .iter()
            .find(|i| i.name == integration_name)
    }

    /// Updates the databases in the config and writes to config.yml
    pub async fn update_databases(&self, new_databases: Vec<Database>) -> Result<(), OxyError> {
        // Create a new config with updated databases
        let mut updated_config = (*self.config).clone();
        updated_config.databases = new_databases;

        // Write the updated config
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Adds a database to the existing configuration
    pub async fn add_database(&self, database: Database) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        // Check if database with same name exists
        if updated_config
            .databases
            .iter()
            .any(|db| db.name == database.name)
        {
            return Err(OxyError::ConfigurationError(format!(
                "Database with name '{}' already exists",
                database.name
            )));
        }

        updated_config.databases.push(database);
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Adds multiple databases to the existing configuration
    pub async fn add_databases(&self, databases: Vec<Database>) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        // Check for duplicates
        for database in &databases {
            if updated_config
                .databases
                .iter()
                .any(|db| db.name == database.name)
            {
                return Err(OxyError::ConfigurationError(format!(
                    "Database with name '{}' already exists",
                    database.name
                )));
            }
        }

        updated_config.databases.extend(databases);
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Removes a database from the configuration by name
    pub async fn remove_database(&self, database_name: &str) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        // Find and remove the database
        let initial_len = updated_config.databases.len();
        updated_config
            .databases
            .retain(|db| db.name != database_name);

        if updated_config.databases.len() == initial_len {
            return Err(OxyError::ConfigurationError(format!(
                "Database with name '{}' not found",
                database_name
            )));
        }

        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Removes a model entry from the configuration by name.
    pub async fn remove_model(&self, model_name: &str) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        let initial_len = updated_config.models.len();
        updated_config.models.retain(|m| m.name() != model_name);

        if updated_config.models.len() == initial_len {
            return Err(OxyError::ConfigurationError(format!(
                "Model with name '{}' not found",
                model_name
            )));
        }

        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Returns the current data repos
    pub fn list_repositories(&self) -> &[crate::config::model::Repository] {
        &self.config.repositories
    }

    /// Adds a repository to the configuration
    pub async fn add_repository(
        &self,
        repo: crate::config::model::Repository,
    ) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        if updated_config
            .repositories
            .iter()
            .any(|r| r.name == repo.name)
        {
            return Err(OxyError::ConfigurationError(format!(
                "Repository with name '{}' already exists",
                repo.name
            )));
        }

        updated_config.repositories.push(repo);
        self.storage.write_config(&updated_config).await?;
        Ok(())
    }

    /// Removes a repository from the configuration by name
    pub async fn remove_repository(&self, name: &str) -> Result<(), OxyError> {
        let mut updated_config = (*self.config).clone();

        let initial_len = updated_config.repositories.len();
        updated_config.repositories.retain(|r| r.name != name);

        if updated_config.repositories.len() == initial_len {
            return Err(OxyError::ConfigurationError(format!(
                "Repository with name '{}' not found",
                name
            )));
        }

        self.storage.write_config(&updated_config).await?;
        Ok(())
    }
}

/// Parse `OXY_S3_PRESIGN_TTL_SECONDS` into a `Duration`, defaulting to
/// 3600 seconds when the variable is absent. Rejects 0, negatives, and
/// non-numeric values with `OxyError::ConfigurationError` so a typo in
/// the env never silently produces a 0-second TTL or a misconfigured
/// backend.
///
/// Also caps the value at AWS SigV4's hard upper bound of 7 days
/// (604_800 seconds). Values above that are rejected at startup —
/// without this check, `PresigningConfig::expires_in` would only error
/// out at the first chart upload, surfacing a misconfiguration after
/// the deploy looks healthy.
fn parse_presign_ttl(raw: Option<String>) -> Result<std::time::Duration, OxyError> {
    const DEFAULT_TTL_SECS: u64 = 3600;
    /// AWS SigV4 hard limit. <https://docs.aws.amazon.com/AmazonS3/latest/userguide/ShareObjectPreSignedURL.html>
    const MAX_TTL_SECS: u64 = 604_800;
    let secs = match raw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => DEFAULT_TTL_SECS,
        Some(s) => {
            let parsed: u64 = s.parse().map_err(|_| {
                OxyError::ConfigurationError(format!(
                    "OXY_S3_PRESIGN_TTL_SECONDS must be a positive integer, got '{s}'"
                ))
            })?;
            if parsed == 0 {
                return Err(OxyError::ConfigurationError(
                    "OXY_S3_PRESIGN_TTL_SECONDS must be > 0".to_string(),
                ));
            }
            if parsed > MAX_TTL_SECS {
                return Err(OxyError::ConfigurationError(format!(
                    "OXY_S3_PRESIGN_TTL_SECONDS must be ≤ {MAX_TTL_SECS} (AWS SigV4 7-day cap), got {parsed}"
                )));
            }
            parsed
        }
    };
    Ok(std::time::Duration::from_secs(secs))
}

#[cfg(test)]
mod presign_ttl_tests {
    use super::parse_presign_ttl;

    #[test]
    fn defaults_to_one_hour_when_absent() {
        let ttl = parse_presign_ttl(None).expect("default");
        assert_eq!(ttl.as_secs(), 3600);
    }

    #[test]
    fn defaults_to_one_hour_when_blank() {
        let ttl = parse_presign_ttl(Some("   ".into())).expect("blank treated as default");
        assert_eq!(ttl.as_secs(), 3600);
    }

    #[test]
    fn accepts_positive_integer() {
        let ttl = parse_presign_ttl(Some("900".into())).expect("900s");
        assert_eq!(ttl.as_secs(), 900);
    }

    #[test]
    fn rejects_zero() {
        let err = parse_presign_ttl(Some("0".into())).expect_err("zero rejected");
        assert!(err.to_string().contains("> 0"), "got: {err}");
    }

    #[test]
    fn rejects_negative() {
        // u64 parse rejects "-1" — we just assert it surfaces as ConfigurationError.
        let err = parse_presign_ttl(Some("-1".into())).expect_err("negative rejected");
        assert!(err.to_string().contains("positive integer"), "got: {err}");
    }

    #[test]
    fn rejects_non_numeric() {
        let err = parse_presign_ttl(Some("forever".into())).expect_err("non-numeric rejected");
        assert!(err.to_string().contains("positive integer"), "got: {err}");
    }

    #[test]
    fn rejects_overflow() {
        // > u64::MAX → parse error → ConfigurationError. No overflow into
        // a small TTL.
        let err =
            parse_presign_ttl(Some("99999999999999999999".into())).expect_err("overflow rejected");
        assert!(err.to_string().contains("positive integer"), "got: {err}");
    }

    #[test]
    fn accepts_seven_day_cap() {
        // AWS SigV4's hard limit — exactly 7 days is allowed.
        let ttl = parse_presign_ttl(Some("604800".into())).expect("7 days");
        assert_eq!(ttl.as_secs(), 604_800);
    }

    #[test]
    fn rejects_above_seven_day_cap() {
        // One second past the 7-day cap would be silently accepted by us
        // and only blow up at the first PresigningConfig::expires_in()
        // call. Reject at startup so the misconfig fails fast.
        let err = parse_presign_ttl(Some("604801".into())).expect_err("above cap rejected");
        assert!(err.to_string().contains("604800"), "got: {err}");
        assert!(err.to_string().contains("7-day"), "got: {err}");
    }
}
