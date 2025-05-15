use std::{fs::File, path::Path};

use arrow::{
    array::RecordBatch,
    datatypes::SchemaRef,
    error::ArrowError,
    ipc::{reader::FileReader, writer::FileWriter},
};

use crate::errors::OxyError;

pub(super) fn connector_internal_error(message: &str, e: impl std::fmt::Display) -> OxyError {
    tracing::error!("{}: {}", message, e);
    OxyError::DBError(format!("{}: {}", message, e))
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

pub(super) fn write_to_ipc<P: AsRef<Path>>(
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
