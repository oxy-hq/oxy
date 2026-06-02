//! Dispatch a [`SourceConfig`] into a concrete [`airway::SourceConnector`].
//!
//! The YAML schema is intentionally open: any `kind` string is
//! permitted at parse time, and dispatch happens here. Adding support
//! for a new airway source means adding one arm to
//! [`build_source_connector`]; the YAML surface stays unchanged.
//!
//! v1 wires up the four **generic** airway sources (`rest_api`,
//! `filesystem`, `sql_database`, `postgres_cdc`). The vendor-specific
//! helpers (`shopify`, `github`, `stripe`, …) all build on top of
//! `RestApiSource` upstream — most can be expressed directly as a
//! `rest_api` config, so we defer wiring per-vendor sugar until there's
//! a real consumer asking for it.
//!
//! [`SourceConfig`]: crate::config::SourceConfig

use airway::connector::SourceConnector;
use airway::connector::sources::filesystem::{FilesystemSource, SourceFileFormat};
use airway::connector::sources::postgres_cdc::PostgresCdcSource;
use airway::connector::sources::rest_api::{RestApiConfig, RestApiSource};
use airway::connector::sources::sql_database::{DatabaseBackend, SqlDatabaseSource, TableConfig};
use airway::connector::sources::toast::ToastSource;
use airway::connector::sources::weather::{WeatherConfig, weather_source};
use airway::types::WriteDisposition;
use serde::Deserialize;
use serde_json::Value;

use crate::config::SourceConfig;
use crate::error::AirwayError;

/// Build the concrete [`SourceConnector`] for a parsed source config.
///
/// Returns a boxed trait object so the worker can hand it straight to
/// `airway::connector::parallel::extract_*` without committing to a
/// specific connector type at the worker layer.
pub fn build_source_connector(
    config: &SourceConfig,
) -> Result<Box<dyn SourceConnector>, AirwayError> {
    match config.kind.as_str() {
        "rest_api" => build_rest_api(&config.config),
        "filesystem" => build_filesystem(&config.config),
        "sql_database" => build_sql_database(&config.config),
        "postgres_cdc" => build_postgres_cdc(&config.config),
        "toast" => build_toast(&config.config),
        "weather" => build_weather(&config.config),
        other => Err(AirwayError::Other(format!(
            "unsupported source kind `{other}`. Wire it up in \
             agentic_airway::source_factory::build_source_connector \
             — every airway source is fair game, this dispatch table \
             just enumerates the ones with a concrete arm so far."
        ))),
    }
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
}

impl From<WriteDispositionLabel> for WriteDisposition {
    fn from(label: WriteDispositionLabel) -> Self {
        match label {
            WriteDispositionLabel::Append => WriteDisposition::Append,
            WriteDispositionLabel::Replace => WriteDisposition::Replace,
            WriteDispositionLabel::Merge => WriteDisposition::Merge,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SourceConfig;
    use serde_json::json;

    fn cfg(kind: &str, config: Value) -> SourceConfig {
        SourceConfig {
            kind: kind.to_string(),
            config,
        }
    }

    #[test]
    fn rest_api_builds() {
        let source = build_source_connector(&cfg(
            "rest_api",
            json!({
                "base_url": "https://api.example.com",
                "endpoints": [],
            }),
        ))
        .expect("build");
        assert_eq!(source.name(), "rest_api");
    }

    #[test]
    fn filesystem_builds_with_json_format() {
        let source = build_source_connector(&cfg(
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
        let source = build_source_connector(&cfg(
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
    fn postgres_cdc_builds() {
        let source = build_source_connector(&cfg(
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
        let source = build_source_connector(&cfg(
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
        let err = build_source_connector(&cfg(
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
        let err = build_source_connector(&cfg(
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
    fn weather_builds_with_locations() {
        let source = build_source_connector(&cfg(
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
        let err = build_source_connector(&cfg("weather", json!({ "locations": [] })))
            .err()
            .expect("expected error");
        assert!(err.to_string().contains("locations"));
    }

    #[test]
    fn unknown_kind_errors_with_extension_hint() {
        let err = build_source_connector(&cfg("not_a_real_thing", json!({})))
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
