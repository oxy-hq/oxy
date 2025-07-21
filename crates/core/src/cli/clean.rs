use crate::{
    config::constants::{CACHE_SOURCE, DATABASE_SEMANTIC_PATH},
    db::client::get_state_dir,
    errors::OxyError,
    project::resolve_project_path,
    theme::StyledText,
};
use std::{
    fs,
    io::{self, Write},
};

fn confirm_deletion(item_description: &str, require_confirmation: bool) -> Result<bool, OxyError> {
    if !require_confirmation {
        return Ok(true);
    }
    print!(
        "⚠️  Are you sure you want to delete {}? (y/N): ",
        item_description
    );
    io::stdout()
        .flush()
        .map_err(|e| OxyError::IOError(format!("Failed to flush stdout: {}", e)))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| OxyError::IOError(format!("Failed to read input: {}", e)))?;
    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

pub async fn clean_all(require_confirmation: bool) -> Result<(), OxyError> {
    println!("🧹 {} ephemeral data...", "Cleaning".text());
    clean_vectors(require_confirmation).await?;
    clean_database_folder(require_confirmation).await?;
    clean_cache(require_confirmation).await?;
    println!("✨ {}", "Ephemeral data cleaned successfully!".success());
    Ok(())
}

pub async fn clean_database_folder(require_confirmation: bool) -> Result<(), OxyError> {
    println!("🗂️  {} .database folder...", "Clearing".text());
    match resolve_project_path() {
        Ok(project_path) => {
            let database_dir = project_path.join(DATABASE_SEMANTIC_PATH);
            if database_dir.exists() {
                if !confirm_deletion(
                    "the .databases folder (semantic models and build artifacts)",
                    require_confirmation,
                )? {
                    println!("  {} Operation cancelled", "ℹ️".text());
                    return Ok(());
                }
                match fs::remove_dir_all(&database_dir) {
                    Ok(()) => {
                        println!("  {} Removed .databases folder", "🗂️".warning());
                        println!("✅ {} cleared", ".databases folder".success());
                    }
                    Err(e) => {
                        return Err(OxyError::IOError(format!(
                            "Failed to remove .databases folder: {e}"
                        )));
                    }
                }
            } else {
                println!("  {} .databases folder not found", "ℹ️".text());
            }
        }
        Err(_) => {
            println!(
                "  {} Not in an Oxy project, skipping .databases folder cleanup",
                "ℹ️".text()
            );
        }
    }
    Ok(())
}

pub async fn clean_vectors(require_confirmation: bool) -> Result<(), OxyError> {
    println!("🔍 {} vector embeddings...", "Clearing".text());
    match resolve_project_path() {
        Ok(project_path) => {
            let lancedb_path = project_path.join(".lancedb");
            if lancedb_path.exists() {
                if !confirm_deletion(
                    "all vector embeddings (.lancedb folder)",
                    require_confirmation,
                )? {
                    println!("  {} Operation cancelled", "ℹ️".text());
                    return Ok(());
                }
                match fs::remove_dir_all(&lancedb_path) {
                    Ok(()) => {
                        println!("  {} Removed .lancedb folder", "🔍".warning());
                    }
                    Err(e) => {
                        return Err(OxyError::IOError(format!(
                            "Failed to remove .lancedb folder: {e}"
                        )));
                    }
                }
            } else {
                println!("  {} .lancedb folder not found", "ℹ️".text());
            }
        }
        Err(_) => {
            println!(
                "  {} Not in an Oxy project, skipping .lancedb cleanup",
                "ℹ️".text()
            );
        }
    }
    println!("✅ {} cleared", "Vector embeddings".success());
    Ok(())
}

pub async fn clean_cache(require_confirmation: bool) -> Result<(), OxyError> {
    println!("🗂️  {} cache folder...", "Clearing".text());
    let state_dir = get_state_dir();
    let cache_dir = state_dir.join(CACHE_SOURCE);

    if cache_dir.exists() {
        if !confirm_deletion("the cache folder", require_confirmation)? {
            println!("  {} Operation cancelled", "ℹ️".text());
            return Ok(());
        }

        match fs::remove_dir_all(&cache_dir) {
            Ok(()) => {
                if let Err(e) = fs::create_dir_all(&cache_dir) {
                    println!(
                        "  {} Warning: Failed to recreate cache directory: {}",
                        "⚠️".warning(),
                        e
                    );
                }
                println!("  {} Removed cache folder", "🗂️".warning());
            }
            Err(e) => {
                return Err(OxyError::IOError(format!(
                    "Failed to remove cache folder: {e}"
                )));
            }
        }
    } else {
        println!("  {} Cache folder not found", "ℹ️".text());
    }

    println!("✅ {} cleared", "Cache".success());
    Ok(())
}
