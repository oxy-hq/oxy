//! `ProjectContext` implementation backed by Oxy's [`WorkspaceManager`].
//!
//! All `oxy::*` imports for connector/model resolution live in this file.
//! This is the concrete adapter — the agentic stack sees only the trait.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentic_analytics::config::{LlmVendor, ResolvedModelInfo};
use agentic_connector::{
    BigQueryConfig, ClickHouseConfig, ConnectorConfig, DatabaseConnector, DomoConfig, DuckDbConfig,
    DuckDbLoadStrategy, DuckDbRawConfig, DuckDbUrlConfig, MysqlConfig, PostgresConfig,
    SnowflakeAuth, SnowflakeConfig,
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
                    match agentic_connector::build_connector_async(cfg).await {
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
        oxy_observability::global::get_global()?;
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
    database_to_connector_config(db, workspace_manager).await
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
            Some(ConnectorConfig::Airhouse(PostgresConfig {
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
