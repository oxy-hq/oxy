use std::path::{Path, PathBuf};

use crate::{errors::OxyError, theme::*};
use arrow::array::RecordBatch;
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::{LinesWithEndings, as_24_bit_terminal_escaped},
};

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
