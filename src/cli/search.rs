use skim::prelude::*;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub fn search_files(project_path: &PathBuf) -> Result<Option<String>, Box<dyn Error>> {
    let data_path = project_path.join("data");
    let manifest = construct_manifest(&data_path)?;

    let preview_cmd = format!("cat {}/{{}}", data_path.to_string_lossy());

    let options = SkimOptionsBuilder::default()
        .multi(false)
        .preview(Some(&preview_cmd))
        .build()
        .unwrap();

    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(std::io::Cursor::new(manifest));

    let selected_items = match Skim::run_with(&options, Some(items)) {
        Some(out) => {
            if out.is_abort {
                return Ok(None); // User cancelled, return None
            }
            out.selected_items
        }
        None => return Ok(None), // Skim was closed without selection
    };

    if let Some(item) = selected_items.first() {
        let file_name = item.output().into_owned();
        let file_path = data_path.join(file_name);
        if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(content) => Ok(Some(content)),
                Err(e) => Err(Box::new(e)),
            }
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

fn construct_manifest(data_path: &Path) -> Result<String, Box<dyn Error>> {
    let mut manifest = String::new();
    collect_files_recursively(data_path, &mut manifest, data_path)?;
    Ok(manifest)
}

fn collect_files_recursively(dir: &Path, manifest: &mut String, base_path: &Path) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursively(&path, manifest, base_path)?;
        } else if path.is_file() {
            if let Some(path_str) = path.strip_prefix(base_path)?.to_str() {
                manifest.push_str(path_str);
                manifest.push('\n');
            }
        }
    }
    Ok(())
}
