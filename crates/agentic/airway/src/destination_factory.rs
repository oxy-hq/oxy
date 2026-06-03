//! Dispatch a [`DestinationConfig`] into a concrete
//! [`airway::destination::Destination`].
//!
//! Same open-dispatch posture as [`crate::source_factory`]: any `kind`
//! is allowed at YAML parse time, and dispatch happens here. v1 wires
//! up `memory`, `airhouse`, and `postgres` — extend the match arm
//! below to wire up more.
//!
//! [`DestinationConfig`]: crate::config::DestinationConfig

use airway::AirhouseDestination;
use airway::MemoryDestination;
use airway::connector::destinations::postgres::PostgresDestination;
use airway::destination::Destination;
use serde::Deserialize;
use serde_json::Value;

use crate::config::DestinationConfig;
use crate::error::AirwayError;

/// Build the concrete [`Destination`] for a parsed destination config.
///
/// Returns a boxed trait object — `Box<dyn Destination>` auto-implements
/// `Destination` so it can be handed straight to
/// `airway::Pipeline::new(name, *destination)` once unboxed, or threaded
/// through `Arc<dyn Destination>` for the long-lived pipeline path.
pub fn build_destination(config: &DestinationConfig) -> Result<Box<dyn Destination>, AirwayError> {
    match config.kind.as_str() {
        "memory" => build_memory(&config.config),
        "airhouse" => build_airhouse(&config.config),
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

fn build_airhouse(raw: &Value) -> Result<Box<dyn Destination>, AirwayError> {
    let params: AirhouseParams = serde_json::from_value(raw.clone())
        .map_err(|e| AirwayError::Other(format!("invalid airhouse config: {e}")))?;
    Ok(Box::new(
        AirhouseDestination::new(&params.connection_string, &params.dataset_name)
            .with_schema_separator(params.schema_separator),
    ))
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
        let dest =
            build_destination(&cfg("memory", json!({"dataset_name": "scratch"}))).expect("build");
        // Just exercise the trait — destination metadata is internal.
        drop(dest);
    }

    #[test]
    fn airhouse_builds() {
        let dest = build_destination(&cfg(
            "airhouse",
            json!({
                "connection_string": "postgres://u:p@h/d",
                "dataset_name": "shopify_raw",
            }),
        ))
        .expect("build");
        drop(dest);
    }

    #[test]
    fn postgres_builds() {
        let dest = build_destination(&cfg(
            "postgres",
            json!({
                "connection_string": "postgres://u:p@h/d",
                "dataset_name": "raw",
            }),
        ))
        .expect("build");
        drop(dest);
    }

    #[test]
    fn memory_rejects_missing_dataset_name() {
        let err = build_destination(&cfg("memory", json!({})))
            .err()
            .expect("expected error");
        assert!(err.to_string().contains("invalid memory config"));
    }

    #[test]
    fn unknown_kind_errors_with_extension_hint() {
        let err = build_destination(&cfg("warpdrive", json!({})))
            .err()
            .expect("expected error");
        let msg = err.to_string();
        assert!(msg.contains("warpdrive"));
        assert!(msg.contains("destination_factory"));
    }
}
