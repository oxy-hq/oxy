use std::error::Error;
use std::path::{Path, PathBuf};
use std::fs;
use skim::prelude::*;

pub fn search_files(project_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let data_path = project_path.join("data");
    let manifest = construct_manifest(&data_path)?;

    let preview_cmd = format!("cat {}/{{}}",
                              data_path.to_string_lossy());

    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(true)
        .preview(Some(&preview_cmd))
        .build()
        .unwrap();

    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(std::io::Cursor::new(manifest));

    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        println!("{}", item.output());
    }

    Ok(())
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
