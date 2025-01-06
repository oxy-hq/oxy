use std::path::PathBuf;

use crate::theme::*;
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::{as_24_bit_terminal_escaped, LinesWithEndings},
};

pub fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    // We should truncate at grapheme-boundary and compute character-widths,
    // yet the dependencies on unicode-segmentation and unicode-width are
    // not worth it.
    let mut chars = s.chars();
    let mut prefix = (&mut chars).take(max_width - 1).collect::<String>();
    if chars.next().is_some() {
        prefix.push('…');
    }
    prefix
}

pub fn expand_globs(paths: &Vec<String>, cwd: PathBuf) -> anyhow::Result<Vec<String>> {
    let mut expanded_paths = Vec::new();
    for path in paths {
        let path = cwd.join(path);
        let pattern = path.to_str().unwrap();
        let glob = glob::glob(pattern).map_err(|err| {
            anyhow::anyhow!(
                "Failed to expand glob pattern '{}': {}",
                pattern,
                err.to_string()
            )
        })?;
        for entry in glob {
            if let Ok(path) = entry {
                if let Some(path_str) = path.to_str() {
                    if path.is_file() {
                        expanded_paths.push(path_str.to_string());
                    }
                }
            }
        }
    }
    Ok(expanded_paths)
}

pub fn print_colored_sql(sql: &str) {
    println!("{}", "\nSQL query:".primary());
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps.find_syntax_by_extension("sql").unwrap();
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    for line in LinesWithEndings::from(sql) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ps).unwrap();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        print!("{}", escaped);
    }
    println!();
}
