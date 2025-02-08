use sqlparse::{FormatOption, Formatter};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::execute::core::value::ContextValue;
use crate::execute::exporter::get_file_directories;
use crate::StyledText;

pub fn get_agent_cache(project_path: &PathBuf, cache_file_path: &str) -> Option<ContextValue> {
    match std::fs::read_to_string(project_path.join(cache_file_path)) {
        Ok(json) => {
            if cache_file_path.ends_with(".sql") {
                return Some(ContextValue::Text(json));
            }
            match serde_json::from_str::<ContextValue>(&json) {
                Ok(value) => Some(value),
                Err(e) => {
                    println!(
                        "{}",
                        format!(
                            "Ignored cache. Error deserializing cache file '{}'",
                            cache_file_path
                        )
                        .warning()
                    );
                    log::error!(
                        "Error deserializing cache file '{}': {}",
                        cache_file_path,
                        e
                    );
                    None
                }
            }
        }
        Err(_) => None,
    }
}

pub fn write_agent_cache(path: &PathBuf, result: &ContextValue) {
    match get_file_directories(path) {
        Ok(file_path) => {
            let mut file = match File::create(&file_path) {
                Ok(f) => f,
                Err(e) => {
                    println!(
                        "{}",
                        format!(
                            "Error creating directories for path '{}': {}",
                            path.display(),
                            e
                        )
                        .warning()
                    );

                    return;
                }
            };
            let buf = match file_path.extension().map(|ext| ext.to_str().unwrap()) {
                Some("sql") => format_sql(format!("{}", result).as_str())
                    .as_bytes()
                    .to_vec(),
                _ => serde_json::to_string(result).unwrap().as_bytes().to_vec(),
            };

            let _ = file.write_all(buf.as_slice()).map_err(|e| {
                println!(
                    "{}",
                    format!("Error writing to cache file: {}", e).warning()
                );
            });
        }
        Err(e) => println!(
            "{}",
            format!(
                "Error creating directories for path '{}': {}",
                path.display(),
                e
            )
            .warning()
        ),
    }
}

fn format_sql(sql: &str) -> String {
    let mut f = Formatter::default();
    let mut formatter = FormatOption::default();
    formatter.reindent = true;

    f.format(sql, &mut formatter)
}
