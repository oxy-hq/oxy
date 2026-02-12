use crate::SemanticLayerError;
use crate::build_manifest::{BuildManifest, hash_file, hash_string};
use serde_json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Result of change detection analysis
#[derive(Debug, Clone)]
pub struct ChangeDetectionResult {
    /// Views that need to be rebuilt (changed or depend on changed views)
    pub views_to_rebuild: Vec<String>,

    /// Topics that need to be rebuilt
    pub topics_to_rebuild: Vec<String>,

    /// Output files that need to be deleted (orphaned from deleted sources)
    pub files_to_delete: Vec<PathBuf>,

    /// Whether a full rebuild is required
    pub requires_full_rebuild: bool,

    /// Reason for full rebuild (if applicable)
    pub full_rebuild_reason: Option<String>,

    /// Whether vector embeddings need to be rebuilt
    pub requires_embedding_rebuild: bool,
}

impl ChangeDetectionResult {
    /// Check if there are any changes
    pub fn is_empty(&self) -> bool {
        self.views_to_rebuild.is_empty()
            && self.topics_to_rebuild.is_empty()
            && self.files_to_delete.is_empty()
            && !self.requires_full_rebuild
            && !self.requires_embedding_rebuild
    }
}

/// File type classification
#[derive(Debug, Clone, PartialEq)]
enum FileType {
    View(String),  // View name
    Topic(String), // Topic name
}

/// Change detector for incremental builds
pub struct ChangeDetector {
    /// Path to the semantic layer directory
    semantic_dir: PathBuf,

    /// Path to the target directory (.semantics/)
    target_dir: PathBuf,
}

impl ChangeDetector {
    /// Create a new change detector
    pub fn new<P: AsRef<Path>>(semantic_dir: P, target_dir: P) -> Self {
        Self {
            semantic_dir: semantic_dir.as_ref().to_path_buf(),
            target_dir: target_dir.as_ref().to_path_buf(),
        }
    }

    /// Detect changes between current state and previous manifest
    ///
    /// # Arguments
    /// * `config_hash` - Hash of current database configuration
    /// * `globals_hash` - Hash of current globals/semantics.yml
    /// * `force` - Force full rebuild
    ///
    /// # Returns
    /// A `ChangeDetectionResult` describing what needs to be rebuilt
    pub fn detect_changes(
        &self,
        config_hash: String,
        globals_hash: String,
        force: bool,
    ) -> Result<ChangeDetectionResult, SemanticLayerError> {
        // Handle force rebuild FIRST (before loading manifest)
        // This allows --force to recover from corrupted manifests
        if force {
            return Ok(ChangeDetectionResult {
                views_to_rebuild: Vec::new(),
                topics_to_rebuild: Vec::new(),
                files_to_delete: Vec::new(),
                requires_full_rebuild: true,
                full_rebuild_reason: Some("Forced rebuild (--force flag)".to_string()),
                requires_embedding_rebuild: true,
            });
        }

        // Load previous manifest
        let manifest_path = self.target_dir.join(".build_manifest.json");
        let prev_manifest = BuildManifest::load(&manifest_path)?;

        // Handle missing manifest (first build)
        let manifest = match prev_manifest {
            Some(m) => m,
            None => {
                return Ok(ChangeDetectionResult {
                    views_to_rebuild: Vec::new(),
                    topics_to_rebuild: Vec::new(),
                    files_to_delete: Vec::new(),
                    requires_full_rebuild: true,
                    full_rebuild_reason: Some("No previous manifest found".to_string()),
                    requires_embedding_rebuild: true,
                });
            }
        };

        // Check globals hash
        if manifest.globals_hash != globals_hash {
            return Ok(ChangeDetectionResult {
                views_to_rebuild: Vec::new(),
                topics_to_rebuild: Vec::new(),
                files_to_delete: Vec::new(),
                requires_full_rebuild: true,
                full_rebuild_reason: Some("Globals changed".to_string()),
                requires_embedding_rebuild: true,
            });
        }

        // Check config hash
        if manifest.config_hash != config_hash {
            return Ok(ChangeDetectionResult {
                views_to_rebuild: Vec::new(),
                topics_to_rebuild: Vec::new(),
                files_to_delete: Vec::new(),
                requires_full_rebuild: true,
                full_rebuild_reason: Some("Database configuration changed".to_string()),
                requires_embedding_rebuild: true,
            });
        }

        // Scan current files and compute hashes
        let current_files = self.scan_semantic_files()?;

        // Check if ANY semantic layer files changed
        // If so, trigger full rebuild for semantic layer (no incremental)
        let mut semantic_files_changed = false;

        // Check for added or modified files
        for (file_path, current_hash) in &current_files {
            let prev_hash = manifest.file_hashes.get(file_path);

            if prev_hash.is_none() || prev_hash.unwrap() != current_hash {
                semantic_files_changed = true;
                break;
            }
        }

        // Check for deleted files
        if !semantic_files_changed {
            for (old_file, _) in &manifest.file_hashes {
                if !current_files.contains_key(old_file) {
                    semantic_files_changed = true;
                    break;
                }
            }
        }

        // If semantic files changed, trigger full rebuild for semantic layer
        if semantic_files_changed {
            // Check for embedding file changes
            let current_embedding_files = self.scan_embedding_files()?;
            let mut requires_embedding_rebuild = false;

            // Check for added, modified, or deleted embedding files
            for (file_path, current_hash) in &current_embedding_files {
                let prev_hash = manifest.embedding_file_hashes.get(file_path);
                if prev_hash.is_none() || prev_hash.unwrap() != current_hash {
                    requires_embedding_rebuild = true;
                    break;
                }
            }

            // Check for deleted embedding files
            if !requires_embedding_rebuild {
                for (old_file, _) in &manifest.embedding_file_hashes {
                    if !current_embedding_files.contains_key(old_file) {
                        requires_embedding_rebuild = true;
                        break;
                    }
                }
            }

            return Ok(ChangeDetectionResult {
                views_to_rebuild: Vec::new(),
                topics_to_rebuild: Vec::new(),
                files_to_delete: Vec::new(),
                requires_full_rebuild: true,
                full_rebuild_reason: Some("Semantic layer files changed".to_string()),
                requires_embedding_rebuild,
            });
        }

        // No semantic changes - check if embeddings changed
        let current_embedding_files = self.scan_embedding_files()?;
        let mut requires_embedding_rebuild = false;

        // Check for added, modified, or deleted embedding files
        for (file_path, current_hash) in &current_embedding_files {
            let prev_hash = manifest.embedding_file_hashes.get(file_path);
            if prev_hash.is_none() || prev_hash.unwrap() != current_hash {
                requires_embedding_rebuild = true;
                break;
            }
        }

        // Check for deleted embedding files
        if !requires_embedding_rebuild {
            for (old_file, _) in &manifest.embedding_file_hashes {
                if !current_embedding_files.contains_key(old_file) {
                    requires_embedding_rebuild = true;
                    break;
                }
            }
        }

        // No changes at all
        Ok(ChangeDetectionResult {
            views_to_rebuild: Vec::new(),
            topics_to_rebuild: Vec::new(),
            files_to_delete: Vec::new(),
            requires_full_rebuild: false,
            full_rebuild_reason: None,
            requires_embedding_rebuild,
        })
    }

    /// Scan semantic files and compute their hashes
    fn scan_semantic_files(&self) -> Result<BTreeMap<String, String>, SemanticLayerError> {
        let mut file_hashes = BTreeMap::new();

        // Scan views directory
        let views_dir = self.semantic_dir.join("views");
        if views_dir.exists() {
            Self::scan_directory(
                &views_dir,
                &self.semantic_dir,
                ".view.yml",
                &mut file_hashes,
            )?;
        }

        // Scan topics directory
        let topics_dir = self.semantic_dir.join("topics");
        if topics_dir.exists() {
            Self::scan_directory(
                &topics_dir,
                &self.semantic_dir,
                ".topic.yml",
                &mut file_hashes,
            )?;
        }

        Ok(file_hashes)
    }

    /// Scan embedding source files (agents, workflows, SQL) and compute their hashes
    pub fn scan_embedding_files(&self) -> Result<BTreeMap<String, String>, SemanticLayerError> {
        let mut file_hashes = BTreeMap::new();

        // Get project root (parent of .semantics directory)
        let project_root = self
            .target_dir
            .parent()
            .ok_or_else(|| SemanticLayerError::IOError("Invalid target directory".to_string()))?;

        // Scan for agent files (*.agent.yml) - typically in project root or subdirs
        Self::scan_directory_all_extensions(
            project_root,
            project_root,
            &[".agent.yml"],
            &mut file_hashes,
        )?;

        // Scan for workflow files (*.workflow.yml)
        Self::scan_directory_all_extensions(
            project_root,
            project_root,
            &[".workflow.yml"],
            &mut file_hashes,
        )?;

        // Scan for SQL files (*.sql)
        Self::scan_directory_all_extensions(
            project_root,
            project_root,
            &[".sql"],
            &mut file_hashes,
        )?;

        // Topics are scanned here too for embeddings (separate from semantic layer)
        let topics_dir = self.semantic_dir.join("topics");
        if topics_dir.exists() {
            Self::scan_directory(&topics_dir, project_root, ".topic.yml", &mut file_hashes)?;
        }

        Ok(file_hashes)
    }

    /// Scan a directory for files with a specific extension
    fn scan_directory(
        dir: &Path,
        base_dir: &Path,
        extension: &str,
        file_hashes: &mut BTreeMap<String, String>,
    ) -> Result<(), SemanticLayerError> {
        for entry in fs::read_dir(dir).map_err(|e| {
            SemanticLayerError::IOError(format!(
                "Failed to read directory {}: {}",
                dir.display(),
                e
            ))
        })? {
            let entry = entry.map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();

            if path.is_file() {
                if let Some(file_name) = path.file_name() {
                    if file_name.to_string_lossy().ends_with(extension) {
                        let hash = hash_file(&path)?;

                        // Use relative path from semantic_dir
                        // base_dir is semantic_dir, strip to get relative path like "views/orders.view.yml"
                        let relative_path = path
                            .strip_prefix(base_dir)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .to_string();

                        file_hashes.insert(relative_path, hash);
                    }
                }
            } else if path.is_dir() {
                // Recursively scan subdirectories
                Self::scan_directory(&path, base_dir, extension, file_hashes)?;
            }
        }

        Ok(())
    }

    /// Scan a directory for files with multiple extensions
    fn scan_directory_all_extensions(
        dir: &Path,
        base_dir: &Path,
        extensions: &[&str],
        file_hashes: &mut BTreeMap<String, String>,
    ) -> Result<(), SemanticLayerError> {
        // Skip .semantics, .oxy, .git, node_modules, target directories
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if dir_name.starts_with('.')
            || dir_name == "node_modules"
            || dir_name == "target"
            || dir_name == "dist"
            || dir_name == "build"
        {
            return Ok(());
        }

        for entry in fs::read_dir(dir).map_err(|e| {
            SemanticLayerError::IOError(format!(
                "Failed to read directory {}: {}",
                dir.display(),
                e
            ))
        })? {
            let entry = entry.map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();

            if path.is_file() {
                if let Some(file_name) = path.file_name() {
                    let file_name_str = file_name.to_string_lossy();
                    if extensions.iter().any(|ext| file_name_str.ends_with(ext)) {
                        let hash = hash_file(&path)?;

                        // Use relative path from base_dir (project root)
                        let relative_path = path
                            .strip_prefix(base_dir)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .to_string();

                        file_hashes.insert(relative_path, hash);
                    }
                }
            } else if path.is_dir() {
                // Recursively scan subdirectories
                Self::scan_directory_all_extensions(&path, base_dir, extensions, file_hashes)?;
            }
        }

        Ok(())
    }

    /// Classify a file path as view or topic
    fn classify_file(file_path: &str) -> Option<FileType> {
        let path = Path::new(file_path);
        let file_name = path.file_name()?.to_string_lossy();

        if file_name.ends_with(".view.yml") {
            let name = file_name.strip_suffix(".view.yml")?;
            Some(FileType::View(name.to_string()))
        } else if file_name.ends_with(".topic.yml") {
            let name = file_name.strip_suffix(".topic.yml")?;
            Some(FileType::Topic(name.to_string()))
        } else {
            None
        }
    }
}

/// Helper to compute hash of database configuration from HashMap
pub fn hash_database_config(
    databases: &HashMap<String, crate::cube::models::DatabaseDetails>,
) -> String {
    // Convert to BTreeMap for deterministic ordering before serializing
    let ordered: BTreeMap<_, _> = databases.iter().collect();
    let json_str = serde_json::to_string(&ordered).unwrap_or_default();
    hash_string(&json_str)
}

/// Helper to compute hash of globals registry
///
/// Since GlobalRegistry doesn't implement Serialize, we hash the globals
/// directory files directly. This is more accurate anyway since it detects
/// file changes.
pub fn hash_globals_registry(globals_dir: &std::path::Path) -> Result<String, SemanticLayerError> {
    // Look for semantics.yml in the globals directory
    let semantics_file = globals_dir.join("semantics.yml");

    if semantics_file.exists() {
        hash_file(&semantics_file)
    } else {
        // No globals file exists, return empty hash
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_classify_file() {
        assert_eq!(
            ChangeDetector::classify_file("views/orders.view.yml"),
            Some(FileType::View("orders".to_string()))
        );
        assert_eq!(
            ChangeDetector::classify_file("topics/sales.topic.yml"),
            Some(FileType::Topic("sales".to_string()))
        );
        assert_eq!(ChangeDetector::classify_file("other.yml"), None);
    }

    #[test]
    fn test_change_detection_result_is_empty() {
        let empty = ChangeDetectionResult {
            views_to_rebuild: vec![],
            topics_to_rebuild: vec![],
            files_to_delete: vec![],
            requires_full_rebuild: false,
            full_rebuild_reason: None,
            requires_embedding_rebuild: false,
        };
        assert!(empty.is_empty());

        let not_empty = ChangeDetectionResult {
            views_to_rebuild: vec!["orders".to_string()],
            topics_to_rebuild: vec![],
            files_to_delete: vec![],
            requires_full_rebuild: false,
            full_rebuild_reason: None,
            requires_embedding_rebuild: false,
        };
        assert!(!not_empty.is_empty());
    }

    #[test]
    fn test_detect_changes_force() {
        let temp_dir = TempDir::new().unwrap();
        let semantic_dir = temp_dir.path().join("semantics");
        let target_dir = temp_dir.path().join(".semantics");

        std::fs::create_dir_all(&semantic_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        let detector = ChangeDetector::new(&semantic_dir, &target_dir);

        // Force rebuild
        let result = detector
            .detect_changes("config_hash".to_string(), "globals_hash".to_string(), true)
            .unwrap();

        assert!(result.requires_full_rebuild);
        assert_eq!(
            result.full_rebuild_reason,
            Some("Forced rebuild (--force flag)".to_string())
        );
    }

    #[test]
    fn test_detect_changes_no_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let semantic_dir = temp_dir.path().join("semantics");
        let target_dir = temp_dir.path().join(".semantics");

        std::fs::create_dir_all(&semantic_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        let detector = ChangeDetector::new(&semantic_dir, &target_dir);

        // No manifest exists
        let result = detector
            .detect_changes("config_hash".to_string(), "globals_hash".to_string(), false)
            .unwrap();

        assert!(result.requires_full_rebuild);
        assert_eq!(
            result.full_rebuild_reason,
            Some("No previous manifest found".to_string())
        );
    }

    #[test]
    fn test_detect_changes_globals_changed() {
        let temp_dir = TempDir::new().unwrap();
        let semantic_dir = temp_dir.path().join("semantics");
        let target_dir = temp_dir.path().join(".semantics");

        std::fs::create_dir_all(&semantic_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        // Create manifest with old globals hash
        let mut manifest = BuildManifest::new();
        manifest.set_globals_hash("old_hash".to_string());
        manifest.set_config_hash("config_hash".to_string());
        manifest
            .save(&target_dir.join(".build_manifest.json"))
            .unwrap();

        let detector = ChangeDetector::new(&semantic_dir, &target_dir);

        // New globals hash
        let result = detector
            .detect_changes("config_hash".to_string(), "new_hash".to_string(), false)
            .unwrap();

        assert!(result.requires_full_rebuild);
        assert_eq!(
            result.full_rebuild_reason,
            Some("Globals changed".to_string())
        );
    }

    #[test]
    fn test_detect_changes_config_changed() {
        let temp_dir = TempDir::new().unwrap();
        let semantic_dir = temp_dir.path().join("semantics");
        let target_dir = temp_dir.path().join(".semantics");

        std::fs::create_dir_all(&semantic_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        // Create manifest with old config hash
        let mut manifest = BuildManifest::new();
        manifest.set_globals_hash("globals_hash".to_string());
        manifest.set_config_hash("old_config".to_string());
        manifest
            .save(&target_dir.join(".build_manifest.json"))
            .unwrap();

        let detector = ChangeDetector::new(&semantic_dir, &target_dir);

        // New config hash
        let result = detector
            .detect_changes("new_config".to_string(), "globals_hash".to_string(), false)
            .unwrap();

        assert!(result.requires_full_rebuild);
        assert_eq!(
            result.full_rebuild_reason,
            Some("Database configuration changed".to_string())
        );
    }

    #[test]
    fn test_hash_database_config() {
        use crate::cube::models::DatabaseDetails;

        let mut databases = HashMap::new();
        databases.insert(
            "db1".to_string(),
            DatabaseDetails {
                name: "db1".to_string(),
                db_type: "postgres".to_string(),
                dataset_id: None,
            },
        );

        let hash1 = hash_database_config(&databases);
        let hash2 = hash_database_config(&databases);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);

        // Different config produces different hash
        databases.insert(
            "db2".to_string(),
            DatabaseDetails {
                name: "db2".to_string(),
                db_type: "mysql".to_string(),
                dataset_id: None,
            },
        );
        let hash3 = hash_database_config(&databases);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_hash_globals_registry() {
        let temp_dir = TempDir::new().unwrap();
        let globals_dir = temp_dir.path().join("globals");

        std::fs::create_dir_all(&globals_dir).unwrap();

        // No semantics.yml file
        let hash1 = hash_globals_registry(&globals_dir).unwrap();
        assert_eq!(hash1, String::new());

        // Create semantics.yml file
        let semantics_file = globals_dir.join("semantics.yml");
        std::fs::write(&semantics_file, "test: value").unwrap();

        let hash2 = hash_globals_registry(&globals_dir).unwrap();
        assert_ne!(hash2, String::new());
        assert_eq!(hash2.len(), 64);

        // Modify file
        std::fs::write(&semantics_file, "test: different").unwrap();
        let hash3 = hash_globals_registry(&globals_dir).unwrap();
        assert_ne!(hash2, hash3);
    }
}
