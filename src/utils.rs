use std::{fs, path::PathBuf};

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

pub fn collect_files_recursively(dir: &str, base_path: &str) -> anyhow::Result<Vec<String>> {
    let manifest: &mut Vec<String> = &mut Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let mut paths = collect_files_recursively(path.to_str().unwrap(), base_path)?;
            let paths = paths.as_mut();
            manifest.append(paths);
        } else if path.is_file() {
            if let Some(path_str) = path.to_str() {
                manifest.push(path_str.to_string());
            }
        }
    }
    Ok(manifest.clone())
}

pub fn list_file_stems(path: &str) -> anyhow::Result<Vec<String>> {
    let files = fs::read_dir(path)?
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect::<Vec<PathBuf>>();
    let names = files
        .iter()
        .map(|file| file.file_stem().unwrap().to_str().unwrap().to_string())
        .collect();
    Ok(names)
}
