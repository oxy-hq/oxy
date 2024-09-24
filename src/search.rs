use skim::prelude::*;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

pub fn search_files(project_path: &PathBuf) -> Result<Option<String>, Box<dyn Error>> {
    let data_path = project_path.join("data");
    let manifest = construct_manifest(&data_path)?;

    let preview_cmd = format!("cat {}/{{}}", data_path.to_string_lossy());

    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
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

fn construct_manifest(data_path: &PathBuf) -> Result<String, Box<dyn Error>> {
    let mut manifest = String::new();

    if data_path.is_dir() {
        for entry in fs::read_dir(data_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Ok(relative_path) = path.strip_prefix(data_path) {
                    manifest.push_str(&format!("{}\n", relative_path.display()));
                }
            }
        }
    } else {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Data directory not found",
        )));
    }

    Ok(manifest)
}
