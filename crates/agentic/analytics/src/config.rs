//! Configuration layer for [`AnalyticsSolver`].
//!
//! Loads an agent YAML config file, resolves context globs (`.view.yml`,
//! `.topic.yml`, `.sql`, `.md`), builds the semantic catalog, constructs
//! the LLM client, and wires everything into an [`AnalyticsSolver`].
//!
//! # Example YAML
//!
//! ```yaml
//! instructions: |
//!   You are a revenue analytics assistant for Acme Corp.
//!   Always report currency in USD.
//!
//! databases:
//!   - type: duckdb
//!     path: ./data/warehouse.duckdb
//!
//! llm:
//!   model: claude-opus-4-6
//!   max_tokens: 4096
//!
//! context:
//!   - ./semantics/*.view.yml
//!   - ./semantics/*.topic.yml
//!   - ./examples/*.sql
//!   - ./docs/*.md
//!   - ./procedures/**/*.procedure.yml
//!
//! # Optional: delegate query execution to a vendor semantic engine.
//! # When present, the engine path is tried first (highest priority).
//! # Omit this section entirely to use the internal compiler + LLM only.
//!
//! # Cube:
//! semantic_engine:
//!   vendor: cube
//!   base_url: https://cube.example.com
//!   api_token: "${CUBE_API_TOKEN}"
//!
//! # Looker:
//! # semantic_engine:
//! #   vendor: looker
//! #   base_url: https://myco.looker.com
//! #   client_id: "${LOOKER_CLIENT_ID}"
//! #   client_secret: "${LOOKER_CLIENT_SECRET}"
//!
//! states:
//!   clarifying:
//!     instructions: |
//!       "Last quarter" means the most recent completed fiscal quarter.
//!     thinking: adaptive
//!     max_retries: 3
//!     model: claude-haiku-4-5   # fast/cheap model for triage
//!   solving:
//!     instructions: Prefer CTEs over subqueries.
//!     thinking:
//!       budget_tokens: 10000
//!     max_retries: 2
//!     model: claude-opus-4-6    # powerful model for SQL generation
//!   # OpenAI o-series effort (shorthand or map form):
//!   diagnosing:
//!     thinking: "effort:high"
//!   interpreting:
//!     thinking:
//!       effort: medium
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Deserialize;

use crate::catalog::SchemaCatalog;
use crate::engine::cube::CubeEngine;
use crate::engine::looker::LookerEngine;
use crate::engine::{EngineError, SemanticEngine};
use crate::llm::{
    DEFAULT_MODEL, LlmClient, OpenAiCompatProvider, OpenAiProvider, ReasoningEffort, ThinkingConfig,
};
use crate::semantic::SemanticCatalog;
use crate::solver::AnalyticsSolver;
use crate::validation::{ValidationConfig, Validator};

// ── Error ─────────────────────────────────────────────────────────────────────

/// Errors returned during config loading and solver construction.
#[derive(Debug)]
pub enum ConfigError {
    /// The YAML file could not be read.
    Io(std::io::Error),
    /// The YAML could not be parsed.
    Yaml(serde_yaml::Error),
    /// A glob pattern was invalid.
    Glob(glob::PatternError),
    /// No databases were configured.
    NoDatabases,
    /// The database type is unsupported (only `sqlite` is built in).
    UnsupportedConnector(String),
    /// The connector could not be opened.
    ConnectorError(String),
    /// Semantic files could not be loaded.
    SemanticError(Box<dyn std::error::Error + Send + Sync>),
    /// The same table name exists in more than one configured database.
    AmbiguousTable(String),
    /// A validation rule name is unknown or its parameters are invalid.
    ValidationError(String),
    /// The `semantic_engine.vendor` value is not a known bundled adapter.
    UnsupportedEngine(String),
    /// The vendor engine could not be reached during the startup health-check.
    ///
    /// Hard failure — the solver is never constructed when this fires.
    EngineConnectionError(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "IO error: {e}"),
            ConfigError::Yaml(e) => write!(f, "YAML parse error: {e}"),
            ConfigError::Glob(e) => write!(f, "glob pattern error: {e}"),
            ConfigError::NoDatabases => write!(f, "no databases configured"),
            ConfigError::UnsupportedConnector(t) => {
                write!(f, "unsupported connector type: '{t}'")
            }
            ConfigError::ConnectorError(e) => write!(f, "connector error: {e}"),
            ConfigError::SemanticError(e) => write!(f, "semantic catalog error: {e}"),
            ConfigError::AmbiguousTable(e) => write!(f, "ambiguous table: {e}"),
            ConfigError::ValidationError(e) => write!(f, "validation config error: {e}"),
            ConfigError::UnsupportedEngine(v) => {
                write!(f, "unsupported semantic engine vendor: '{v}'")
            }
            ConfigError::EngineConnectionError(e) => {
                write!(f, "semantic engine connection error: {e}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(e: serde_yaml::Error) -> Self {
        ConfigError::Yaml(e)
    }
}

impl From<glob::PatternError> for ConfigError {
    fn from(e: glob::PatternError) -> Self {
        ConfigError::Glob(e)
    }
}

// ── YAML deserialization types ─────────────────────────────────────────────────

/// Vendor identifier for bundled semantic engine adapters.
///
/// Internal to the config layer — never part of the `SemanticEngine` public API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum VendorKind {
    Cube,
    Looker,
}

/// Configuration for an external vendor semantic engine.
///
/// ```yaml
/// semantic_engine:
///   vendor: cube
///   base_url: https://cube.example.com
///   api_token: "${CUBE_API_TOKEN}"
///
/// # — OR for Looker —
/// semantic_engine:
///   vendor: looker
///   base_url: https://myco.looker.com
///   client_id: "${LOOKER_CLIENT_ID}"
///   client_secret: "${LOOKER_CLIENT_SECRET}"
/// ```
#[derive(Debug, Deserialize)]
pub struct SemanticEngineConfig {
    vendor: VendorKind,
    pub base_url: String,
    /// API token (Cube).  Supports `"${ENV_VAR}"` interpolation.
    #[serde(default)]
    pub api_token: Option<String>,
    /// OAuth client ID (Looker).
    #[serde(default)]
    pub client_id: Option<String>,
    /// OAuth client secret (Looker).
    #[serde(default)]
    pub client_secret: Option<String>,
}

impl SemanticEngineConfig {
    /// Resolve `api_token`, interpolating `${VAR}` from the environment.
    fn resolved_api_token(&self) -> Result<String, ConfigError> {
        let raw = self.api_token.as_deref().unwrap_or("");
        Ok(interpolate_env(raw))
    }

    fn resolved_client_id(&self) -> Result<String, ConfigError> {
        let raw = self.client_id.as_deref().unwrap_or("");
        Ok(interpolate_env(raw))
    }

    fn resolved_client_secret(&self) -> Result<String, ConfigError> {
        let raw = self.client_secret.as_deref().unwrap_or("");
        Ok(interpolate_env(raw))
    }
}

/// Interpolate `${VAR_NAME}` placeholders with environment variable values.
fn interpolate_env(s: &str) -> String {
    // Simple pattern: replace all ${FOO} with the env var FOO, or leave as-is.
    let mut result = s.to_string();
    while let Some(start) = result.find("${") {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 2..start + end];
            let value = std::env::var(var_name).unwrap_or_default();
            result = format!(
                "{}{}{}",
                &result[..start],
                value,
                &result[start + end + 1..]
            );
        } else {
            break;
        }
    }
    result
}

/// Top-level agent configuration.
#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    /// Global instructions injected into every LLM call.
    #[serde(default)]
    pub instructions: Option<String>,

    /// Database names from config.yml to use as connectors.
    ///
    /// Each string is the `name:` of a database entry in the project's
    /// `config.yml`.  The HTTP layer resolves these names to live connectors
    /// (via [`BuildContext`]) so that connection details are defined once and
    /// reused across agents.  All database types supported by Oxy (DuckDB,
    /// Postgres, BigQuery, Snowflake, ClickHouse, …) can be listed here.
    ///
    /// ```yaml
    /// databases:
    ///   - local
    ///   - analytics_db
    /// ```
    #[serde(default)]
    pub databases: Vec<String>,

    /// Glob patterns for context files (`.view.yml`, `.topic.yml`, `.sql`,
    /// `.md`).  Resolved relative to the directory containing the YAML file.
    #[serde(default)]
    pub context: Vec<String>,

    /// Per-state overrides.
    #[serde(default)]
    pub states: HashMap<String, StateConfig>,

    /// LLM configuration.
    #[serde(default)]
    pub llm: LlmConfigYaml,

    /// Global thinking/reasoning config applied to all pipeline states.
    ///
    /// Can be overridden per-state via the `states:` section.
    ///
    /// ```yaml
    /// thinking: adaptive       # shorthand
    /// # — or —
    /// thinking:
    ///   budget_tokens: 10000   # explicit budget
    /// ```
    #[serde(default)]
    pub thinking: Option<ThinkingConfigYaml>,

    /// Validation rule configuration.
    ///
    /// When absent, all built-in rules run with their default parameters.
    /// Use this section to disable specific rules or tune parameters such as
    /// the outlier detection threshold:
    ///
    /// ```yaml
    /// validation:
    ///   rules:
    ///     solved:
    ///       - name: outlier_detection
    ///         enabled: true
    ///         threshold_sigma: 3.0
    ///         min_rows: 6
    /// ```
    #[serde(default)]
    pub validation: Option<ValidationConfig>,

    /// Optional vendor semantic engine configuration.
    ///
    /// When absent, the solver routes exclusively through the internal
    /// semantic layer and LLM paths (identical to today's behaviour).
    #[serde(default)]
    pub semantic_engine: Option<SemanticEngineConfig>,
}

/// Per-state configuration overrides.
#[derive(Debug, Clone, Deserialize)]
pub struct StateConfig {
    /// Additional instructions injected for this state only.
    #[serde(default)]
    pub instructions: Option<String>,

    /// Thinking/reasoning config for this state.
    #[serde(default)]
    pub thinking: Option<ThinkingConfigYaml>,

    /// Maximum tool-use rounds before failing.
    #[serde(default)]
    pub max_retries: Option<u32>,

    /// Model ID override for this state.
    ///
    /// When set, this model is used instead of the global `llm.model`.
    /// The vendor, API key, and base URL from the global `llm:` section are
    /// inherited; only the model ID is replaced.
    ///
    /// ```yaml
    /// states:
    ///   clarifying:
    ///     model: claude-haiku-4-5   # cheap model for fast triage
    ///   solving:
    ///     model: claude-opus-4-6    # powerful model for SQL generation
    /// ```
    #[serde(default)]
    pub model: Option<String>,
}

/// Thinking configuration as expressed in YAML.
///
/// Accepts either a shorthand string (`"adaptive"`, `"disabled"`) or a
/// map with `budget_tokens`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ThinkingConfigYaml {
    /// Shorthand string: `"adaptive"`, `"disabled"`, `"effort:low"`,
    /// `"effort:medium"`, or `"effort:high"`.
    Shorthand(String),
    /// Explicit budget: `budget_tokens: N`.
    Manual { budget_tokens: u32 },
    /// OpenAI o-series reasoning effort: `effort: low|medium|high`.
    Effort { effort: String },
}

impl ThinkingConfigYaml {
    /// Convert to the runtime [`ThinkingConfig`].
    pub fn to_thinking_config(&self) -> ThinkingConfig {
        match self {
            ThinkingConfigYaml::Shorthand(s) if s.eq_ignore_ascii_case("adaptive") => {
                ThinkingConfig::Adaptive
            }
            // Shorthand "effort:low", "effort::low", etc.
            ThinkingConfigYaml::Shorthand(s) if s.to_ascii_lowercase().starts_with("effort:") => {
                let rest = s["effort:".len()..].trim_start_matches(':');
                parse_effort_level(rest)
            }
            ThinkingConfigYaml::Shorthand(_) => ThinkingConfig::Disabled,
            ThinkingConfigYaml::Manual { budget_tokens } => ThinkingConfig::Manual {
                budget_tokens: *budget_tokens,
            },
            // Map form: `effort: low|medium|high`
            ThinkingConfigYaml::Effort { effort } => parse_effort_level(effort),
        }
    }
}

/// Parse an effort level string into [`ThinkingConfig::Effort`].
/// Defaults to `Medium` for unrecognised values.
fn parse_effort_level(s: &str) -> ThinkingConfig {
    let level = match s.trim().to_ascii_lowercase().as_str() {
        "low" => ReasoningEffort::Low,
        "high" => ReasoningEffort::High,
        _ => ReasoningEffort::Medium,
    };
    ThinkingConfig::Effort(level)
}

/// LLM vendor / backend selector.
///
/// ```yaml
/// llm:
///   vendor: openai_compat   # Ollama, vLLM, LM Studio, …
///   model: llama3.2
///   base_url: http://localhost:11434/v1
/// ```
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LlmVendor {
    /// Anthropic Messages API (default).  Uses `ANTHROPIC_API_KEY`.
    #[default]
    Anthropic,
    /// OpenAI Responses API (`/v1/responses`).  Uses `OPENAI_API_KEY`.
    OpenAi,
    /// OpenAI-compatible Chat Completions API (`/v1/chat/completions`).
    /// Suitable for Ollama, vLLM, LM Studio, and similar local/self-hosted
    /// backends.  Uses `OPENAI_API_KEY` as fallback (or pass `api_key`
    /// directly).
    OpenAiCompat,
}

/// LLM configuration section.
#[derive(Debug, Deserialize)]
pub struct LlmConfigYaml {
    /// Named model reference from the project's `config.yml`.
    ///
    /// When set, the vendor, api_key, and base_url of that model entry are used
    /// as defaults and any other fields in this section override them.
    ///
    /// ```yaml
    /// llm:
    ///   ref: openai-4o-mini   # model name from config.yml
    ///   model: gpt-5.4        # override the model_ref only
    /// ```
    #[serde(default, rename = "ref")]
    pub model_ref: Option<String>,

    /// Backend vendor.  Defaults to [`LlmVendor::Anthropic`].
    ///
    /// Overrides the vendor resolved from `ref` when both are set.
    #[serde(default)]
    pub vendor: LlmVendor,

    /// Model ID.  Overrides the `model_ref` resolved from `ref`.
    #[serde(default)]
    pub model: Option<String>,

    /// API key.  Falls back to `ANTHROPIC_API_KEY` (Anthropic) or
    /// `OPENAI_API_KEY` (OpenAi / OpenAiCompat) environment variables.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Base URL override.
    ///
    /// - Anthropic: proxy URL (default: `https://api.anthropic.com/v1/messages`)
    /// - OpenAi: Responses API base (default: `https://api.openai.com/v1/responses`)
    /// - OpenAiCompat: local server root, e.g. `http://localhost:11434/v1`
    #[serde(default)]
    pub base_url: Option<String>,

    /// Maximum output tokens per call.  Defaults to 4096.
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Default thinking/reasoning config for all pipeline states.
    ///
    /// Takes precedence over the top-level `thinking:` field on `AgentConfig`.
    ///
    /// ```yaml
    /// llm:
    ///   thinking: adaptive
    /// ```
    #[serde(default)]
    pub thinking: Option<ThinkingConfigYaml>,

    /// "Extended thinking" mode preset: an alternative model + thinking configuration
    /// activated at runtime via the UI toggle.
    ///
    /// ```yaml
    /// llm:
    ///   thinking: effort:low
    ///   extended_thinking:
    ///     model: gpt-5.4
    ///     thinking: effort:medium
    /// ```
    #[serde(default)]
    pub extended_thinking: Option<ExtendedThinkingConfigYaml>,
}

/// Extended thinking mode preset configuration.
///
/// Overrides the default model and/or thinking config when the user
/// activates "extended thinking" mode from the UI.
#[derive(Debug, Clone, Deserialize)]
pub struct ExtendedThinkingConfigYaml {
    /// Model ID override for extended thinking mode.
    #[serde(default)]
    pub model: Option<String>,
    /// Thinking config override for extended thinking mode.
    #[serde(default)]
    pub thinking: Option<ThinkingConfigYaml>,
}

impl Default for LlmConfigYaml {
    fn default() -> Self {
        Self {
            model_ref: None,
            vendor: LlmVendor::Anthropic,
            model: None,
            api_key: None,
            base_url: None,
            max_tokens: None,
            thinking: None,
            extended_thinking: None,
        }
    }
}

// ── Resolved context ──────────────────────────────────────────────────────────

/// Files bucketed by type after glob expansion.
#[derive(Debug, Default)]
pub struct ResolvedContext {
    /// `.view.yml` and `.topic.yml` files for the semantic catalog.
    pub semantic_files: Vec<PathBuf>,
    /// `.sql` example files to inject into the Solving prompt.
    pub sql_examples: Vec<String>,
    /// `.md` documentation files to inject into Clarifying and Interpreting.
    pub domain_docs: Vec<String>,
    /// `.procedure.yml` / `.procedure.yaml` files discovered via context globs.
    ///
    /// When non-empty, the HTTP layer wires an `OxyProcedureRunner` initialised
    /// with these paths so the `search_procedures` tool can locate them without
    /// scanning the entire project directory.
    pub procedure_files: Vec<PathBuf>,
    /// Database names discovered in context files:
    ///
    /// - `data_source:` field of `.view.yml` files
    /// - `/* oxy:\n    database: <name> */` header comment of `.sql` files
    ///
    /// Surfaced for the caller to validate or auto-wire project connectors.
    pub referenced_databases: Vec<String>,
}

// ── Context extraction helpers ─────────────────────────────────────────────────

/// Extract the datasource name from a view YAML file without performing a full parse.
///
/// Accepts both `datasource` (airlayer convention) and `data_source` (legacy).
fn extract_view_data_source(content: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct ViewMeta {
        datasource: Option<String>,
        data_source: Option<String>,
    }
    serde_yaml::from_str::<ViewMeta>(content)
        .ok()
        .and_then(|v| v.datasource.or(v.data_source))
        .filter(|s| !s.is_empty())
}

/// Extract all `database:` values from an `execute_sql` task in a procedure YAML file.
///
/// Recursively walks the YAML value tree so that databases nested inside
/// `loop_sequential` (or any other nested task container) are also discovered.
fn extract_procedure_databases(content: &str) -> Vec<String> {
    fn collect(val: &serde_yaml::Value, out: &mut Vec<String>) {
        match val {
            serde_yaml::Value::Mapping(m) => {
                if let Some(serde_yaml::Value::String(db)) =
                    m.get(serde_yaml::Value::String("database".into()))
                    && !db.is_empty()
                    && !out.contains(db)
                {
                    out.push(db.clone());
                }
                for (_, v) in m {
                    collect(v, out);
                }
            }
            serde_yaml::Value::Sequence(seq) => {
                for item in seq {
                    collect(item, out);
                }
            }
            _ => {}
        }
    }
    let mut databases = Vec::new();
    if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(content) {
        collect(&val, &mut databases);
    }
    databases
}

/// Extract the `database` value from the `oxy:` comment block in a SQL file.
///
/// Recognises the format:
/// ```sql
/// /*
///   oxy:
///     database: my_db
/// */
/// ```
fn extract_sql_oxy_database(content: &str) -> Option<String> {
    let start = content.find("/*")?;
    let end_offset = content[start..].find("*/")?;
    let comment = &content[start + 2..start + end_offset];

    #[derive(serde::Deserialize)]
    struct OxyBlock {
        database: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct OxyComment {
        oxy: Option<OxyBlock>,
    }
    serde_yaml::from_str::<OxyComment>(comment)
        .ok()
        .and_then(|c| c.oxy)
        .and_then(|o| o.database)
        .filter(|s| !s.is_empty())
}

/// Push `name` into `list` if it is not already present.
fn push_unique(list: &mut Vec<String>, name: String) {
    if !list.contains(&name) {
        list.push(name);
    }
}

// ── BuildContext ──────────────────────────────────────────────────────────────

/// Project-level context passed to [`AgentConfig::build_solver_with_context`].
///
/// Constructed by the HTTP layer from `WorkspaceManager` so that
/// `agentic-analytics` itself does not need a hard dependency on the `oxy`
/// crate.
#[derive(Default)]
pub struct BuildContext {
    /// Pre-built database connectors keyed by logical name.
    ///
    /// Merged with connectors built from the inline `databases:` entries.
    /// Intended to carry connectors resolved from `databases:` names.
    pub extra_connectors: HashMap<String, Arc<dyn agentic_connector::DatabaseConnector>>,
    /// Name of the connector to treat as default when no inline `databases:`
    /// entry has `default: true`.  Only consulted when `extra_connectors` is
    /// non-empty.
    pub extra_default_connector: Option<String>,
    /// Resolved model ref (e.g. `"claude-opus-4-6"`, `"gpt-4.1"`) from the
    /// project `config.yml` model entry.  Applied when `llm.model` is absent
    /// in the agent YAML.
    pub project_model: Option<String>,
    /// Resolved API key from the model's `key_var` in `config.yml`.
    ///
    /// Applied when `llm.api_key` is absent in the agent YAML.
    pub project_api_key: Option<String>,
    /// LLM vendor resolved from the project model config.
    ///
    /// Applied when `llm.vendor` is default (Anthropic) and `llm.model` is
    /// absent in the agent YAML.
    pub project_vendor: Option<LlmVendor>,
    /// Base URL resolved from the project model config (e.g. Ollama's
    /// `api_url` or a custom OpenAI-compat endpoint).
    ///
    /// Applied when `llm.base_url` is absent in the agent YAML.
    pub project_base_url: Option<String>,
    /// Optional schema cache shared across requests.
    ///
    /// Keyed by connector name.  When set, `build_solver_with_context` checks
    /// the cache before calling `introspect_schema` and stores any freshly
    /// introspected result back into the cache.
    pub schema_cache: Option<Arc<Mutex<HashMap<String, SchemaCatalog>>>>,

    /// True when the project model context (`project_model`, `project_vendor`,
    /// etc.) was populated from an explicit `llm.ref:` in the agent YAML rather
    /// than from the project default model fallback.
    ///
    /// Used by `build_solver_with_context` to decide vendor precedence: when a
    /// `ref` is set the ref's vendor is preferred even if `llm.model` is also
    /// explicitly overridden.
    pub has_explicit_ref: bool,

    /// Runtime thinking config override (from UI "extended thinking" mode toggle).
    ///
    /// When set, overrides the YAML-configured global thinking config.
    /// Per-state thinking overrides still take precedence.
    pub thinking_override: Option<ThinkingConfig>,

    /// Runtime model override (from UI "extended thinking" mode toggle).
    ///
    /// When set, a new LLM client is built with this model ID (inheriting
    /// vendor, api_key, and base_url from the resolved config).
    pub model_override: Option<String>,
}

// ── AgentConfig methods ───────────────────────────────────────────────────────

// ── Schema introspection ──────────────────────────────────────────────────────

/// Introspect the database schema through the already-built connector,
/// tagging every table with `connector_name` for multi-connector routing.
///
/// The connector's [`introspect_schema`] method is vendor-agnostic: each
/// connector implementation knows how to query its own system catalog.
/// On failure (or when the connector does not implement introspection) an
/// empty [`SchemaCatalog`] is returned so the solver degrades gracefully.
///
/// [`introspect_schema`]: agentic_connector::DatabaseConnector::introspect_schema
fn build_schema_named(
    connector: &dyn agentic_connector::DatabaseConnector,
    connector_name: &str,
) -> SchemaCatalog {
    match connector.introspect_schema() {
        Ok(info) => SchemaCatalog::from_schema_info_named(&info, connector_name),
        Err(e) => {
            tracing::warn!(connector = connector_name, error = %e, "schema introspection failed, using empty schema");
            SchemaCatalog::default()
        }
    }
}

// ── Engine factory ────────────────────────────────────────────────────────────

/// Build a bundled [`SemanticEngine`] adapter from the YAML `semantic_engine` block.
///
/// Covers the two bundled adapters (Cube, Looker).  External engines are
/// supplied programmatically via [`AnalyticsSolverBuilder::engine_arc`] and
/// never pass through this factory.
///
/// Does **not** call `ping()` — the caller is responsible for the startup
/// health-check so it can map [`EngineError::EngineUnreachable`] to
/// [`ConfigError::EngineConnectionError`].
fn build_engine(cfg: &SemanticEngineConfig) -> Result<Box<dyn SemanticEngine>, ConfigError> {
    match cfg.vendor {
        VendorKind::Cube => {
            let token = cfg.resolved_api_token()?;
            Ok(Box::new(CubeEngine::new(cfg.base_url.clone(), token)))
        }
        VendorKind::Looker => {
            let client_id = cfg.resolved_client_id()?;
            let client_secret = cfg.resolved_client_secret()?;
            Ok(Box::new(LookerEngine::new(
                cfg.base_url.clone(),
                client_id,
                client_secret,
            )))
        }
    }
}

/// Build an [`LlmClient`] from the resolved vendor / key / model / base_url.
///
/// Extracted so it can be called both for the global client and for per-state
/// model overrides (which inherit vendor, key, and base_url).
fn build_llm_client(
    vendor: &LlmVendor,
    api_key: &str,
    model: &str,
    base_url: Option<&str>,
) -> LlmClient {
    match vendor {
        LlmVendor::Anthropic => LlmClient::with_model(api_key, model),
        LlmVendor::OpenAi => {
            let provider = if let Some(url) = base_url {
                OpenAiProvider::with_base_url(api_key, model, url)
            } else {
                OpenAiProvider::new(api_key, model)
            };
            LlmClient::with_provider(provider)
        }
        LlmVendor::OpenAiCompat => {
            let url = base_url.unwrap_or("http://localhost:11434/v1");
            LlmClient::with_provider(OpenAiCompatProvider::new(api_key, model, url))
        }
    }
}

impl AgentConfig {
    /// Parse an [`AgentConfig`] from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, ConfigError> {
        Ok(serde_yaml::from_str(yaml)?)
    }

    /// Load an [`AgentConfig`] from a YAML file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// Glob-expand `context` patterns and bucket files by extension.
    ///
    /// Patterns are resolved relative to `base_dir`.
    pub fn resolve_context(&self, base_dir: &Path) -> Result<ResolvedContext, ConfigError> {
        let mut ctx = ResolvedContext::default();

        for pattern in &self.context {
            // Make the pattern absolute by prepending base_dir when relative.
            let abs_pattern = if Path::new(pattern).is_absolute() {
                pattern.clone()
            } else {
                base_dir.join(pattern).to_string_lossy().into_owned()
            };

            for entry in glob::glob(&abs_pattern)? {
                let path = entry.map_err(|e| ConfigError::Io(e.into_error()))?;
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                if name.ends_with(".procedure.yml") || name.ends_with(".procedure.yaml") {
                    let content = std::fs::read_to_string(&path).map_err(ConfigError::Io)?;
                    for db in extract_procedure_databases(&content) {
                        push_unique(&mut ctx.referenced_databases, db);
                    }
                    ctx.procedure_files.push(path);
                } else if name.ends_with(".view.yml") || name.ends_with(".view.yaml") {
                    // Quick parse to surface the data_source before full catalog load.
                    let content = std::fs::read_to_string(&path).map_err(ConfigError::Io)?;
                    if let Some(db) = extract_view_data_source(&content) {
                        push_unique(&mut ctx.referenced_databases, db);
                    }
                    ctx.semantic_files.push(path);
                } else if name.ends_with(".topic.yml") || name.ends_with(".topic.yaml") {
                    ctx.semantic_files.push(path);
                } else if path.extension().is_some_and(|e| e == "sql") {
                    let content = std::fs::read_to_string(&path).map_err(ConfigError::Io)?;
                    if let Some(db) = extract_sql_oxy_database(&content) {
                        push_unique(&mut ctx.referenced_databases, db);
                    }
                    ctx.sql_examples.push(content);
                } else if path.extension().is_some_and(|e| e == "md") {
                    let content = std::fs::read_to_string(&path).map_err(ConfigError::Io)?;
                    ctx.domain_docs.push(content);
                }
                // Other extensions are silently ignored.
            }
        }

        Ok(ctx)
    }

    /// Build a fully configured [`AnalyticsSolver`] using only the inline config.
    ///
    /// Delegates to [`Self::build_solver_with_context`] with a default
    /// [`BuildContext`].  Use `build_solver_with_context` directly when you
    /// want to inject project-level databases or model overrides.
    pub async fn build_solver(
        &self,
        base_dir: &Path,
    ) -> Result<(AnalyticsSolver, Vec<PathBuf>), ConfigError> {
        self.build_solver_with_context(base_dir, BuildContext::default())
            .await
    }

    /// Build a fully configured [`AnalyticsSolver`] using connectors supplied
    /// by the HTTP layer via [`BuildContext`].
    ///
    /// # Steps
    ///
    /// 1. Resolve context globs → [`ResolvedContext`].
    /// 2. Load semantic files → `Option<SemanticCatalog>`.
    /// 3. Merge [`BuildContext::extra_connectors`] and their introspected schemas.
    /// 4. Build [`SemanticCatalog`] from semantic + merged schema.
    /// 5. Build [`LlmClient`] with project/env-var fallbacks.
    /// 6. Construct [`AnalyticsSolver`] with context and global thinking.
    ///
    /// Returns `(solver, procedure_files)` where `procedure_files` is the list
    /// of `.procedure.yml` paths discovered via `context` globs.
    pub async fn build_solver_with_context(
        &self,
        base_dir: &Path,
        build_ctx: BuildContext,
    ) -> Result<(AnalyticsSolver, Vec<PathBuf>), ConfigError> {
        // 1. Resolve context.
        let ctx = self.resolve_context(base_dir)?;
        // 2. Merge connectors injected by the HTTP layer from `databases:` names.
        if build_ctx.extra_connectors.is_empty() {
            return Err(ConfigError::NoDatabases);
        }

        let mut connectors: HashMap<String, Arc<dyn agentic_connector::DatabaseConnector>> =
            HashMap::new();

        for (name, connector) in build_ctx.extra_connectors {
            connectors.insert(name, connector);
        }

        let default_connector = build_ctx
            .extra_default_connector
            .or_else(|| connectors.keys().next().cloned())
            .unwrap_or_default();

        // 3. Load semantic catalog (after connectors so we have dialect info).
        //    Schema introspection is no longer done eagerly — the LLM uses
        //    `list_tables` / `describe_table` tools on demand.
        let dialect_map =
            crate::airlayer_compat::build_dialect_map(&connectors, &default_connector);
        let catalog = if ctx.semantic_files.is_empty() {
            SemanticCatalog::empty()
        } else {
            SemanticCatalog::load_files(&ctx.semantic_files, dialect_map)
                .map_err(ConfigError::SemanticError)?
        };

        // 5. Build LLM client.
        //
        // Precedence (highest → lowest):
        //   a. Explicit `llm:` fields in the agent YAML.
        //   b. Project model resolved from config.yml via BuildContext.
        //   c. Environment variables / built-in defaults.
        //
        // When `llm.model` is absent in the YAML the project model config
        // (vendor, model_ref, api_key, base_url) is used wholesale.
        let model = self
            .llm
            .model
            .as_deref()
            .or(build_ctx.project_model.as_deref())
            .unwrap_or(DEFAULT_MODEL)
            .to_string();

        // Vendor precedence:
        //   - When an explicit `llm.ref:` was supplied, the ref's vendor is the
        //     base; `llm.vendor` can still override it but only when the yaml
        //     also sets `llm.vendor` explicitly. Since we can't distinguish
        //     "user set anthropic" from "defaulted to anthropic", the practical
        //     rule is: ref vendor wins when a ref is present.
        //   - Without a ref, the yaml vendor wins when `llm.model` is set
        //     (user is picking a specific model, they own the vendor too);
        //     otherwise fall back to the project default vendor.
        let effective_vendor = if build_ctx.has_explicit_ref {
            build_ctx
                .project_vendor
                .as_ref()
                .unwrap_or(&self.llm.vendor)
        } else if self.llm.model.is_some() {
            &self.llm.vendor
        } else {
            build_ctx
                .project_vendor
                .as_ref()
                .unwrap_or(&self.llm.vendor)
        };

        // API key: YAML → project resolved key → vendor env var.
        let api_key = self
            .llm
            .api_key
            .clone()
            .or(build_ctx.project_api_key)
            .or_else(|| match effective_vendor {
                LlmVendor::Anthropic => std::env::var("ANTHROPIC_API_KEY").ok(),
                LlmVendor::OpenAi | LlmVendor::OpenAiCompat => std::env::var("OPENAI_API_KEY").ok(),
            })
            .unwrap_or_default();

        // Base URL: YAML → project resolved URL.
        let effective_base_url = self
            .llm
            .base_url
            .as_deref()
            .or(build_ctx.project_base_url.as_deref());

        let client = build_llm_client(effective_vendor, &api_key, &model, effective_base_url);

        // Build per-state clients for states that declare a `model:` override.
        // Inherits vendor / api_key / base_url from the global config.
        let state_clients: std::collections::HashMap<String, LlmClient> = self
            .states
            .iter()
            .filter_map(|(state_name, state_cfg)| {
                state_cfg.model.as_deref().map(|state_model| {
                    let c = build_llm_client(
                        effective_vendor,
                        &api_key,
                        state_model,
                        effective_base_url,
                    );
                    (state_name.clone(), c)
                })
            })
            .collect();

        // 6. Build validator from config (defaults to all rules enabled).
        let validator = match &self.validation {
            Some(cfg) => Validator::from_config(cfg)
                .map_err(|e| ConfigError::ValidationError(e.to_string()))?,
            None => Validator::default_validator(),
        };

        // 7. Build and health-check the vendor engine (if configured).
        let engine: Option<Arc<dyn SemanticEngine>> =
            if let Some(engine_cfg) = &self.semantic_engine {
                let engine = build_engine(engine_cfg)?;
                engine.ping().await.map_err(|e| match e {
                    EngineError::EngineUnreachable(msg) => ConfigError::EngineConnectionError(msg),
                    other => ConfigError::EngineConnectionError(other.to_string()),
                })?;
                Some(Arc::from(engine))
            } else {
                None
            };

        // 8. Construct solver with all connectors.
        let mut solver = AnalyticsSolver::new_multi(client, catalog, connectors, default_connector)
            .with_global_instructions(self.instructions.clone())
            .with_sql_examples(ctx.sql_examples)
            .with_domain_docs(ctx.domain_docs)
            .with_state_configs(self.states.clone())
            .with_state_clients(state_clients)
            .with_validator(validator)
            .with_max_tokens(self.llm.max_tokens);

        // Wire global thinking config: llm.thinking > top-level thinking.
        let effective_thinking = self.llm.thinking.as_ref().or(self.thinking.as_ref());
        if let Some(thinking_cfg) = effective_thinking {
            solver = solver.with_global_thinking(thinking_cfg.to_thinking_config());
        }

        // Apply runtime overrides from the UI "extended thinking" mode toggle.
        // These override *all* config — including per-state thinking and
        // model overrides — so the extended thinking preset wins unconditionally.
        let extended_thinking_active =
            build_ctx.thinking_override.is_some() || build_ctx.model_override.is_some();
        if let Some(thinking) = build_ctx.thinking_override {
            solver = solver.with_global_thinking(thinking);
        }
        if let Some(ref override_model) = build_ctx.model_override {
            let override_client = build_llm_client(
                effective_vendor,
                &api_key,
                override_model,
                effective_base_url,
            );
            solver = solver.with_client_override(override_client);
        }
        // `with_client_override` already sets `extended_thinking_active = true`
        // when a model override is present.  Handle the case where only a
        // thinking override is provided (no model override).
        if extended_thinking_active && !solver.extended_thinking_active {
            solver.extended_thinking_active = true;
        }

        if let Some(e) = engine {
            solver = solver.with_engine(e);
        }

        Ok((solver, ctx.procedure_files))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn write_temp(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    // ── llm.ref parsing ───────────────────────────────────────────────────────

    #[test]
    fn llm_ref_parses_correctly() {
        let yaml = "llm:\n  ref: openai-4o-mini\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.llm.model_ref.as_deref(), Some("openai-4o-mini"));
        assert!(config.llm.model.is_none());
    }

    #[test]
    fn llm_ref_with_model_override() {
        let yaml = "llm:\n  ref: openai-4o-mini\n  model: gpt-5.4\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.llm.model_ref.as_deref(), Some("openai-4o-mini"));
        assert_eq!(config.llm.model.as_deref(), Some("gpt-5.4"));
    }

    #[test]
    fn llm_ref_absent_by_default() {
        let yaml = "llm:\n  model: gpt-4\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        assert!(config.llm.model_ref.is_none());
    }

    #[test]
    fn llm_ref_empty_config_defaults() {
        let config = AgentConfig::from_yaml("{}").unwrap();
        assert!(config.llm.model_ref.is_none());
        assert!(config.llm.model.is_none());
    }

    // ── extract_procedure_databases ───────────────────────────────────────────

    #[test]
    fn procedure_databases_flat_tasks() {
        let yaml = r#"
name: my_proc
tasks:
  - name: q1
    type: execute_sql
    database: warehouse
    sql_query: SELECT 1
  - name: q2
    type: execute_sql
    database: staging
    sql_query: SELECT 2
"#;
        let mut dbs = extract_procedure_databases(yaml);
        dbs.sort();
        assert_eq!(dbs, vec!["staging", "warehouse"]);
    }

    #[test]
    fn procedure_databases_nested_loop_sequential() {
        let yaml = r#"
name: my_proc
tasks:
  - name: loop_step
    type: loop_sequential
    values: [1, 2, 3]
    tasks:
      - name: inner_query
        type: execute_sql
        database: local
        sql_query: SELECT 1
"#;
        let dbs = extract_procedure_databases(yaml);
        assert_eq!(dbs, vec!["local"]);
    }

    #[test]
    fn procedure_databases_deduplication() {
        let yaml = r#"
name: my_proc
tasks:
  - name: q1
    type: execute_sql
    database: local
    sql_query: SELECT 1
  - name: loop_step
    type: loop_sequential
    values: [1, 2]
    tasks:
      - name: q2
        type: execute_sql
        database: local
        sql_query: SELECT 2
"#;
        let dbs = extract_procedure_databases(yaml);
        assert_eq!(dbs, vec!["local"]);
    }

    #[test]
    fn procedure_databases_no_execute_sql() {
        let yaml = r#"
name: my_proc
tasks:
  - name: fmt
    type: formatter
    template: "hello"
"#;
        let dbs = extract_procedure_databases(yaml);
        assert!(dbs.is_empty());
    }

    #[test]
    fn procedure_databases_multiple_nested_levels() {
        // database appears at top-level task and inside a nested loop
        let yaml = r#"
name: p
tasks:
  - name: top
    type: execute_sql
    database: alpha
    sql_query: SELECT 1
  - name: outer_loop
    type: loop_sequential
    values: [1]
    tasks:
      - name: inner_loop
        type: loop_sequential
        values: [1]
        tasks:
          - name: deep
            type: execute_sql
            database: beta
            sql_query: SELECT 2
"#;
        let mut dbs = extract_procedure_databases(yaml);
        dbs.sort();
        assert_eq!(dbs, vec!["alpha", "beta"]);
    }

    // ── extract_sql_oxy_database ──────────────────────────────────────────────

    #[test]
    fn sql_oxy_database_present() {
        let sql = "/*\n  oxy:\n    database: local\n    embed:\n      - How many stores\n*/\nSELECT COUNT(*) FROM stores;";
        assert_eq!(extract_sql_oxy_database(sql), Some("local".to_string()));
    }

    #[test]
    fn sql_oxy_no_comment() {
        assert_eq!(extract_sql_oxy_database("SELECT 1;"), None);
    }

    #[test]
    fn sql_oxy_comment_without_database() {
        let sql = "/*\n  oxy:\n    embed:\n      - How many stores\n*/\nSELECT 1;";
        assert_eq!(extract_sql_oxy_database(sql), None);
    }

    // ── resolve_context — database inference ──────────────────────────────────

    #[test]
    fn resolve_context_infers_db_from_sql_oxy_comment() {
        let tmp = std::env::temp_dir().join("oxy_cfg_test_sql");
        fs::create_dir_all(&tmp).unwrap();
        write_temp(
            &tmp,
            "q.sql",
            "/*\n  oxy:\n    database: analytics\n*/\nSELECT 1;",
        );

        let config = AgentConfig::from_yaml("context:\n  - '*.sql'\n").unwrap();
        let ctx = config.resolve_context(&tmp).unwrap();

        assert!(
            ctx.referenced_databases.contains(&"analytics".to_string()),
            "expected 'analytics' in referenced_databases, got: {:?}",
            ctx.referenced_databases
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn resolve_context_infers_db_from_procedure_file() {
        let tmp = std::env::temp_dir().join("oxy_cfg_test_proc");
        fs::create_dir_all(&tmp).unwrap();
        write_temp(
            &tmp,
            "my.procedure.yml",
            "name: p\ntasks:\n  - name: q\n    type: execute_sql\n    database: warehouse\n    sql_query: SELECT 1\n",
        );

        let config = AgentConfig::from_yaml("context:\n  - '*.procedure.yml'\n").unwrap();
        let ctx = config.resolve_context(&tmp).unwrap();

        assert!(
            ctx.referenced_databases.contains(&"warehouse".to_string()),
            "expected 'warehouse' in referenced_databases, got: {:?}",
            ctx.referenced_databases
        );
        assert_eq!(ctx.procedure_files.len(), 1);

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn resolve_context_infers_db_from_nested_procedure_loop() {
        let tmp = std::env::temp_dir().join("oxy_cfg_test_nested");
        fs::create_dir_all(&tmp).unwrap();
        write_temp(
            &tmp,
            "deep.procedure.yml",
            "name: p\ntasks:\n  - name: outer\n    type: loop_sequential\n    values: [1]\n    tasks:\n      - name: q\n        type: execute_sql\n        database: remote\n        sql_query: SELECT 1\n",
        );

        let config = AgentConfig::from_yaml("context:\n  - '*.procedure.yml'\n").unwrap();
        let ctx = config.resolve_context(&tmp).unwrap();

        assert!(
            ctx.referenced_databases.contains(&"remote".to_string()),
            "expected 'remote', got: {:?}",
            ctx.referenced_databases
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn resolve_context_deduplicates_databases_across_files() {
        let tmp = std::env::temp_dir().join("oxy_cfg_test_dedup");
        fs::create_dir_all(&tmp).unwrap();
        write_temp(
            &tmp,
            "q1.sql",
            "/*\n  oxy:\n    database: local\n*/\nSELECT 1;",
        );
        write_temp(
            &tmp,
            "q2.sql",
            "/*\n  oxy:\n    database: local\n*/\nSELECT 2;",
        );
        write_temp(
            &tmp,
            "proc.procedure.yml",
            "name: p\ntasks:\n  - name: q\n    type: execute_sql\n    database: local\n    sql_query: SELECT 3\n",
        );

        let config =
            AgentConfig::from_yaml("context:\n  - '*.sql'\n  - '*.procedure.yml'\n").unwrap();
        let ctx = config.resolve_context(&tmp).unwrap();

        let count = ctx
            .referenced_databases
            .iter()
            .filter(|d| *d == "local")
            .count();
        assert_eq!(
            count, 1,
            "database names should be deduplicated; got: {:?}",
            ctx.referenced_databases
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn resolve_context_merges_databases_from_sql_and_procedure() {
        let tmp = std::env::temp_dir().join("oxy_cfg_test_merge");
        fs::create_dir_all(&tmp).unwrap();
        write_temp(
            &tmp,
            "q.sql",
            "/*\n  oxy:\n    database: alpha\n*/\nSELECT 1;",
        );
        write_temp(
            &tmp,
            "proc.procedure.yml",
            "name: p\ntasks:\n  - name: q\n    type: execute_sql\n    database: beta\n    sql_query: SELECT 1\n",
        );

        let config =
            AgentConfig::from_yaml("context:\n  - '*.sql'\n  - '*.procedure.yml'\n").unwrap();
        let ctx = config.resolve_context(&tmp).unwrap();

        let mut dbs = ctx.referenced_databases.clone();
        dbs.sort();
        assert_eq!(dbs, vec!["alpha", "beta"]);

        fs::remove_dir_all(&tmp).ok();
    }

    // ── extended_thinking config parsing ────────────────────────────────────

    #[test]
    fn extended_thinking_config_parses_anthropic() {
        let yaml = r#"
llm:
  ref: claude-sonnet-4-6
  thinking: adaptive
  extended_thinking:
    model: claude-opus-4-6
    thinking: adaptive
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let et = config.llm.extended_thinking.unwrap();
        assert_eq!(et.model.as_deref(), Some("claude-opus-4-6"));
        assert!(matches!(
            et.thinking.unwrap().to_thinking_config(),
            ThinkingConfig::Adaptive
        ));
    }

    #[test]
    fn extended_thinking_config_parses_openai_effort() {
        let yaml = r#"
llm:
  ref: openai
  model: gpt-5.4
  thinking: effort::low
  extended_thinking:
    model: gpt-5.4
    thinking: effort::medium
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let et = config.llm.extended_thinking.unwrap();
        assert!(matches!(
            et.thinking.unwrap().to_thinking_config(),
            ThinkingConfig::Effort(ReasoningEffort::Medium)
        ));
    }

    #[test]
    fn thinking_in_llm_takes_precedence_over_top_level() {
        let yaml = r#"
thinking: disabled
llm:
  thinking: adaptive
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        // llm.thinking should take precedence over top-level thinking
        let effective = config.llm.thinking.as_ref().or(config.thinking.as_ref());
        assert!(matches!(
            effective.unwrap().to_thinking_config(),
            ThinkingConfig::Adaptive
        ));
    }

    #[test]
    fn top_level_thinking_used_when_llm_thinking_absent() {
        let yaml = "thinking: adaptive\nllm:\n  model: test\n";
        let config = AgentConfig::from_yaml(yaml).unwrap();
        assert!(config.llm.thinking.is_none());
        let effective = config.llm.thinking.as_ref().or(config.thinking.as_ref());
        assert!(matches!(
            effective.unwrap().to_thinking_config(),
            ThinkingConfig::Adaptive
        ));
    }

    #[test]
    fn extended_thinking_absent_when_not_configured() {
        let config = AgentConfig::from_yaml("{}").unwrap();
        assert!(config.llm.extended_thinking.is_none());
    }

    #[test]
    fn extended_thinking_with_only_model_override() {
        let yaml = r#"
llm:
  extended_thinking:
    model: claude-opus-4-6
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let et = config.llm.extended_thinking.unwrap();
        assert_eq!(et.model.as_deref(), Some("claude-opus-4-6"));
        assert!(et.thinking.is_none());
    }

    #[test]
    fn extended_thinking_with_only_thinking_override() {
        let yaml = r#"
llm:
  extended_thinking:
    thinking: effort::high
"#;
        let config = AgentConfig::from_yaml(yaml).unwrap();
        let et = config.llm.extended_thinking.unwrap();
        assert!(et.model.is_none());
        assert!(matches!(
            et.thinking.unwrap().to_thinking_config(),
            ThinkingConfig::Effort(ReasoningEffort::High)
        ));
    }
}
