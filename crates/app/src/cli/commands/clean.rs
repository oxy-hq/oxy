use oxy::config::WorkingCopy;
use oxy::{
    config::{
        ConfigManager,
        constants::{CACHE_SOURCE, DATABASE_SEMANTIC_PATH},
    },
    theme::StyledText,
};
use oxy_shared::errors::OxyError;
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

fn confirm_deletion(item_description: &str, require_confirmation: bool) -> Result<bool, OxyError> {
    if !require_confirmation {
        return Ok(true);
    }
    print!("⚠️  Are you sure you want to delete {item_description}? (y/N): ");
    io::stdout()
        .flush()
        .map_err(|e| OxyError::IOError(format!("Failed to flush stdout: {e}")))?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| OxyError::IOError(format!("Failed to read input: {e}")))?;
    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

pub async fn clean_all(
    require_confirmation: bool,
    config_manager: &ConfigManager<WorkingCopy>,
) -> Result<(), OxyError> {
    println!("🧹 {} ephemeral data...", "Cleaning".text());
    clean_vectors(require_confirmation, config_manager).await?;
    clean_database_folder(require_confirmation, config_manager).await?;
    clean_cache(require_confirmation, config_manager).await?;
    println!("✨ {}", "Ephemeral data cleaned successfully!".success());
    Ok(())
}

pub async fn clean_database_folder(
    require_confirmation: bool,
    config_manager: &ConfigManager<WorkingCopy>,
) -> Result<(), OxyError> {
    println!("🗂️  {} .database folder...", "Clearing".text());
    let database_dir = config_manager.resolve_file(DATABASE_SEMANTIC_PATH).await?;
    let database_dir = PathBuf::from(database_dir);

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

    Ok(())
}

pub async fn clean_vectors(
    require_confirmation: bool,
    config_manager: &ConfigManager<WorkingCopy>,
) -> Result<(), OxyError> {
    println!("🔍 {} vector embeddings...", "Clearing".text());

    let lancedb_path = config_manager.resolve_file(".lancedb").await?;
    let lancedb_path = PathBuf::from(lancedb_path);
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

    println!("✅ {} cleared", "Vector embeddings".success());
    Ok(())
}

pub async fn clean_cache(
    require_confirmation: bool,
    config_manager: &ConfigManager<WorkingCopy>,
) -> Result<(), OxyError> {
    println!("🗂️  {} cache folder...", "Clearing".text());
    let state_dir = config_manager.resolve_state_dir().await?;
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
