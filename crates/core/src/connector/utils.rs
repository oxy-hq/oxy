use std::{fs::File, path::Path};

use arrow::{
    array::RecordBatch,
    datatypes::SchemaRef,
    error::ArrowError,
    ipc::{reader::FileReader, writer::FileWriter},
};

use oxy_shared::errors::OxyError;

/// Wrap a driver-level error into an [`OxyError`] with the full source chain
/// flattened into the message.
///
/// Driver errors from `connectorx`, `tokio-postgres`, `arrow`, etc. frequently
/// carry the interesting detail one or more layers deep; the top-level
/// `Display` often reads as a generic label ("unexpected message from server")
/// with the root cause only reachable via [`std::error::Error::source`]. This
/// helper flattens the whole chain so a single log line / error response
/// exposes that detail.
///
/// Callers wrapping an `anyhow::Error` pass `err.as_ref()` to get a
/// `&dyn std::error::Error` view.
pub(super) fn connector_internal_error(
    message: &str,
    e: &(dyn std::error::Error + 'static),
) -> OxyError {
    let chain = format_error_chain(e);
    tracing::error!(error.debug = ?e, error.chain = %chain, "{}", message);
    OxyError::DBError(format!("{message}: {chain}"))
}

/// Walk an error's `source()` chain and flatten into a single readable line.
pub(super) fn format_error_chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![e.to_string()];
    let mut current = e.source();
    while let Some(src) = current {
        let msg = src.to_string();
        // Avoid pathological duplicates when a wrapper prints its source inline.
        if !parts.last().map(|p| p == &msg).unwrap_or(false) {
            parts.push(format!("caused by: {msg}"));
        }
        current = src.source();
    }
    parts.join(" | ")
}

pub fn load_result(file_path: &str) -> anyhow::Result<(Vec<RecordBatch>, SchemaRef)> {
    let file = File::open(file_path).map_err(|_| {
      anyhow::Error::msg("Executed query did not generate a valid output file. If you are using an agent to generate the query, consider giving it a shorter prompt.".to_string())
  })?;
    let reader = FileReader::try_new(file, None)?;
    let schema = reader.schema();
    // Collect results and handle potential errors
    let batches: Result<Vec<RecordBatch>, ArrowError> = reader.collect();
    let batches = batches?;

    Ok((batches, schema))
}

pub fn write_to_ipc<P: AsRef<Path>>(
    batches: &Vec<RecordBatch>,
    file_path: P,
    schema: &SchemaRef,
) -> anyhow::Result<()> {
    let file = File::create(file_path)?;
    if batches.is_empty() {
        tracing::debug!("Warning: query returned no results.");
    }

    tracing::debug!("Schema: {:?}", schema);
    let schema_ref = schema.as_ref();
    let mut writer = FileWriter::try_new(file, schema_ref)?;
    for batch in batches {
        writer.write(batch)?;
    }
    writer.finish()?;
    Ok(())
}
