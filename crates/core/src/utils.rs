use std::path::{Path, PathBuf};

use crate::config::model::Dimension;
use crate::{errors::OxyError, theme::*};
use arrow::array::RecordBatch;
use csv::StringRecord;
use duckdb::Connection;
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::{LinesWithEndings, as_24_bit_terminal_escaped},
};
use tokio::task::spawn_blocking;

pub const MAX_DISPLAY_ROWS: usize = 100;
pub const MAX_OUTPUT_LENGTH: usize = 1000;

pub fn truncate_with_ellipsis(s: &str, max_width: Option<usize>) -> String {
    // We should truncate at grapheme-boundary and compute character-widths,
    // yet the dependencies on unicode-segmentation and unicode-width are
    // not worth it.
    let mut chars = s.chars();
    let max_width = max_width.unwrap_or(MAX_OUTPUT_LENGTH) - 1;
    let mut prefix = (&mut chars).take(max_width).collect::<String>();
    if chars.next().is_some() {
        prefix.push('…');
    }
    prefix
}

pub fn truncate_datasets(batches: Vec<RecordBatch>) -> (Vec<RecordBatch>, bool) {
    if !batches.is_empty() && batches[0].num_rows() > MAX_DISPLAY_ROWS {
        return (vec![batches[0].slice(0, MAX_DISPLAY_ROWS)], true);
    }
    (batches, false)
}

pub fn format_table_output(table: &str, truncated: bool) -> String {
    if truncated {
        format!(
            "{}\n{}",
            format!(
                "Results have been truncated. Showing only the first {} rows.",
                MAX_DISPLAY_ROWS
            ),
            table
        )
    } else {
        table.to_string()
    }
}

pub fn print_colored_sql(sql: &str) {
    println!("{}", "\nSQL query:".primary());
    println!("{}", get_colored_sql(sql));
    println!();
}

pub fn get_colored_sql(sql: &str) -> String {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps.find_syntax_by_extension("sql").unwrap();
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    let mut colored_sql = String::new();
    for line in LinesWithEndings::from(sql) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps).unwrap();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        colored_sql.push_str(&escaped);
    }
    colored_sql
}

pub fn find_project_path() -> Result<PathBuf, OxyError> {
    let mut current_dir = std::env::current_dir().expect("Could not get current directory");

    for _ in 0..10 {
        let config_path = current_dir.join("config.yml");
        if config_path.exists() {
            return Ok(current_dir);
        }

        if !current_dir.pop() {
            break;
        }
    }

    Err(OxyError::RuntimeError(
        "Could not find config.yml".to_string(),
    ))
}

pub fn get_relative_path(path: PathBuf, root: PathBuf) -> Result<String, OxyError> {
    let relative_path = path
        .strip_prefix(root)
        .map_err(|_| OxyError::ArgumentError("Could not get relative path".to_string()))?;
    Ok(relative_path.to_string_lossy().to_string())
}

pub fn variant_eq<T>(a: &T, b: &T) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

pub fn get_file_directories<P: AsRef<Path>>(file_path: P) -> Result<P, OxyError> {
    create_parent_dirs(&file_path).map_err(|e| {
        OxyError::IOError(format!(
            "Error creating directories for path '{}': {}",
            file_path.as_ref().display(),
            e
        ))
    })?;
    Ok(file_path)
}

pub fn create_parent_dirs<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub fn list_by_sub_extension(dir: &PathBuf, sub_extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(list_by_sub_extension(&path, sub_extension));
            } else if path.is_file()
                // && path.extension().and_then(|s| s.to_str()) == Some("")
                && path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.ends_with(sub_extension))
                    .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }

    files
}

pub async fn asyncify<F, T>(f: F) -> Result<T, OxyError>
where
    F: FnOnce() -> Result<T, OxyError> + Send + 'static,
    T: Send + 'static,
{
    match spawn_blocking(f).await {
        Ok(res) => res,
        Err(err) => Err(OxyError::RuntimeError(format!(
            "Failed to spawn blocking task: {}",
            err
        ))),
    }
}

pub fn extract_csv_dimensions(
    path: &std::path::Path,
) -> Result<Vec<Dimension>, crate::errors::OxyError> {
    let conn = Connection::open_in_memory().map_err(|e| {
        crate::errors::OxyError::RuntimeError(format!("Failed to open in-memory DuckDB: {}", e))
    })?;

    let sql = format!(
        "CREATE VIEW auto_csv AS SELECT * FROM read_csv_auto('{}', SAMPLE_SIZE=10000, ALL_VARCHAR=FALSE);",
        path.display()
    );
    conn.execute(&sql, []).map_err(|e| {
        crate::errors::OxyError::RuntimeError(format!(
            "DuckDB failed to read CSV {}: {}",
            path.display(),
            e
        ))
    })?;

    let mut stmt = conn
        .prepare("PRAGMA table_info('auto_csv');")
        .map_err(|e| {
            crate::errors::OxyError::RuntimeError(format!(
                "DuckDB failed to prepare schema query: {}",
                e
            ))
        })?;
    let mut rows = stmt.query([]).map_err(|e| {
        crate::errors::OxyError::RuntimeError(format!("DuckDB failed to query schema: {}", e))
    })?;

    let mut columns = Vec::new();
    while let Some(row) = rows.next().map_err(|e| {
        crate::errors::OxyError::RuntimeError(format!("DuckDB failed to read schema row: {}", e))
    })? {
        let name: String = row.get(1).map_err(|e| {
            crate::errors::OxyError::RuntimeError(format!("DuckDB schema row: {}", e))
        })?;
        let dtype: String = row.get(2).map_err(|e| {
            crate::errors::OxyError::RuntimeError(format!("DuckDB schema row: {}", e))
        })?;
        columns.push((name, dtype));
    }

    let mut samples: Vec<Vec<String>> = vec![Vec::new(); columns.len()];
    const MAX_SAMPLES_PER_COLUMN: usize = 1;
    if let Ok(mut reader) = csv::Reader::from_path(path) {
        for _ in 0..5 {
            let mut row = StringRecord::new();
            if !reader
                .read_record(&mut row)
                .map_err(|e| OxyError::RuntimeError(format!("CSV read error: {}", e)))?
            {
                break;
            }
            for (i, field) in row.iter().enumerate().take(columns.len()) {
                if samples[i].len() < MAX_SAMPLES_PER_COLUMN {
                    samples[i].push(field.to_string());
                }
            }
        }
    }

    let dimensions = columns
        .into_iter()
        .enumerate()
        .map(|(i, (name, dtype))| Dimension {
            name,
            synonyms: None,
            sample: samples.get(i).cloned().unwrap_or_default(),
            data_type: Some(dtype),
            is_partition_key: None,
        })
        .collect();

    Ok(dimensions)
}
