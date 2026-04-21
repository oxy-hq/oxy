//! `ProjectContext` implementation backed by Oxy's [`WorkspaceManager`].
//!
//! All `oxy::*` imports for connector/model resolution live in this file.
//! This is the concrete adapter — the agentic stack sees only the trait.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentic_analytics::config::{LlmVendor, ResolvedModelInfo};
use agentic_connector::{
    BigQueryConfig, ClickHouseConfig, ConnectorConfig, DatabaseConnector, DuckDbConfig,
    DuckDbLoadStrategy, DuckDbRawConfig, DuckDbUrlConfig, PostgresConfig, SnowflakeConfig,
};
use agentic_pipeline::SharedMetricSink;
use agentic_pipeline::platform::ProjectContext;
use agentic_workflow::WorkspaceContext;
use agentic_workflow::workspace::IntegrationConfig;
use async_trait::async_trait;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy::config::model::{DatabaseType, DuckDBOptions, IntegrationType, Model, SnowflakeAuthType};

/// Adapter that exposes a [`WorkspaceManager`] as a [`ProjectContext`] and
/// (for the workflow runner) an [`agentic_workflow::WorkspaceContext`].
///
/// Built connectors are cached via a [`tokio::sync::OnceCell`] so the workflow
/// runner's `get_connector` calls don't re-resolve/re-build per step. The
/// cache is per-instance; callers that want to share it should wrap in `Arc`
/// before handing the context around.
pub struct OxyProjectContext {
    workspace_manager: WorkspaceManager,
    connectors: tokio::sync::OnceCell<HashMap<String, Arc<dyn DatabaseConnector>>>,
}

impl OxyProjectContext {
    pub fn new(workspace_manager: WorkspaceManager) -> Self {
        Self {
            workspace_manager,
            connectors: tokio::sync::OnceCell::new(),
        }
    }

    pub fn workspace_manager(&self) -> &WorkspaceManager {
        &self.workspace_manager
    }

    /// Build connectors from workspace database configs. Called once lazily
    /// on first `WorkspaceContext::get_connector` invocation.
    async fn built_connectors(&self) -> &HashMap<String, Arc<dyn DatabaseConnector>> {
        self.connectors
            .get_or_init(|| async {
                let db_names: Vec<String> = self
                    .workspace_manager
                    .config_manager
                    .list_databases()
                    .iter()
                    .map(|db| db.name.clone())
                    .collect();
                let configs = agentic_pipeline::platform::resolve_connectors(&db_names, self).await;
                let mut map = HashMap::new();
                for (name, cfg) in configs {
                    match agentic_connector::build_connector(cfg) {
                        Ok(connector) => {
                            map.insert(name, Arc::from(connector));
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "workspace_context",
                                db = %name,
                                error = %e,
                                "failed to build connector"
                            );
                        }
                    }
                }
                map
            })
            .await
    }
}

#[async_trait]
impl ProjectContext for OxyProjectContext {
    async fn resolve_connector(&self, db_name: &str) -> Option<ConnectorConfig> {
        resolve_connector_impl(db_name, &self.workspace_manager).await
    }

    async fn resolve_model(
        &self,
        model_ref: Option<&str>,
        has_explicit_model: bool,
    ) -> Option<ResolvedModelInfo> {
        resolve_model_impl(model_ref, has_explicit_model, &self.workspace_manager).await
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

    fn metric_sink(&self) -> Option<SharedMetricSink> {
        // Only hand back a sink when an observability store is actually
        // registered. Non-enterprise runs leave `get_global()` as `None`
        // and the adapter's first call would just log-and-skip anyway —
        // returning `None` here keeps the pipeline hot path free of the
        // atomic load + tracing::warn on every query.
        if oxy_observability::global::get_global().is_none() {
            return None;
        }
        Some(Arc::new(super::metric_sink::OxyAnalyticsMetricSink::new()))
    }
}

#[async_trait]
impl WorkspaceContext for OxyProjectContext {
    fn workspace_path(&self) -> &Path {
        self.workspace_manager.config_manager.workspace_path()
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
        let connectors = self.built_connectors().await;
        connectors.get(name).cloned().ok_or_else(|| {
            let available: Vec<&str> = connectors.keys().map(|k| k.as_str()).collect();
            format!("database '{}' not found. Available: {:?}", name, available)
        })
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
        }
    }

    async fn list_workflow_files(&self) -> Result<Vec<PathBuf>, String> {
        self.workspace_manager
            .config_manager
            .list_workflows()
            .await
            .map_err(|e| format!("{e}"))
    }

    async fn resolve_workflow_yaml(&self, workflow_ref: &str) -> Result<String, String> {
        let path = self
            .workspace_manager
            .config_manager
            .workspace_path()
            .join(workflow_ref);
        std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read workflow {}: {e}", path.display()))
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

    match &db.database_type {
        DatabaseType::DuckDB(duck) => match &duck.options {
            DuckDBOptions::Local { file_search_path } => {
                let path = match workspace_manager
                    .config_manager
                    .resolve_file(file_search_path)
                    .await
                {
                    Ok(p) => std::path::PathBuf::from(p),
                    Err(e) => {
                        tracing::warn!(db = %db_name, "DuckDB: cannot resolve path: {e}");
                        return None;
                    }
                };
                Some(ConnectorConfig::DuckDb(DuckDbConfig {
                    data_dir: path,
                    load_strategy: DuckDbLoadStrategy::View,
                }))
            }
            DuckDBOptions::DuckLake(ducklake_config) => {
                let stmts = match ducklake_config
                    .to_duckdb_attach_stmt(&workspace_manager.secrets_manager)
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(db = %db_name, "DuckLake attach: {e}");
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
            let url = format!("http://{}:8123", host);
            Some(ConnectorConfig::ClickHouse(ClickHouseConfig {
                url,
                user,
                password,
                database,
            }))
        }

        DatabaseType::Snowflake(sf) => {
            let password = match sf.get_password(&workspace_manager.secrets_manager).await {
                Ok(p) => p,
                Err(e) => {
                    if matches!(
                        sf.auth_type,
                        SnowflakeAuthType::Password { .. } | SnowflakeAuthType::PasswordVar { .. }
                    ) {
                        tracing::warn!(db = %db_name, "Snowflake: cannot resolve password: {e}");
                    } else {
                        tracing::warn!(db = %db_name, "Snowflake: only password auth supported in agentic connector");
                    }
                    return None;
                }
            };
            Some(ConnectorConfig::Snowflake(SnowflakeConfig {
                account: sf.account.clone(),
                username: sf.username.clone(),
                password,
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
                    tracing::warn!(db = %db_name, "BigQuery: {e}");
                    return None;
                }
            };
            let key_path = workspace_manager
                .config_manager
                .resolve_file(&key_path)
                .await
                .unwrap_or(key_path);
            let project_id = extract_project_id_from_key(&key_path).unwrap_or_default();
            Some(ConnectorConfig::BigQuery(BigQueryConfig {
                key_path,
                project_id,
                dataset: bq.dataset.clone(),
            }))
        }

        DatabaseType::MotherDuck(md) => {
            let token = match md.get_token(&workspace_manager.secrets_manager).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(db = %db_name, "MotherDuck token: {e}");
                    return None;
                }
            };
            let url = match &md.database {
                Some(db) => format!("md:{}?motherduck_token={}", db, token),
                None => format!("md:?motherduck_token={}", token),
            };
            Some(ConnectorConfig::DuckDbUrl(DuckDbUrlConfig { url }))
        }

        DatabaseType::Mysql(_) => {
            tracing::warn!(db = %db_name, "MySQL not yet supported in agentic connector");
            None
        }
        DatabaseType::DOMO(_) => {
            tracing::warn!(db = %db_name, "DOMO not yet supported in agentic connector");
            None
        }
    }
}

fn extract_project_id_from_key(key_path: &str) -> Option<String> {
    let contents = std::fs::read_to_string(key_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&contents).ok()?;
    v.get("project_id")?.as_str().map(|s| s.to_string())
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

            let (vendor, base_url, extra_api_key) = match model {
                Model::Anthropic { config: m } => (LlmVendor::Anthropic, m.api_url.clone(), None),
                Model::OpenAI { config: m } => (LlmVendor::OpenAi, m.api_url.clone(), None),
                Model::Ollama { config: m } => (
                    LlmVendor::OpenAiCompat,
                    Some(m.api_url.clone()),
                    Some(m.api_key.clone()),
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
            })
        }
        Err(e) => {
            tracing::warn!(model = name, "could not resolve model from config.yml: {e}");
            None
        }
    }
}
