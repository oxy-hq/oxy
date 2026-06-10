//! Dispatch a [`DestinationConfig`] into a concrete
//! [`airway::destination::Destination`].
//!
//! Same open-dispatch posture as [`crate::source_factory`]: any `kind`
//! is allowed at YAML parse time, and dispatch happens here. v1 wires
//! up `memory`, `airhouse`, and `postgres` — extend the match arm
//! below to wire up more.
//!
//! [`DestinationConfig`]: crate::config::DestinationConfig

use std::sync::Arc;

use airway::AirhouseDestination;
use airway::MemoryDestination;
use airway::connector::destinations::postgres::PostgresDestination;
use airway::destination::Destination;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

use crate::config::DestinationConfig;
use crate::error::AirwayError;

/// Host-side credential provider for the airhouse destination: returns a fresh
/// `postgresql://` DSN on demand (with a `String` error, like the host
/// [`RefreshTokenSink`](crate::source_factory::RefreshTokenSink)). The host
/// implements this over the airhouse broker so the destination re-mints a
/// non-expired ephemeral credential on every (re)connect; bridged onto airway's
/// `CredentialProvider` via [`AirwayCredentialProvider`].
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    async fn connection_string(&self) -> Result<String, String>;
}

/// Bridges a host [`CredentialProvider`] (String error) onto airway's
/// `CredentialProvider` (AirwayError) so the destination can drive it.
struct AirwayCredentialProvider(Arc<dyn CredentialProvider>);

#[async_trait]
impl airway::CredentialProvider for AirwayCredentialProvider {
    async fn connection_string(&self) -> Result<String, airway::AirwayError> {
        self.0
            .connection_string()
            .await
            .map_err(airway::AirwayError::Destination)
    }
}

/// Build the concrete [`Destination`] for a parsed destination config.
///
/// Returns a boxed trait object — `Box<dyn Destination>` auto-implements
/// `Destination` so it can be handed straight to
/// `airway::Pipeline::new(name, *destination)` once unboxed, or threaded
/// through `Arc<dyn Destination>` for the long-lived pipeline path.
pub fn build_destination(
    config: &DestinationConfig,
    credential_provider: Option<Arc<dyn CredentialProvider>>,
) -> Result<Box<dyn Destination>, AirwayError> {
    match config.kind.as_str() {
        "memory" => build_memory(&config.config),
        "airhouse" => build_airhouse(&config.config, credential_provider),
        "postgres" => build_postgres(&config.config),
        other => Err(AirwayError::Other(format!(
            "unsupported destination kind `{other}`. Wire it up in \
             agentic_airway::destination_factory::build_destination \
             — every airway destination is fair game, this dispatch \
             table just enumerates the ones with a concrete arm so far."
        ))),
    }
}

// ── memory ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryParams {
    /// Logical dataset name shown in events.
    dataset_name: String,
}

fn build_memory(raw: &Value) -> Result<Box<dyn Destination>, AirwayError> {
    let params: MemoryParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid memory config: {e}")))?;
    Ok(Box::new(MemoryDestination::new(params.dataset_name)))
}

// ── airhouse ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AirhouseParams {
    connection_string: String,
    dataset_name: String,
    /// Optional table-name → schema separator (e.g. `___`). Splits a flat
    /// source table `<schema><sep><table>` into `<schema>.<table>` at the
    /// destination instead of one root schema.
    #[serde(default)]
    schema_separator: Option<String>,
}

fn build_airhouse(
    raw: &Value,
    credential_provider: Option<Arc<dyn CredentialProvider>>,
) -> Result<Box<dyn Destination>, AirwayError> {
    let params: AirhouseParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid airhouse config: {e}")))?;
    let mut dest = AirhouseDestination::new(&params.connection_string, &params.dataset_name)
        .with_schema_separator(params.schema_separator);
    // When the host supplies a provider (airhouse_managed), the destination
    // re-mints a fresh credential on every (re)connect instead of reusing the
    // possibly-expired DSN baked into `connection_string`.
    if let Some(provider) = credential_provider {
        dest = dest.with_credential_provider(Arc::new(AirwayCredentialProvider(provider)));
    }
    Ok(Box::new(dest))
}

// ── postgres ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PostgresParams {
    connection_string: String,
    dataset_name: String,
}

fn build_postgres(raw: &Value) -> Result<Box<dyn Destination>, AirwayError> {
    let params: PostgresParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid postgres config: {e}")))?;
    Ok(Box::new(PostgresDestination::new(
        &params.connection_string,
        &params.dataset_name,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DestinationConfig;
    use serde_json::json;

    fn cfg(kind: &str, config: Value) -> DestinationConfig {
        DestinationConfig {
            kind: kind.to_string(),
            config,
        }
    }

    #[test]
    fn memory_builds() {
        let dest = build_destination(&cfg("memory", json!({"dataset_name": "scratch"})), None)
            .expect("build");
        // Just exercise the trait — destination metadata is internal.
        drop(dest);
    }

    #[test]
    fn airhouse_builds() {
        let dest = build_destination(
            &cfg(
                "airhouse",
                json!({
                    "connection_string": "postgres://u:p@h/d",
                    "dataset_name": "shopify_raw",
                }),
            ),
            None,
        )
        .expect("build");
        drop(dest);
    }

    #[test]
    fn postgres_builds() {
        let dest = build_destination(
            &cfg(
                "postgres",
                json!({
                    "connection_string": "postgres://u:p@h/d",
                    "dataset_name": "raw",
                }),
            ),
            None,
        )
        .expect("build");
        drop(dest);
    }

    #[test]
    fn memory_rejects_missing_dataset_name() {
        let err = build_destination(&cfg("memory", json!({})), None)
            .err()
            .expect("expected error");
        assert!(err.to_string().contains("invalid memory config"));
    }

    #[test]
    fn unknown_kind_errors_with_extension_hint() {
        let err = build_destination(&cfg("warpdrive", json!({})), None)
            .err()
            .expect("expected error");
        let msg = err.to_string();
        assert!(msg.contains("warpdrive"));
        assert!(msg.contains("destination_factory"));
    }
}
