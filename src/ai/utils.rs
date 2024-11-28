use arrow::json as arrow_json;
use arrow::{
    error::ArrowError,
    record_batch::RecordBatch,
    util::display::{ArrayFormatter, FormatOptions},
};
use comfy_table::presets::ASCII_MARKDOWN;
use comfy_table::{Cell, Table};
use std::fmt::Display;

pub fn record_batches_to_markdown(results: &[RecordBatch]) -> Result<impl Display, ArrowError> {
    let options = FormatOptions::default().with_display_error(true);
    let mut table = Table::new();
    table.load_preset(ASCII_MARKDOWN);

    if results.is_empty() {
        return Ok(table);
    }

    let schema = results[0].schema();

    let mut header = Vec::new();
    for field in schema.fields() {
        header.push(Cell::new(field.name()));
    }

    table.set_header(header);
    for batch in results {
        let formatters = batch
            .columns()
            .iter()
            .map(|c| ArrayFormatter::try_new(c.as_ref(), &options))
            .collect::<Result<Vec<_>, ArrowError>>()?;

        for row in 0..batch.num_rows() {
            let mut cells = Vec::new();
            for formatter in &formatters {
                cells.push(Cell::new(formatter.value(row)));
            }
            table.add_row(cells);
        }
    }

    Ok(table)
}

pub fn record_batches_to_json(batches: &[RecordBatch]) -> Result<String, ArrowError> {
    // Write the record batches out as JSON
    let buf = Vec::new();
    let mut writer = arrow_json::LineDelimitedWriter::new(buf);

    // Convert each RecordBatch reference to &RecordBatch
    let batch_refs: Vec<&RecordBatch> = batches.iter().collect();
    writer.write_batches(&batch_refs)?;
    writer.finish()?;

    // Get the underlying buffer back and convert to string
    let buf = writer.into_inner();
    String::from_utf8(buf).map_err(|e| ArrowError::ExternalError(Box::new(e)))
}
