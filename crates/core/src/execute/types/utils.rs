use arrow::datatypes::Schema;
use arrow::json as arrow_json;
use arrow::{
    error::ArrowError,
    record_batch::RecordBatch,
    util::display::{ArrayFormatter, FormatOptions},
};
use std::fmt::Display;
use std::sync::Arc;
use tabled::Table;
use tabled::settings::Panel;
use tabled::{
    builder::Builder,
    settings::{Settings, Style, Width, peaker::Priority},
};
use terminal_size::{Height as TerminalHeight, Width as TerminalWidth, terminal_size};

fn get_terminal_size() -> (usize, usize) {
    let terminal_size = terminal_size();
    match terminal_size {
        Some((TerminalWidth(width), TerminalHeight(height))) => (width as usize, height as usize),
        None => (80, 24),
    }
}

fn build_table(headers: &[String], rows: Vec<Vec<String>>) -> Table {
    let mut builder = Builder::default();
    builder.push_record(headers);
    for row in rows {
        builder.push_record(row);
    }

    builder.build()
}

fn get_header(schema: &Arc<Schema>) -> Vec<String> {
    let headers: Vec<String> = schema
        .fields()
        .iter()
        .map(|f| f.name().to_string())
        .collect();
    headers
}

fn create_formatters(batch: &RecordBatch) -> Result<Vec<ArrayFormatter<'_>>, ArrowError> {
    let formatters = batch
        .columns()
        .iter()
        .map(|c| {
            ArrayFormatter::try_new(
                c.as_ref(),
                &FormatOptions::default().with_display_error(true),
            )
        })
        .collect::<Result<Vec<_>, ArrowError>>()?;
    Ok(formatters)
}

fn format_row(formatters: &[ArrayFormatter], row: usize) -> Vec<String> {
    formatters
        .iter()
        .map(|f| clean_text(&f.value(row).to_string()))
        .collect()
}

pub fn clean_text(text: &str) -> String {
    // Tabled doesn't calculate the width of emojis correctly when using variant selector, so we need to remove them
    text.replace("\u{fe0f}", "")
}

pub fn record_batches_to_markdown(
    results: &[RecordBatch],
    schema: &Arc<Schema>,
) -> Result<impl Display, ArrowError> {
    let headers: Vec<String> = get_header(schema);
    let rows = record_batches_to_rows(results)?;

    let mut table = build_table(&headers, rows);
    table.with(Style::markdown());

    Ok(table.to_string())
}

pub fn record_batches_to_rows(results: &[RecordBatch]) -> Result<Vec<Vec<String>>, ArrowError> {
    let mut rows = Vec::new();
    for batch in results {
        let formatters = create_formatters(batch)?;
        for row in 0..batch.num_rows() {
            rows.push(format_row(&formatters, row));
        }
    }
    Ok(rows)
}

pub fn record_batches_to_table(
    results: &[RecordBatch],
    schema: &Arc<Schema>,
) -> Result<String, ArrowError> {
    let headers: Vec<String> = get_header(schema);

    let (width, _) = get_terminal_size();

    // Limit columns to improve readability
    let max_column = std::cmp::max(2, (width / 16) as i32);
    let displayed_column = max_column - 1;

    let total_column = headers.len();
    let displayed_headers = headers
        .into_iter()
        .take(displayed_column.try_into().unwrap())
        .chain(if total_column > displayed_column as usize {
            Some("…".to_string())
        } else {
            None
        })
        .collect::<Vec<String>>();
    tracing::debug!("Displayed columns: {:?}", displayed_headers);
    let mut rows = Vec::new();
    for batch in results {
        let formatters = create_formatters(batch)?
            .into_iter()
            .take(displayed_column as usize)
            .collect::<Vec<_>>();
        for row in 0..batch.num_rows() {
            let mut formatted_row = format_row(&formatters, row);
            if total_column > displayed_column as usize {
                formatted_row.push("…".to_string());
            }
            rows.push(formatted_row);
        }
    }

    let term_size_settings = Settings::default()
        .with(Width::wrap(width).priority(Priority::max(true)))
        .with(Width::increase(width).priority(Priority::min(true)));

    let mut table = build_table(&displayed_headers, rows);
    table.with(Style::ascii());
    table.with(term_size_settings);

    if total_column > displayed_column as usize {
        table.with(Panel::footer(format!(
            "{} columns ({} shown)",
            total_column, displayed_column
        )));
    }

    Ok(table.to_string())
}

pub fn record_batches_to_json(batches: &[RecordBatch]) -> Result<String, ArrowError> {
    // Write the record batches out as JSON
    let buf = Vec::new();
    let mut writer = arrow_json::ArrayWriter::new(buf);

    // Convert each RecordBatch reference to &RecordBatch
    let batch_refs: Vec<&RecordBatch> = batches.iter().collect();
    writer.write_batches(&batch_refs)?;
    writer.finish()?;

    // Get the underlying buffer back and convert to string
    let buf = writer.into_inner();
    String::from_utf8(buf).map_err(|e| ArrowError::ExternalError(Box::new(e)))
}

pub fn record_batches_to_2d_array(
    results: &[RecordBatch],
    schema: &Arc<Schema>,
) -> Result<Vec<Vec<String>>, ArrowError> {
    let headers: Vec<String> = get_header(schema);
    let mut rows = Vec::new();
    rows.push(headers.clone());
    for batch in results {
        let formatters = create_formatters(batch)?;
        for row in 0..batch.num_rows() {
            rows.push(format_row(&formatters, row));
        }
    }
    Ok(rows)
}
