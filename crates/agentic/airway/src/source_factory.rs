//! Dispatch a [`SourceConfig`] into a concrete [`airway::SourceConnector`].
//!
//! The YAML schema is intentionally open: any `kind` string is
//! permitted at parse time, and dispatch happens here. Adding support
//! for a new airway source means adding one arm to
//! [`build_source_connector`]; the YAML surface stays unchanged.
//!
//! v1 wires up the generic airway sources (`rest_api`,
//! `filesystem`, `sql_database`, `clickhouse`, `postgres_cdc`). The
//! `clickhouse` arm targets ClickHouse's HTTP interface (JSONEachRow)
//! via airway's `SqlDatabaseSource::clickhouse` convenience
//! constructor — it carries discrete connection fields rather than a
//! single connection string, so it gets its own `kind` instead of
//! riding the `sql_database` backend enum. The vendor-specific
//! helpers (`shopify`, `github`, `stripe`, …) all build on top of
//! `RestApiSource` upstream — most can be expressed directly as a
//! `rest_api` config, so we defer wiring per-vendor sugar until there's
//! a real consumer asking for it.
//!
//! [`SourceConfig`]: crate::config::SourceConfig

use std::collections::BTreeMap;
use std::sync::Arc;

use airway::connector::SourceConnector;
use airway::connector::sources::besttime::{BestTimeConfig, besttime_source};
use airway::connector::sources::filesystem::{FilesystemSource, SourceFileFormat};
use airway::connector::sources::http_file::{HttpFileConfig, http_file_source};
use airway::connector::sources::overpass::{OverpassConfig, overpass_source};
use airway::connector::sources::overture::{OvertureConfig, overture_source};
use airway::connector::sources::postgres_cdc::PostgresCdcSource;
use airway::connector::sources::quickbooks::{QuickBooksSource, SANDBOX_BASE_URL};
use airway::connector::sources::rest_api::{RestApiConfig, RestApiSource};
use airway::connector::sources::sql_database::{
    ClickHouseConn, DatabaseBackend, SqlDatabaseSource, TableConfig,
};
// Re-exported so `agentic-pipeline` / `agentic-http` can shape the
// discovery response without taking a direct `airway` dependency.
pub use airway::connector::sources::sql_database::{DiscoveredColumn, DiscoveredTable};
use airway::connector::sources::toast::ToastSource;
use airway::connector::sources::weather::{WeatherConfig, weather_source};
use airway::types::WriteDisposition;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::config::SourceConfig;
use crate::error::AirwayError;

/// Write-back port for a rotated OAuth refresh token (QuickBooks).
///
/// Implemented by the host (`agentic-pipeline`'s executor) so the rotated
/// token can be persisted to the host's secret store between runs. Uses a
/// `String` error so implementors don't take a dependency on the `airway`
/// crate's error type — [`build_quickbooks`] bridges this to airway's own
/// `RefreshTokenSink` ([`AirwayRefreshSink`]).
#[async_trait]
pub trait RefreshTokenSink: Send + Sync {
    async fn persist(&self, refresh_token: &str) -> Result<(), String>;
}

/// Bridges a host [`RefreshTokenSink`] (String error) onto airway's
/// `RefreshTokenSink` (AirwayError), so the engine can drive it.
struct AirwayRefreshSink(Arc<dyn RefreshTokenSink>);

#[async_trait]
impl airway::connector::sources::quickbooks::RefreshTokenSink for AirwayRefreshSink {
    async fn persist(&self, refresh_token: &str) -> Result<(), airway::AirwayError> {
        self.0
            .persist(refresh_token)
            .await
            .map_err(airway::AirwayError::Extract)
    }
}

/// Read port for a host-maintained OAuth **access** token (QuickBooks).
///
/// The counterpart to [`RefreshTokenSink`], and mutually exclusive with it.
/// Supplying one puts the connector in read-only mode: it never calls Intuit's
/// token endpoint, because Intuit expires the previous refresh token whenever
/// it issues a new one, so a grant tolerates exactly one rotation writer. When
/// the host already runs that writer (for Poke House, a scheduled
/// `refresh-qb-token` Oxy Function), this pipeline must be a reader.
///
/// Implemented by `agentic-pipeline`'s executor over the secret store. Uses a
/// `String` error so implementors need no `airway` dependency —
/// [`build_quickbooks`] bridges it via [`AirwayAccessTokenSource`].
///
/// **Called per request, not once per run.** The executor resolves ordinary
/// `*_var` secrets once, before dispatch; doing that here would pin a ~60-minute
/// access token for a backfill that outlives it. This port is invoked on demand
/// so each call re-reads whatever the refresher last wrote.
///
/// Concretely, in airway ≥ 0.1.25: `QuickBooksAuth::authorization_header` is
/// called by every data-API request, and in read-only mode it returns from this
/// port before touching the token cache — nothing is memoised engine-side
/// (`readonly_mode_reasks_the_source_on_every_call` pins that). The only cache
/// in the path is `SecretManagerService`'s 300s TTL, so a rotation is picked up
/// within 5 minutes. That window is safe rather than merely tolerable: Poke
/// House's refresher fires every ~50 minutes against a 60-minute token, so the
/// value being served during it still has ~10 minutes of validity left.
#[async_trait]
pub trait AccessTokenSource: Send + Sync {
    async fn access_token(&self) -> Result<String, String>;
}

/// Which side of a QuickBooks grant's token custody this run takes.
///
/// An enum rather than two `Option`s because the two are mutually exclusive and
/// "both supplied" has no correct meaning — it would have to resolve to a
/// silent precedence rule, and the wrong branch of that rule is precisely the
/// failure this type exists to prevent (a second refresher forking the grant's
/// rotation chain). `None` at the call site means the source needs no token
/// hook at all, which is every non-QuickBooks source.
#[derive(Clone)]
pub enum QuickBooksTokens {
    /// This connector owns rotation: it refreshes at Intuit and writes the
    /// rotated refresh token back through the sink.
    Rotating(Arc<dyn RefreshTokenSink>),
    /// The host owns rotation: this connector only ever reads access tokens
    /// and never contacts Intuit's token endpoint.
    ReadOnly(Arc<dyn AccessTokenSource>),
}

/// Bridges a host [`AccessTokenSource`] (String error) onto airway's
/// `AccessTokenSource` (AirwayError), so the engine can drive it.
struct AirwayAccessTokenSource(Arc<dyn AccessTokenSource>);

#[async_trait]
impl airway::connector::sources::quickbooks::AccessTokenSource for AirwayAccessTokenSource {
    async fn access_token(&self) -> Result<String, airway::AirwayError> {
        self.0
            .access_token()
            .await
            .map_err(airway::AirwayError::Extract)
    }
}

/// Build the concrete [`SourceConnector`] for a parsed source config.
///
/// Returns a boxed trait object so the worker can hand it straight to
/// `airway::connector::parallel::extract_*` without committing to a
/// specific connector type at the worker layer.
///
/// `refresh_sink` is an optional write-back hook for OAuth refresh tokens
/// that the host supplies (only the `quickbooks` source consumes it; all
/// other arms ignore it). It lets a rotated refresh token be persisted to
/// the host's secret store between runs.
///
/// `environment` is the deployment-wide vendor-environment intent (see
/// [`resolve_quickbooks_base_url`]); only the `quickbooks` arm consumes it
/// today — every other arm ignores it, since QuickBooks is the only
/// connector currently declaring a sandbox host.
pub fn build_source_connector(
    config: &SourceConfig,
    tokens: Option<QuickBooksTokens>,
    environment: airway::connector::Environment,
) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let (connector, applied) = build_source_connector_inner(config, tokens, environment)?;
    admit_environment_is_applied(config, environment, connector.as_ref(), applied)?;
    Ok(connector)
}

/// Whether the factory arm that built this connector resolved its base URL
/// from the run's [`Environment`](airway::connector::Environment).
///
/// The marker exists so [`admit_environment_is_applied`] can ask *"did an arm
/// apply it?"* instead of naming a connector kind. A kind named over in the
/// guard is a claim stored apart from the code that makes it true: it keeps
/// reading "handled" after the arm stops handling it, and it has to be
/// remembered again for every arm added later.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EnvironmentApplied {
    /// The arm read `environment` and resolved this connector's base URL from
    /// it (including honouring an explicit per-source override).
    Yes,
    /// The arm ignored `environment`. Correct exactly while the connector
    /// declares no sandbox host — which is what the guard verifies.
    No,
}

/// Refuse a `sandbox` run whose connector declares a sandbox host that this
/// factory has no arm to apply.
///
/// Airway's `admit_with` checks that a connector *supports* the environment;
/// applying it is oxy's job, because airway resolves each source's sandbox host
/// from `Environment::installed()` — a process-wide global oxy never installs.
/// Today only the `quickbooks` arm applies one, and that is complete, since
/// QuickBooks is the sole connector in the pinned airway declaring
/// `sandbox_base_url()`. But the next airway bump that adds a host to any other
/// connector would otherwise produce the exact silent failure this module exists
/// to prevent: admission passes, and oxy leaves the source pointed at production.
///
/// Both halves are derived rather than listed — the connector states whether a
/// host exists, the arm states whether it applied one — so the guard stays true
/// without anyone remembering to update it.
fn admit_environment_is_applied(
    config: &SourceConfig,
    environment: airway::connector::Environment,
    connector: &dyn SourceConnector,
    applied: EnvironmentApplied,
) -> Result<(), AirwayError> {
    if !matches!(environment, airway::connector::Environment::Sandbox) {
        return Ok(());
    }
    if applied == EnvironmentApplied::Yes || connector.sandbox_base_url().is_none() {
        return Ok(());
    }
    Err(AirwayError::Other(format!(
        "source kind `{}` declares a sandbox host but this factory has no arm applying it, \
         so the run would pass admission and then talk to production. Add the mapping in \
         agentic_airway::source_factory alongside the quickbooks arm.",
        config.kind
    )))
}

fn build_source_connector_inner(
    config: &SourceConfig,
    tokens: Option<QuickBooksTokens>,
    environment: airway::connector::Environment,
) -> Result<(Box<dyn SourceConnector>, EnvironmentApplied), AirwayError> {
    // Dispatched ahead of the table below so the `Yes` is produced by the same
    // expression that passes `environment` in. An arm that stops taking
    // `environment` stops compiling here, rather than leaving a stale claim
    // behind in the guard.
    if config.kind == "quickbooks" {
        let connector = build_quickbooks(&config.config, tokens, environment)?;
        return Ok((connector, EnvironmentApplied::Yes));
    }

    // Every arm below ignores `environment`; `admit_environment_is_applied`
    // checks that against what the built connector actually declares.
    let connector = match config.kind.as_str() {
        "rest_api" => build_rest_api(&config.config),
        "filesystem" => build_filesystem(&config.config),
        "sql_database" => build_sql_database(&config.config),
        "clickhouse" => build_clickhouse(&config.config),
        "postgres_cdc" => build_postgres_cdc(&config.config),
        "toast" => build_toast(&config.config),
        "weather" => build_weather(&config.config),
        "besttime" => build_besttime(&config.config),
        "overture" => build_overture(&config.config),
        "http_file" => build_http_file(&config.config),
        "overpass" => build_overpass(&config.config),
        other => Err(AirwayError::Other(format!(
            "unsupported source kind `{other}`. Wire it up in \
             agentic_airway::source_factory::build_source_connector \
             — every airway source is fair game, this dispatch table \
             just enumerates the ones with a concrete arm so far."
        ))),
    }?;
    Ok((connector, EnvironmentApplied::No))
}

// ── rest_api ─────────────────────────────────────────────────────────────────

fn build_rest_api(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let config: RestApiConfig = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid rest_api config: {e}")))?;
    Ok(Box::new(RestApiSource::new(config)))
}

// ── filesystem ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilesystemParams {
    /// Base path or URL (`/data`, `s3://bucket/prefix`, `gs://...`, `az://...`).
    base_path: String,
    /// Glob pattern (e.g. `*.jsonl`, `**/*.csv`).
    pattern: String,
    /// `json` | `jsonl` | `csv`.
    format: FilesystemFormatLabel,
    /// Optional table name (defaults to airway's `file_data`).
    #[serde(default)]
    table_name: Option<String>,
    /// JSON path to records inside each JSON document (e.g. `data.items`).
    #[serde(default)]
    json_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FilesystemFormatLabel {
    Json,
    Jsonl,
    Csv,
}

impl From<FilesystemFormatLabel> for SourceFileFormat {
    fn from(label: FilesystemFormatLabel) -> Self {
        match label {
            FilesystemFormatLabel::Json => SourceFileFormat::Json,
            FilesystemFormatLabel::Jsonl => SourceFileFormat::Jsonl,
            FilesystemFormatLabel::Csv => SourceFileFormat::Csv,
        }
    }
}

fn build_filesystem(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let params: FilesystemParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid filesystem config: {e}")))?;

    let mut source =
        FilesystemSource::new(&params.base_path, &params.pattern, params.format.into());
    if let Some(name) = params.table_name.as_deref() {
        source = source.with_table_name(name);
    }
    if let Some(jp) = params.json_path.as_deref() {
        source = source.with_json_path(jp);
    }
    Ok(Box::new(source))
}

// ── sql_database ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SqlDatabaseParams {
    connection_string: String,
    backend: SqlBackendLabel,
    #[serde(default)]
    tables: Vec<SqlTableParams>,
}

/// Snake-case YAML labels for airway's `DatabaseBackend` variants.
/// Mirror is exhaustive — adding a backend in airway surfaces here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SqlBackendLabel {
    Postgres,
    Mysql,
    Sqlite,
    Mssql,
    Oracle,
    Custom,
}

impl From<SqlBackendLabel> for DatabaseBackend {
    fn from(label: SqlBackendLabel) -> Self {
        match label {
            SqlBackendLabel::Postgres => DatabaseBackend::Postgres,
            SqlBackendLabel::Mysql => DatabaseBackend::MySQL,
            SqlBackendLabel::Sqlite => DatabaseBackend::SQLite,
            SqlBackendLabel::Mssql => DatabaseBackend::MSSQL,
            SqlBackendLabel::Oracle => DatabaseBackend::Oracle,
            SqlBackendLabel::Custom => DatabaseBackend::Custom,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SqlTableParams {
    name: String,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    primary_key: Option<Vec<String>>,
    #[serde(default)]
    cursor_field: Option<String>,
    #[serde(default)]
    write_disposition: WriteDispositionLabel,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WriteDispositionLabel {
    #[default]
    Append,
    Replace,
    Merge,
    /// Append to a `<table>_raw` buffer; a scheduled airhouse vacuum
    /// compaction rebuilds the public table latest-wins. Avoids the
    /// O(target) `MERGE INTO` that OOMs the data plane on large tables.
    Replacing,
}

impl From<WriteDispositionLabel> for WriteDisposition {
    fn from(label: WriteDispositionLabel) -> Self {
        match label {
            WriteDispositionLabel::Append => WriteDisposition::Append,
            WriteDispositionLabel::Replace => WriteDisposition::Replace,
            WriteDispositionLabel::Merge => WriteDisposition::Merge,
            WriteDispositionLabel::Replacing => WriteDisposition::Replacing,
        }
    }
}

fn build_sql_database(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let params: SqlDatabaseParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid sql_database config: {e}")))?;

    let mut source = SqlDatabaseSource::new(&params.connection_string, params.backend.into());
    for table in params.tables {
        source = source.with_table(TableConfig {
            name: table.name,
            schema: table.schema,
            query: table.query,
            primary_key: table.primary_key,
            cursor_field: table.cursor_field,
            write_disposition: table.write_disposition.into(),
        });
    }
    Ok(Box::new(source))
}

// ── clickhouse ───────────────────────────────────────────────────────────────

/// ClickHouse source over the HTTP interface (`FORMAT JSONEachRow`).
///
/// Unlike `sql_database` (which takes a single `connection_string`),
/// ClickHouse carries discrete connection fields. The
/// `agentic-pipeline` executor substitutes `password_var` ->
/// `password` from the secret manager before dispatch, so the factory
/// only ever sees the resolved literal — `password_var` is therefore
/// not an accepted field here.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClickHouseParams {
    /// Host without scheme/port (e.g. `my-host.clickhouse.cloud`).
    host: String,
    /// HTTP(S) interface port. Defaults to airway's `8123`.
    #[serde(default)]
    port: Option<u16>,
    /// Database/schema to read from.
    database: String,
    /// Username. Defaults to airway's `default`.
    #[serde(default)]
    username: Option<String>,
    /// Resolved password literal (executor maps `password_var` -> this).
    #[serde(default)]
    password: Option<String>,
    /// Use HTTPS instead of HTTP. Defaults to airway's `false`.
    ///
    /// KNOWN / LOW-PRIORITY footgun for hand-authored YAML: a config with
    /// `port: 8443` and no `secure:` silently uses plaintext on a TLS port.
    /// The wizard template always emits `secure` and `port` together, so this
    /// is only reachable by hand-editing; left as-is deliberately.
    #[serde(default)]
    secure: Option<bool>,
    /// Client-side connect timeout in seconds. Defaults to airway's `30`.
    #[serde(default)]
    connect_timeout_secs: Option<u64>,
    /// Client-side read (inactivity) timeout in seconds. Defaults to airway's
    /// `120`. Resets on each received body chunk, so a steadily-streaming
    /// table of any size completes; only a stall longer than this aborts.
    /// Raise this when a slow destination back-pressures the read for long
    /// stretches (see also `settings.http_send_timeout`, which governs the
    /// ClickHouse *server* side of the same stall).
    #[serde(default)]
    read_timeout_secs: Option<u64>,
    /// Extra ClickHouse settings forwarded as URL query parameters on every
    /// request (e.g. `http_send_timeout`, `send_timeout`, `receive_timeout`,
    /// `max_execution_time`, `max_block_size`). Values may be written as
    /// numbers or strings in YAML; both are forwarded as strings.
    ///
    /// Socket timeouts MUST be set here rather than via a trailing SQL
    /// `SETTINGS` clause: ClickHouse fixes the HTTP response socket's timeout
    /// at request setup, before the query body's `SETTINGS` are parsed, so a
    /// `SETTINGS http_send_timeout=…` inside the SQL is ignored.
    #[serde(default)]
    settings: BTreeMap<String, Value>,
    /// Rows per streamed batch (one destination write). Defaults to airway's
    /// `10000`. Lower it when a slow destination back-pressures the read long
    /// enough to trip ClickHouse's socket send timeout — smaller writes drain
    /// faster, so the source read pauses for shorter stretches. `max_block_size`
    /// (a `settings` entry) does NOT do this: it tunes ClickHouse's output, not
    /// the destination-write size.
    #[serde(default)]
    batch_size: Option<usize>,
    /// Tables to extract. Reuses the `sql_database` table shape.
    #[serde(default)]
    tables: Vec<SqlTableParams>,
}

/// Build a `ClickHouseConn` from parsed params. All `ClickHouseConn`
/// fields are public; set them directly so an absent `password` stays
/// `None` (sending an empty `X-ClickHouse-Key` would differ from
/// sending no key at all).
fn clickhouse_conn(params: &ClickHouseParams) -> ClickHouseConn {
    let mut conn = ClickHouseConn::new(&params.host, &params.database);
    if let Some(port) = params.port {
        conn.port = port;
    }
    if let Some(username) = &params.username {
        conn.username = username.clone();
    }
    conn.password = params.password.clone();
    if let Some(secure) = params.secure {
        conn.secure = secure;
    }
    if let Some(secs) = params.connect_timeout_secs {
        conn.connect_timeout_secs = secs;
    }
    if let Some(secs) = params.read_timeout_secs {
        conn.read_timeout_secs = secs;
    }
    if let Some(n) = params.batch_size {
        conn.batch_size = n;
    }
    conn.settings = params
        .settings
        .iter()
        .map(|(k, v)| (k.clone(), json_value_to_setting(v)))
        .collect();
    conn
}

/// Render a YAML/JSON setting value as the string ClickHouse expects in a URL
/// query parameter. Strings pass through unquoted (`"600"` -> `600`); numbers
/// and bools use their plain literal so `http_send_timeout: 600` works without
/// the author needing to quote it.
fn json_value_to_setting(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn build_clickhouse(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let params: ClickHouseParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid clickhouse config: {e}")))?;

    let mut source = SqlDatabaseSource::clickhouse(clickhouse_conn(&params));
    for table in params.tables {
        source = source.with_table(TableConfig {
            name: table.name,
            schema: table.schema,
            query: table.query,
            primary_key: table.primary_key,
            cursor_field: table.cursor_field,
            write_disposition: table.write_disposition.into(),
        });
    }
    Ok(Box::new(source))
}

// ── discovery ────────────────────────────────────────────────────────────────

/// Connect to a source and list its tables (with columns) so a
/// pipeline-create UI can offer them for selection instead of making
/// the user hand-type table names. The `config` carries live
/// credentials supplied at wizard time — nothing is persisted here.
///
/// Only sources that support live introspection are wired; today that's
/// `clickhouse`. Other kinds return an error rather than silently
/// yielding nothing.
pub async fn discover_source_tables(
    config: &SourceConfig,
) -> Result<Vec<DiscoveredTable>, AirwayError> {
    match config.kind.as_str() {
        "clickhouse" => discover_clickhouse_tables(&config.config).await,
        other => Err(AirwayError::Other(format!(
            "table discovery is not supported for source kind `{other}`"
        ))),
    }
}

async fn discover_clickhouse_tables(raw: &Value) -> Result<Vec<DiscoveredTable>, AirwayError> {
    let params: ClickHouseParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid clickhouse config: {e}")))?;
    SqlDatabaseSource::clickhouse(clickhouse_conn(&params))
        .discover_tables()
        .await
        .map_err(|e| AirwayError::Other(format!("clickhouse discovery failed: {e}")))
}

// ── postgres_cdc ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostgresCdcParams {
    connection_string: String,
    slot_name: String,
    publication_name: String,
    #[serde(default)]
    tables: Vec<String>,
    #[serde(default)]
    batch_size: Option<usize>,
    #[serde(default)]
    initial_snapshot: Option<bool>,
}

fn build_postgres_cdc(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let params: PostgresCdcParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid postgres_cdc config: {e}")))?;

    let mut source = PostgresCdcSource::new(
        &params.connection_string,
        &params.slot_name,
        &params.publication_name,
    );
    if !params.tables.is_empty() {
        source = source.with_tables(params.tables);
    }
    if let Some(size) = params.batch_size {
        source = source.with_batch_size(size);
    }
    if let Some(snap) = params.initial_snapshot {
        source = source.with_initial_snapshot(snap);
    }
    Ok(Box::new(source))
}

// ── toast ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToastParams {
    /// Toast OAuth2 client id (an identifier, not a secret).
    client_id: String,
    /// Toast OAuth2 client secret. The `agentic-pipeline` executor
    /// substitutes `client_secret_var` -> this field from the secret
    /// manager before dispatch, so the factory only ever sees the
    /// resolved literal.
    client_secret: String,
    /// One entry per restaurant GUID to fan the pull across.
    restaurant_guids: Vec<String>,
    /// Sandbox / non-prod override. Defaults to airway's prod base URL.
    #[serde(default)]
    base_url: Option<String>,
    /// Bounded-backfill window `[start, end)` (RFC3339), injected by the
    /// executor for a backfill run. Both-or-neither; absent for normal
    /// incremental runs. See [`parse_backfill_window`].
    #[serde(default)]
    backfill_start: Option<String>,
    #[serde(default)]
    backfill_end: Option<String>,
}

fn build_toast(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let params: ToastParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid toast config: {e}")))?;
    if params.restaurant_guids.is_empty() {
        return Err(AirwayError::Other(
            "toast config: `restaurant_guids` must list at least one restaurant".into(),
        ));
    }
    let mut source = ToastSource::new(
        params.client_id,
        params.client_secret,
        params.restaurant_guids,
    );
    if let Some(base) = params.base_url.as_deref() {
        source = source.with_base_url(base);
    }
    if let Some((start, end)) = parse_backfill_window(&params.backfill_start, &params.backfill_end)?
    {
        // Toast backfills are RESUMABLE: the cursor advances into the run's
        // resume_state so a reset-in-place retry resumes mid-window. This MUST
        // be paired with the run-scoped state store (the worker selects it for
        // backfill runs) — otherwise the advanced cursor would corrupt the live
        // pipeline cursor, the exact hazard the non-resumable freeze guards.
        source = source.with_resumable_backfill_window(start, end);
    }
    Ok(Box::new(source))
}

/// Parse an optional RFC3339 `[start, end)` backfill window from source config.
///
/// Both-or-neither: `Ok(None)` when neither bound is set (normal run),
/// `Ok(Some(..))` when both parse, and an error if only one is present or
/// either fails to parse. Used by the date-windowed source builders.
fn parse_backfill_window(
    start: &Option<String>,
    end: &Option<String>,
) -> Result<Option<(DateTime<Utc>, DateTime<Utc>)>, AirwayError> {
    let parse = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| AirwayError::Other(format!("invalid backfill timestamp `{s}`: {e}")))
    };
    match (start.as_deref(), end.as_deref()) {
        (None, None) => Ok(None),
        (Some(s), Some(e)) => Ok(Some((parse(s)?, parse(e)?))),
        _ => Err(AirwayError::Other(
            "backfill window requires both `backfill_start` and `backfill_end`".into(),
        )),
    }
}

// ── quickbooks ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuickBooksParams {
    /// Intuit OAuth2 client id (an identifier, not a secret).
    client_id: String,
    /// Intuit OAuth2 client secret. The `agentic-pipeline` executor
    /// substitutes `client_secret_var` -> this field from the secret
    /// manager before dispatch, so the factory only sees the resolved
    /// literal.
    ///
    /// Required in rotating mode, unused in read-only mode (the data API
    /// authenticates with the bearer token alone) — hence optional here and
    /// enforced per-mode in [`build_quickbooks`].
    #[serde(default)]
    client_secret: Option<String>,
    /// Bootstrap refresh token (rotates on first use). Resolved from
    /// `refresh_token_var` by the executor; the rotated value is written
    /// back via the supplied [`RefreshTokenSink`]. Same per-mode optionality
    /// as `client_secret`.
    #[serde(default)]
    refresh_token: Option<String>,
    /// Name of the secret holding a host-maintained **access** token.
    ///
    /// Its presence in the YAML is what selects read-only mode. Unlike every
    /// other `*_var`, the executor does **not** substitute it into this struct:
    /// it builds an [`AccessTokenSource`] that re-reads per request, because an
    /// access token lives ~60 minutes and a backfill can outlive it. The field
    /// is declared here only so `deny_unknown_fields` accepts the YAML; the
    /// value is read by the executor, not by this factory.
    #[serde(default)]
    access_token_var: Option<String>,
    /// QuickBooks company id. Accepts a YAML string *or* a bare integer —
    /// realm ids are all-digits (e.g. 9341456860808037) and an unquoted
    /// value would otherwise fail deserialization ("invalid type: integer").
    #[serde(deserialize_with = "de_string_or_number")]
    realm_id: String,
    /// Sandbox override (`https://sandbox-quickbooks.api.intuit.com`).
    #[serde(default)]
    base_url: Option<String>,
    /// Optional API minor version (`?minorversion=`).
    #[serde(default, deserialize_with = "de_opt_string_or_number")]
    minor_version: Option<String>,
    /// Bounded-backfill window `[start, end)` (RFC3339), injected by the
    /// executor for a backfill run. Both-or-neither; absent for normal runs.
    #[serde(default)]
    backfill_start: Option<String>,
    #[serde(default)]
    backfill_end: Option<String>,
}

/// Deserialize a field that may appear as a YAML string or a bare number,
/// always yielding a `String`. Guards all-digit identifiers (realm id,
/// minor version) that YAML would otherwise parse as integers.
fn de_string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        I64(i64),
        U64(u64),
    }
    Ok(match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::String(s) => s,
        StringOrNumber::I64(n) => n.to_string(),
        StringOrNumber::U64(n) => n.to_string(),
    })
}

fn de_opt_string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(de_string_or_number(deserializer)?))
}

/// The base URL a QuickBooks source should use, or `None` to leave the
/// connector's own default in place.
///
/// `environment` is the deployment-wide *intent*; the host is per-vendor.
/// `admit_with` checks that a connector supports the environment, but
/// **applying** it is a separate step: airway resolves each source's sandbox
/// host from `Environment::installed()`, a process-wide global oxy never
/// installs. Without this, a `sandbox` run passes admission and then talks to
/// production — the one direction that must not fail silently.
///
/// An explicit `base_url` from the pipeline YAML is the narrower setting and
/// wins in either environment.
fn resolve_quickbooks_base_url(
    explicit: Option<&str>,
    environment: airway::connector::Environment,
) -> Option<String> {
    if let Some(base) = explicit {
        // Narrower wins — but say so. This pairing passes admission (QuickBooks
        // declares a sandbox host) and then sends production traffic, which is
        // the one direction this module's doc says must not fail silently.
        if matches!(environment, airway::connector::Environment::Sandbox)
            && base != SANDBOX_BASE_URL
        {
            tracing::warn!(
                base_url = %base,
                "environment is sandbox but an explicit base_url overrides it; \
                 requests go to that host, not the vendor sandbox"
            );
        }
        return Some(base.to_string());
    }
    matches!(environment, airway::connector::Environment::Sandbox)
        .then(|| SANDBOX_BASE_URL.to_string())
}

fn build_quickbooks(
    raw: &Value,
    tokens: Option<QuickBooksTokens>,
    environment: airway::connector::Environment,
) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let params: QuickBooksParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid quickbooks config: {e}")))?;

    // FAIL CLOSED. `access_token_var` in the YAML is a declaration that some
    // other writer owns this grant's rotation. If the executor did not hand us
    // a matching source — an unresolvable secret, or a dispatch path that never
    // builds one — the only other way to authenticate is to refresh, which is
    // exactly what the declaration forbids. Refusing is the whole point: a
    // silent fallback would fork the rotation chain and brick the grant, and it
    // would do so at 3am on a schedule rather than here.
    let read_only_declared = params.access_token_var.is_some();
    let tokens = match (read_only_declared, tokens) {
        (true, Some(QuickBooksTokens::ReadOnly(src))) => Some(QuickBooksTokens::ReadOnly(src)),
        (true, other) => {
            let got = match other {
                Some(QuickBooksTokens::Rotating(_)) => "a refresh-token sink",
                _ => "nothing",
            };
            return Err(AirwayError::Other(format!(
                "quickbooks config declares `access_token_var` (read-only token custody) \
                 but the host supplied {got}. Refusing to fall back to refreshing: \
                 Intuit expires the previous refresh token whenever it issues a new one, \
                 so a second refresher would fork this grant's rotation chain."
            )));
        }
        (false, other) => other,
    };

    let mut source = match &tokens {
        // Read-only: the data API authenticates with the bearer token alone, so
        // the refresh-side credentials are genuinely unused. Passing empty
        // strings is safe *because* `with_access_token_source` below makes the
        // refresh path unreachable.
        Some(QuickBooksTokens::ReadOnly(_)) => {
            QuickBooksSource::new(params.client_id, "", "", params.realm_id)
        }
        _ => {
            let client_secret = params.client_secret.ok_or_else(|| {
                AirwayError::Other(
                    "quickbooks config: `client_secret_var` is required unless \
                     `access_token_var` selects read-only token custody"
                        .into(),
                )
            })?;
            let refresh_token = params.refresh_token.ok_or_else(|| {
                AirwayError::Other(
                    "quickbooks config: `refresh_token_var` is required unless \
                     `access_token_var` selects read-only token custody"
                        .into(),
                )
            })?;
            QuickBooksSource::new(
                params.client_id,
                client_secret,
                refresh_token,
                params.realm_id,
            )
        }
    };
    if let Some(base) = resolve_quickbooks_base_url(params.base_url.as_deref(), environment) {
        source = source.with_base_url(&base);
    }
    if let Some(mv) = params.minor_version.as_deref() {
        source = source.with_minor_version(mv);
    }
    match tokens {
        Some(QuickBooksTokens::Rotating(sink)) => {
            source = source.with_refresh_token_sink(Arc::new(AirwayRefreshSink(sink)));
        }
        Some(QuickBooksTokens::ReadOnly(src)) => {
            source = source.with_access_token_source(Arc::new(AirwayAccessTokenSource(src)));
        }
        None => {}
    }
    if let Some((start, end)) = parse_backfill_window(&params.backfill_start, &params.backfill_end)?
    {
        source = source.with_backfill_window(start, end);
    }
    Ok(Box::new(source))
}

// ── weather (Open-Meteo) ───────────────────────────────────────────────────────

fn build_weather(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    // airway's `WeatherConfig` derives `Deserialize` and owns its own
    // defaults/validation, so — unlike toast/sql_database — we reuse it
    // directly rather than mirroring a params struct here.
    let config: WeatherConfig = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid weather config: {e}")))?;
    if config.locations.is_empty() {
        return Err(AirwayError::Other(
            "weather config: `locations` must list at least one location".into(),
        ));
    }
    let source = weather_source(config)
        .map_err(|e| AirwayError::Other(format!("weather source init failed: {e}")))?;
    Ok(Box::new(source))
}

// ── besttime (POST-then-extract foot-traffic forecasts) ────────────────────────

fn build_besttime(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    // airway's `BestTimeConfig` derives `Deserialize` and owns its own
    // validation; we reuse it directly (same shape as `weather`). The
    // agentic-pipeline executor substitutes `api_key_var` → `api_key` from
    // the secret manager before dispatch, so the factory only ever sees the
    // resolved literal — `api_key_var` is therefore not an accepted field
    // here.
    let config: BestTimeConfig = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid besttime config: {e}")))?;
    if config.venue_ids.is_empty() {
        return Err(AirwayError::Other(
            "besttime config: `venue_ids` must list at least one BestTime venue_id".into(),
        ));
    }
    let source = besttime_source(config)
        .map_err(|e| AirwayError::Other(format!("besttime source init failed: {e}")))?;
    Ok(Box::new(source))
}

// ── overture (Overture Maps Places, S3 GeoParquet) ─────────────────────────────

fn build_overture(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let config: OvertureConfig = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid overture config: {e}")))?;
    // Local empty-bboxes guard with a clear message, sibling to
    // build_besttime above. The upstream `overture_source` does its own
    // check but tests assert against this contract's wording; relying on
    // the upstream string pins our tests to a dependency we don't own.
    if config.bboxes.is_empty() {
        return Err(AirwayError::Other(
            "overture config: `bboxes` must list at least one bounding box".into(),
        ));
    }
    let source = overture_source(config)
        .map_err(|e| AirwayError::Other(format!("overture source init failed: {e}")))?;
    Ok(Box::new(source))
}

// ── http_file (generic HTTP/HTTPS file download via DuckDB) ────────────────────

fn build_http_file(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    // airway's `HttpFileConfig` derives `Deserialize` and owns its own
    // validation; we reuse it directly (same shape as `weather` / `besttime`).
    let config: HttpFileConfig = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid http_file config: {e}")))?;
    let source = http_file_source(config)
        .map_err(|e| AirwayError::Other(format!("http_file source init failed: {e}")))?;
    Ok(Box::new(source))
}

// ── overpass (OpenStreetMap via Overpass API) ─────────────────────────────────

fn build_overpass(raw: &Value) -> Result<Box<dyn SourceConnector>, AirwayError> {
    let config: OverpassConfig = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid overpass config: {e}")))?;
    // Local empty-bboxes guard with a clear message, sibling to
    // build_overture above and build_besttime's venue_ids guard.
    if config.bboxes.is_empty() {
        return Err(AirwayError::Other(
            "overpass config: `bboxes` must list at least one bounding box".into(),
        ));
    }
    let source = overpass_source(config)
        .map_err(|e| AirwayError::Other(format!("overpass source init failed: {e}")))?;
    Ok(Box::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SourceConfig;
    use serde_json::json;

    use airway::connector::auth::AuthConfig;

    fn cfg(kind: &str, config: Value) -> SourceConfig {
        SourceConfig {
            kind: kind.to_string(),
            config,
        }
    }

    /// Build with no refresh-token sink (the common test case), under the
    /// default `Environment::Production`.
    fn build(config: &SourceConfig) -> Result<Box<dyn SourceConnector>, AirwayError> {
        build_source_connector(config, None, airway::connector::Environment::Production)
    }

    #[test]
    fn rest_api_builds() {
        let source = build(&cfg(
            "rest_api",
            json!({
                "base_url": "https://api.example.com",
                "endpoints": [],
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "rest_api");
    }

    // The agentic-pipeline executor resolves a rest_api credential into
    // `auth.token` (bearer) / `auth.key` (`api_key` header, `api_key_query`).
    // RestApiConfig does NOT `deny_unknown_fields`, so if airway ever renamed
    // those fields the resolved secret would land in an ignored field and the
    // request would go out unauthenticated. These round-trips pin the contract
    // so a rename in airway-internal fails here at CI instead.
    #[test]
    fn rest_api_bearer_reads_resolved_token() {
        let config: RestApiConfig = serde_json::from_value(json!({
            "base_url": "https://api.yelp.com/v3",
            "auth": { "type": "bearer", "token": "resolved-secret" },
            "endpoints": [],
        }))
        .expect("deserialize");
        match config.auth {
            AuthConfig::Bearer { token } => assert_eq!(token, "resolved-secret"),
            other => panic!("expected bearer carrying the resolved token, got {other:?}"),
        }
    }

    #[test]
    fn rest_api_api_key_query_reads_resolved_key() {
        let config: RestApiConfig = serde_json::from_value(json!({
            "base_url": "https://api.census.gov/data/2023/acs/acs5",
            "auth": { "type": "api_key_query", "key": "resolved-secret", "param": "key" },
            "endpoints": [],
        }))
        .expect("deserialize");
        match config.auth {
            AuthConfig::ApiKeyQuery { key, param } => {
                assert_eq!(key, "resolved-secret");
                assert_eq!(param, "key");
            }
            other => panic!("expected api_key_query carrying the resolved key, got {other:?}"),
        }
    }

    #[test]
    fn rest_api_api_key_header_reads_resolved_key() {
        // The header variant's serde tag is `api_key` (not `api_key_header`); it
        // also carries the secret in `key`, so the executor's `key_var` -> `key`
        // mapping covers it.
        let config: RestApiConfig = serde_json::from_value(json!({
            "base_url": "https://example.com",
            "auth": { "type": "api_key", "key": "resolved-secret" },
            "endpoints": [],
        }))
        .expect("deserialize");
        match config.auth {
            AuthConfig::ApiKey { key, .. } => assert_eq!(key, "resolved-secret"),
            other => panic!("expected api_key carrying the resolved key, got {other:?}"),
        }
    }

    #[test]
    fn rest_api_rejects_unresolved_token_var() {
        // If an old binary fails to resolve `token_var` -> `token`, airway's
        // Bearer is missing its required `token`, so the build fails loudly (a
        // clear deserialize error) rather than silently sending an empty
        // credential.
        let err = build(&cfg(
            "rest_api",
            json!({
                "base_url": "https://x",
                "auth": { "type": "bearer", "token_var": "YELP_API_KEY" },
                "endpoints": [],
            }),
        ))
        .err()
        .expect("expected error");
        assert!(
            err.to_string().contains("invalid rest_api config"),
            "got: {err}"
        );
    }

    #[test]
    fn filesystem_builds_with_json_format() {
        let source = build(&cfg(
            "filesystem",
            json!({
                "base_path": "/tmp/data",
                "pattern": "*.jsonl",
                "format": "jsonl",
                "table_name": "events",
            }),
        ))
        .expect("build");
        // Filesystem reports its connector name (`filesystem`).
        assert_eq!(source.name(), "filesystem");
    }

    #[test]
    fn sql_database_builds_with_no_tables() {
        let source = build(&cfg(
            "sql_database",
            json!({
                "connection_string": "postgres://u:p@h/d",
                "backend": "postgres",
            }),
        ))
        .expect("build");
        let _ = source.name(); // not asserting exact name; just exercise path
    }

    #[test]
    fn clickhouse_builds_minimal() {
        let source = build(&cfg(
            "clickhouse",
            json!({
                "host": "my-host.clickhouse.cloud",
                "database": "default",
            }),
        ))
        .expect("build");
        // SqlDatabaseSource reports its connector name.
        let _ = source.name();
    }

    #[test]
    fn clickhouse_builds_with_credentials_and_tables() {
        let source = build(&cfg(
            "clickhouse",
            json!({
                "host": "my-host.clickhouse.cloud",
                "port": 8443,
                "database": "analytics",
                "username": "reader",
                "password": "resolved-secret",
                "secure": true,
                "tables": [
                    { "name": "events", "cursor_field": "created_at", "write_disposition": "append" }
                ],
            }),
        ))
        .expect("build");
        let _ = source.name();
    }

    #[test]
    fn clickhouse_rejects_unresolved_password_var() {
        // `password_var` must be stripped by the executor; if it leaks
        // through, deny_unknown_fields catches it.
        let err = build(&cfg(
            "clickhouse",
            json!({
                "host": "h",
                "database": "d",
                "password_var": "CLICKHOUSE_PASSWORD",
            }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().contains("invalid clickhouse config"));
    }

    #[test]
    fn postgres_cdc_builds() {
        let source = build(&cfg(
            "postgres_cdc",
            json!({
                "connection_string": "postgres://u:p@h/d",
                "slot_name": "oxy_slot",
                "publication_name": "oxy_pub",
                "tables": ["users", "orders"],
                "batch_size": 5000,
                "initial_snapshot": false,
            }),
        ))
        .expect("build");
        let _ = source.name();
    }

    #[test]
    fn toast_builds_with_resolved_credentials() {
        // Mirrors what the executor hands the factory: literal
        // client_id / client_secret (no `*_var` keys survive).
        let source = build(&cfg(
            "toast",
            json!({
                "client_id": "abc123",
                "client_secret": "shhh-resolved",
                "restaurant_guids": ["11111111-2222-3333-4444-555555555555"],
            }),
        ))
        .expect("build");
        let _ = source.name();
    }

    #[test]
    fn toast_rejects_empty_restaurant_guids() {
        let err = build(&cfg(
            "toast",
            json!({
                "client_id": "abc",
                "client_secret": "s",
                "restaurant_guids": [],
            }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().contains("restaurant_guids"));
    }

    #[test]
    fn toast_rejects_unresolved_var_key() {
        // `client_secret_var` must be stripped by the executor; if it
        // leaks through, deny_unknown_fields catches it.
        let err = build(&cfg(
            "toast",
            json!({
                "client_id": "abc",
                "client_secret_var": "TOAST_SECRET",
                "restaurant_guids": ["g"],
            }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().contains("invalid toast config"));
    }

    #[test]
    fn quickbooks_builds_with_resolved_credentials() {
        // Mirrors what the executor hands the factory: literal
        // client_secret / refresh_token (no `*_var` keys survive).
        let source = build(&cfg(
            "quickbooks",
            json!({
                "client_id": "intuit-client",
                "client_secret": "shhh-resolved",
                "refresh_token": "refresh-resolved",
                "realm_id": "1234567890",
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "quickbooks");
        // All 8 resources are advertised.
        assert_eq!(source.resources().len(), 8);
    }

    #[test]
    fn quickbooks_builds_with_optional_fields() {
        let source = build(&cfg(
            "quickbooks",
            json!({
                "client_id": "c",
                "client_secret": "s",
                "refresh_token": "r",
                "realm_id": "realm",
                "base_url": "https://sandbox-quickbooks.api.intuit.com",
                "minor_version": "70",
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "quickbooks");
    }

    #[test]
    fn quickbooks_accepts_numeric_realm_id() {
        // A bare-integer realm_id (unquoted YAML) must coerce to a string
        // rather than fail with "invalid type: integer".
        let source = build(&cfg(
            "quickbooks",
            json!({
                "client_id": "c",
                "client_secret": "s",
                "refresh_token": "r",
                "realm_id": 9341456860808037i64,
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "quickbooks");
    }

    #[test]
    fn toast_builds_with_backfill_window() {
        let source = build(&cfg(
            "toast",
            json!({
                "client_id": "abc",
                "client_secret": "shh",
                "restaurant_guids": ["g"],
                "backfill_start": "2024-01-01T00:00:00Z",
                "backfill_end": "2024-02-01T00:00:00Z",
            }),
        ))
        .expect("build toast with backfill window");
        assert_eq!(source.name(), "toast");
    }

    #[test]
    fn quickbooks_builds_with_backfill_window() {
        let source = build(&cfg(
            "quickbooks",
            json!({
                "client_id": "c",
                "client_secret": "s",
                "refresh_token": "r",
                "realm_id": "1234567890",
                "backfill_start": "2024-01-01T00:00:00Z",
                "backfill_end": "2024-02-01T00:00:00Z",
            }),
        ))
        .expect("build quickbooks with backfill window");
        assert_eq!(source.name(), "quickbooks");
    }

    #[test]
    fn backfill_window_requires_both_bounds() {
        let err = build(&cfg(
            "toast",
            json!({
                "client_id": "abc",
                "client_secret": "shh",
                "restaurant_guids": ["g"],
                "backfill_start": "2024-01-01T00:00:00Z",
            }),
        ))
        .err()
        .expect("expected error for one-sided backfill window");
        assert!(err.to_string().contains("requires both"));
    }

    #[test]
    fn backfill_window_rejects_malformed_timestamp() {
        let err = build(&cfg(
            "quickbooks",
            json!({
                "client_id": "c",
                "client_secret": "s",
                "refresh_token": "r",
                "realm_id": "1",
                "backfill_start": "not-a-date",
                "backfill_end": "2024-02-01T00:00:00Z",
            }),
        ))
        .err()
        .expect("expected error for malformed backfill timestamp");
        assert!(err.to_string().contains("invalid backfill timestamp"));
    }

    #[test]
    fn quickbooks_rejects_unresolved_var_key() {
        // `client_secret_var` / `refresh_token_var` must be stripped by
        // the executor; if one leaks through, deny_unknown_fields catches it.
        let err = build(&cfg(
            "quickbooks",
            json!({
                "client_id": "c",
                "client_secret_var": "QB_CLIENT_SECRET",
                "refresh_token_var": "QB_REFRESH_TOKEN",
                "realm_id": "realm",
            }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().contains("invalid quickbooks config"));
    }

    // ── read-only token custody ─────────────────────────────────────────

    struct StubAccessTokenSource;

    #[async_trait]
    impl AccessTokenSource for StubAccessTokenSource {
        async fn access_token(&self) -> Result<String, String> {
            Ok("access.stub".into())
        }
    }

    struct StubRefreshSink;

    #[async_trait]
    impl RefreshTokenSink for StubRefreshSink {
        async fn persist(&self, _refresh_token: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn build_with(
        config: &SourceConfig,
        tokens: Option<QuickBooksTokens>,
    ) -> Result<Box<dyn SourceConnector>, AirwayError> {
        build_source_connector(config, tokens, airway::connector::Environment::Production)
    }

    /// Read-only mode needs neither `client_secret` nor `refresh_token` — the
    /// data API authenticates with the bearer token alone.
    #[test]
    fn quickbooks_read_only_builds_without_refresh_credentials() {
        let source = build_with(
            &cfg(
                "quickbooks",
                json!({
                    "client_id": "c",
                    "realm_id": "realm",
                    "access_token_var": "apps/app-id/QB_ACCESS_TOKEN",
                }),
            ),
            Some(QuickBooksTokens::ReadOnly(Arc::new(StubAccessTokenSource))),
        )
        .expect("build read-only quickbooks");
        assert_eq!(source.name(), "quickbooks");
    }

    /// THE safety property. `access_token_var` declares that another writer
    /// owns this grant's rotation; if no source arrives, the only other way to
    /// authenticate is to refresh — which would fork the chain. Refuse instead.
    #[test]
    fn quickbooks_read_only_declared_without_a_source_is_refused() {
        let err = build_with(
            &cfg(
                "quickbooks",
                json!({
                    "client_id": "c",
                    "client_secret": "s",
                    "refresh_token": "r",
                    "realm_id": "realm",
                    "access_token_var": "apps/app-id/QB_ACCESS_TOKEN",
                }),
            ),
            None,
        )
        .err()
        .expect("expected refusal");
        assert!(
            err.to_string()
                .contains("Refusing to fall back to refreshing"),
            "unexpected error: {err}"
        );
    }

    /// Same refusal when the host supplies the *wrong* hook — a sink for a
    /// config that declared read-only custody.
    #[test]
    fn quickbooks_read_only_declared_but_handed_a_sink_is_refused() {
        let err = build_with(
            &cfg(
                "quickbooks",
                json!({
                    "client_id": "c",
                    "client_secret": "s",
                    "refresh_token": "r",
                    "realm_id": "realm",
                    "access_token_var": "apps/app-id/QB_ACCESS_TOKEN",
                }),
            ),
            Some(QuickBooksTokens::Rotating(Arc::new(StubRefreshSink))),
        )
        .err()
        .expect("expected refusal");
        assert!(
            err.to_string().contains("a refresh-token sink"),
            "unexpected error: {err}"
        );
    }

    /// Rotating mode still demands both credentials, so dropping them to
    /// `Option` for read-only mode can't silently weaken the refresh path.
    #[test]
    fn quickbooks_rotating_still_requires_refresh_credentials() {
        let err = build_with(
            &cfg(
                "quickbooks",
                json!({ "client_id": "c", "realm_id": "realm" }),
            ),
            None,
        )
        .err()
        .expect("expected error");
        assert!(
            err.to_string().contains("`client_secret_var` is required"),
            "unexpected error: {err}"
        );
    }

    // ── the sandbox-reaches-production guards ───────────────────────────

    /// Stands in for a future airway connector that declares a sandbox host
    /// this factory has no arm for. None exists today, which is exactly why
    /// the guard is checked against the connector rather than a list.
    ///
    /// Carries the host so the same fixture covers the other side too — a
    /// connector that declares none is the shape all ~11 environment-blind
    /// arms have in the pinned airway.
    struct Declares(Option<&'static str>);

    const DECLARES_SANDBOX: Declares = Declares(Some("https://sandbox.example.invalid"));
    const DECLARES_NO_SANDBOX: Declares = Declares(None);

    #[async_trait]
    impl SourceConnector for Declares {
        fn name(&self) -> &str {
            "declares"
        }
        fn resources(&self) -> Vec<airway::connector::ResourceInfo> {
            Vec::new()
        }
        fn sandbox_base_url(&self) -> Option<&str> {
            self.0
        }
        async fn extract(
            &self,
            _resource: &str,
            _state: Option<&Value>,
        ) -> Result<airway::connector::ExtractionResult, airway::AirwayError> {
            unimplemented!("not exercised by these tests")
        }
    }

    #[test]
    fn a_declared_sandbox_host_with_no_arm_is_refused() {
        let err = admit_environment_is_applied(
            &cfg("future_vendor", serde_json::json!({})),
            airway::connector::Environment::Sandbox,
            &DECLARES_SANDBOX,
            EnvironmentApplied::No,
        )
        .expect_err("admission would pass and oxy would leave it on production");
        assert!(err.to_string().contains("future_vendor"), "got: {err}");
    }

    #[test]
    fn an_arm_that_applied_the_environment_is_exempt() {
        admit_environment_is_applied(
            &cfg("quickbooks", serde_json::json!({})),
            airway::connector::Environment::Sandbox,
            &DECLARES_SANDBOX,
            EnvironmentApplied::Yes,
        )
        .expect("the arm resolved the host from `environment`");
    }

    /// The reason the marker replaced a hardcoded kind: the exemption follows
    /// the arm's behaviour, not its name. A `quickbooks` arm that stopped
    /// applying the environment must be refused exactly like any other, rather
    /// than keep passing on the strength of its kind string.
    #[test]
    fn the_kind_alone_does_not_exempt_an_arm_that_skipped_the_environment() {
        admit_environment_is_applied(
            &cfg("quickbooks", serde_json::json!({})),
            airway::connector::Environment::Sandbox,
            &DECLARES_SANDBOX,
            EnvironmentApplied::No,
        )
        .expect_err("a kind that no longer applies the environment must not stay exempt");
    }

    /// The guard must stay inert for the ~11 arms that legitimately ignore
    /// `environment`, or stage 1 would stop being behaviour-preserving.
    #[test]
    fn an_arm_that_ignored_the_environment_passes_when_no_host_is_declared() {
        admit_environment_is_applied(
            &cfg("toast", serde_json::json!({})),
            airway::connector::Environment::Sandbox,
            &DECLARES_NO_SANDBOX,
            EnvironmentApplied::No,
        )
        .expect("nothing to apply: the connector declares no sandbox host");
    }

    #[test]
    fn production_never_triggers_the_guard() {
        admit_environment_is_applied(
            &cfg("future_vendor", serde_json::json!({})),
            airway::connector::Environment::Production,
            &DECLARES_SANDBOX,
            EnvironmentApplied::No,
        )
        .expect("production needs no sandbox mapping");
    }

    #[test]
    fn sandbox_resolves_the_intuit_sandbox_host() {
        assert_eq!(
            resolve_quickbooks_base_url(None, airway::connector::Environment::Sandbox),
            Some(airway::connector::sources::quickbooks::SANDBOX_BASE_URL.to_string()),
        );
    }

    #[test]
    fn production_leaves_the_connector_default_alone() {
        // `None` means "don't call `with_base_url`", so the connector keeps
        // its own production default rather than oxy restating it.
        assert_eq!(
            resolve_quickbooks_base_url(None, airway::connector::Environment::Production),
            None,
        );
    }

    /// An explicit `base_url` in the pipeline YAML is narrower than the
    /// deployment-wide intent and must win over it — in both environments.
    #[test]
    fn an_explicit_base_url_outranks_the_environment() {
        for env in [
            airway::connector::Environment::Sandbox,
            airway::connector::Environment::Production,
        ] {
            assert_eq!(
                resolve_quickbooks_base_url(Some("https://qb.internal.invalid"), env),
                Some("https://qb.internal.invalid".to_string()),
                "explicit base_url must win under {env:?}",
            );
        }
    }

    #[test]
    fn quickbooks_still_builds_under_sandbox() {
        let config = SourceConfig {
            kind: "quickbooks".to_string(),
            config: serde_json::json!({
                "client_id": "id",
                "client_secret": "secret",
                "refresh_token": "token",
                "realm_id": 9341456860808037i64,
            }),
        };
        build_source_connector(&config, None, airway::connector::Environment::Sandbox)
            .expect("builds under sandbox");
    }

    #[test]
    fn weather_builds_with_locations() {
        let source = build(&cfg(
            "weather",
            json!({
                "locations": [
                    { "id": 1, "latitude": 37.7749, "longitude": -122.4194 }
                ],
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "weather");
    }

    #[test]
    fn weather_rejects_empty_locations() {
        let err = build(&cfg("weather", json!({ "locations": [] })))
            .err()
            .expect("expected error");
        assert!(err.to_string().contains("locations"));
    }

    #[test]
    fn besttime_builds_with_resolved_credentials() {
        // Mirrors what the executor hands the factory: literal `api_key`
        // (no `api_key_var` survives).
        let source = build(&cfg(
            "besttime",
            json!({
                "api_key": "resolved-secret",
                "venue_ids": ["ven_abc123", "ven_def456"],
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "besttime");
    }

    #[test]
    fn besttime_rejects_empty_venue_ids() {
        let err = build(&cfg(
            "besttime",
            json!({
                "api_key": "resolved-secret",
                "venue_ids": [],
            }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().contains("venue_ids"));
    }

    #[test]
    fn besttime_rejects_unresolved_var_key() {
        // `api_key_var` must be stripped by the executor; if it leaks
        // through, deny_unknown_fields catches it.
        let err = build(&cfg(
            "besttime",
            json!({
                "api_key_var": "BESTTIME_API_KEY",
                "venue_ids": ["ven_abc"],
            }),
        ))
        .err()
        .expect("expected error");
        // Either `api_key` is missing (required) or `api_key_var` is unknown —
        // both surface as "invalid besttime config".
        assert!(err.to_string().contains("invalid besttime config"));
    }

    #[test]
    fn overture_builds_with_bboxes() {
        let source = build(&cfg(
            "overture",
            json!({
                "release": "2026-05-21.0",
                "bboxes": [{
                    "name": "bay_area",
                    "min_lat": 36.85, "min_lng": -123.10,
                    "max_lat": 38.15, "max_lng": -121.55
                }]
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "overture");
    }

    #[test]
    fn overture_rejects_empty_bboxes() {
        let err = build(&cfg(
            "overture",
            json!({ "release": "2026-05-21.0", "bboxes": [] }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().contains("bbox"));
    }

    #[test]
    fn http_file_builds_csv_gz_with_filters_and_columns() {
        let source = build(&cfg(
            "http_file",
            json!({
                "url": "https://example.com/sample.csv.gz",
                "format": "csv_gz",
                "resource_name": "sample_table",
                "csv_options": {"header": true, "delimiter": ","},
                "filters": [{"column": "state", "op": "eq", "value": "06"}],
                "columns": ["w_geocode", "C000"]
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "http_file");
    }

    #[test]
    fn http_file_builds_zip_csv() {
        let source = build(&cfg(
            "http_file",
            json!({
                "url": "https://example.com/places.zip",
                "format": "zip_csv",
                "resource_name": "census_places",
                "zip_inner_glob": "*.csv"
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "http_file");
    }

    #[test]
    fn http_file_rejects_unknown_field() {
        let err = build(&cfg(
            "http_file",
            json!({
                "url": "https://example.com/x.csv",
                "format": "csv",
                "resource_name": "x",
                "bogus_field": true
            }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().contains("invalid http_file config"));
    }

    #[test]
    fn overpass_builds_with_bbox_and_template() {
        let source = build(&cfg(
            "overpass",
            json!({
                "bboxes": [{
                    "name": "bay_area",
                    "min_lat": 36.85, "min_lng": -123.10,
                    "max_lat": 38.15, "max_lng": -121.55
                }],
                "query_template":
                    "[out:json][timeout:60];(node[\"amenity\"=\"fast_food\"]({bbox}););out tags center;",
                "resource_name": "osm_pois"
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "overpass");
    }

    #[test]
    fn overpass_rejects_empty_bboxes() {
        let err = build(&cfg(
            "overpass",
            json!({
                "bboxes": [],
                "query_template": "[out:json];out;",
                "resource_name": "x"
            }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().to_lowercase().contains("bbox"));
    }

    #[test]
    fn overpass_rejects_unknown_field() {
        let err = build(&cfg(
            "overpass",
            json!({
                "bboxes": [{
                    "name": "x", "min_lat": 0.0, "min_lng": 0.0,
                    "max_lat": 1.0, "max_lng": 1.0
                }],
                "query_template": "[out:json];out;",
                "resource_name": "x",
                "bogus_field": "nope"
            }),
        ))
        .err()
        .expect("expected error");
        assert!(err.to_string().contains("invalid overpass config"));
    }

    #[test]
    fn unknown_kind_errors_with_extension_hint() {
        let err = build(&cfg("not_a_real_thing", json!({})))
            .err()
            .expect("expected error");
        let msg = err.to_string();
        assert!(msg.contains("not_a_real_thing"));
        assert!(msg.contains("source_factory"));
    }

    // Note: airway's `RestApiConfig` doesn't `deny_unknown_fields`, so
    // stowaway fields under `config:` are silently ignored. The
    // top-level `AirwayPipelineSpec`/`SourceConfig` structs ARE strict
    // — see `config::tests::rejects_unknown_top_level_field`. Strictness
    // at the connector-config level is airway's call, not ours.
}
