use crate::SemanticLayerError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Version of the build manifest format
const MANIFEST_VERSION: &str = "1.0";

/// Build manifest that tracks the state of the last successful build
///
/// This manifest is used to enable incremental builds by tracking:
/// - File hashes of input files (semantic views/topics, config, globals)
/// - Embedding source file hashes (agents, workflows, SQL files)
/// - Output file mappings (which generated files came from which sources)
/// - Dependency graph (which views depend on which other views)
/// - Build metadata (timestamp, version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildManifest {
    /// Version of the manifest format
    pub version: String,

    /// SHA256 hashes of semantic layer input files
    /// Key: relative path from semantic dir (e.g., "views/orders.view.yml")
    /// Value: SHA256 hash of file contents
    /// Note: Using BTreeMap for stable iteration order (sorted keys)
    pub file_hashes: BTreeMap<String, String>,

    /// SHA256 hashes of embedding source files (agents, workflows, SQL, topics)
    /// Key: relative path from project root (e.g., "agents/default.agent.yml")
    /// Value: SHA256 hash of file contents
    /// Note: Using BTreeMap for stable iteration order (sorted keys)
    #[serde(default)]
    pub embedding_file_hashes: BTreeMap<String, String>,

    /// Mapping of source files to generated output files
    /// Key: source file path (relative to semantic dir, e.g., "views/orders.view.yml")
    /// Value: list of generated file paths
    /// Note: Using BTreeMap for stable iteration order (sorted keys)
    pub output_mapping: BTreeMap<String, Vec<String>>,

    /// Entity dependency graph
    /// Key: view name
    /// Value: list of view names that this view depends on
    /// Note: Using BTreeMap for stable iteration order (sorted keys)
    pub dependency_graph: BTreeMap<String, Vec<String>>,

    /// Timestamp of last successful build (Unix epoch seconds)
    pub last_build: i64,

    /// Hash of config.yml database configurations
    pub config_hash: String,

    /// Hash of globals/semantics.yml
    pub globals_hash: String,
}

impl BuildManifest {
    /// Create a new empty manifest
    pub fn new() -> Self {
        Self {
            version: MANIFEST_VERSION.to_string(),
            file_hashes: BTreeMap::new(),
            embedding_file_hashes: BTreeMap::new(),
            output_mapping: BTreeMap::new(),
            dependency_graph: BTreeMap::new(),
            last_build: 0,
            config_hash: String::new(),
            globals_hash: String::new(),
        }
    }

    /// Load manifest from file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Option<Self>, SemanticLayerError> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path)
            .map_err(|e| SemanticLayerError::IOError(format!("Failed to read manifest: {}", e)))?;

        // Parse manifest, treating parse errors as missing manifest
        let manifest: BuildManifest = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    "Failed to parse manifest (corrupted/invalid JSON): {}. Treating as missing.",
                    e
                );
                return Ok(None);
            }
        };

        // Version check
        if manifest.version != MANIFEST_VERSION {
            tracing::warn!(
                "Manifest version mismatch: expected {}, found {}. Treating as missing.",
                MANIFEST_VERSION,
                manifest.version
            );
            return Ok(None);
        }

        Ok(Some(manifest))
    }

    /// Save manifest to file (atomic write via temp file)
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), SemanticLayerError> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SemanticLayerError::IOError(format!("Failed to create manifest directory: {}", e))
            })?;
        }

        // Write to temp file first (atomic write)
        let temp_path = path.with_extension("tmp");
        let file = fs::File::create(&temp_path).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to create temp manifest file: {}", e))
        })?;

        serde_json::to_writer_pretty(file, self)
            .map_err(|e| SemanticLayerError::IOError(format!("Failed to write manifest: {}", e)))?;

        // Atomic rename
        fs::rename(&temp_path, path).map_err(|e| {
            SemanticLayerError::IOError(format!("Failed to rename manifest: {}", e))
        })?;

        Ok(())
    }

    /// Set the current timestamp
    pub fn update_timestamp(&mut self) {
        self.last_build = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
    }

    /// Add a file hash to the manifest
    pub fn add_file_hash<P: AsRef<Path>>(&mut self, file_path: P, hash: String) {
        self.file_hashes
            .insert(file_path.as_ref().to_string_lossy().to_string(), hash);
    }

    /// Add an output mapping entry
    pub fn add_output_mapping<P: AsRef<Path>>(
        &mut self,
        source_path: P,
        output_paths: Vec<String>,
    ) {
        self.output_mapping.insert(
            source_path.as_ref().to_string_lossy().to_string(),
            output_paths,
        );
    }

    /// Set the dependency graph
    pub fn set_dependency_graph(&mut self, graph: BTreeMap<String, Vec<String>>) {
        self.dependency_graph = graph;
    }

    /// Set the config hash
    pub fn set_config_hash(&mut self, hash: String) {
        self.config_hash = hash;
    }

    /// Set the globals hash
    pub fn set_globals_hash(&mut self, hash: String) {
        self.globals_hash = hash;
    }

    /// Set the embedding file hashes
    pub fn set_embedding_file_hashes(&mut self, hashes: BTreeMap<String, String>) {
        self.embedding_file_hashes = hashes;
    }
}

impl Default for BuildManifest {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute SHA256 hash of file contents
pub fn hash_file<P: AsRef<Path>>(path: P) -> Result<String, SemanticLayerError> {
    let content = fs::read(path.as_ref()).map_err(|e| {
        SemanticLayerError::IOError(format!(
            "Failed to read file {}: {}",
            path.as_ref().display(),
            e
        ))
    })?;

    let hash = Sha256::digest(&content);
    Ok(format!("{:x}", hash))
}

/// Compute SHA256 hash of a string
pub fn hash_string(content: &str) -> String {
    let hash = Sha256::digest(content.as_bytes());
    format!("{:x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_manifest_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        let mut manifest = BuildManifest::new();
        manifest.add_file_hash("test.yml", "abc123".to_string());
        manifest.set_config_hash("config123".to_string());
        manifest.set_globals_hash("globals123".to_string());
        manifest.update_timestamp();

        // Save
        manifest.save(&manifest_path).unwrap();

        // Load
        let loaded = BuildManifest::load(&manifest_path).unwrap().unwrap();

        assert_eq!(loaded.version, MANIFEST_VERSION);
        assert_eq!(
            loaded.file_hashes.get("test.yml"),
            Some(&"abc123".to_string())
        );
        assert_eq!(loaded.config_hash, "config123");
        assert_eq!(loaded.globals_hash, "globals123");
        assert!(loaded.last_build > 0);
    }

    #[test]
    fn test_manifest_missing() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("missing.json");

        let loaded = BuildManifest::load(&manifest_path).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_hash_string() {
        let hash1 = hash_string("hello");
        let hash2 = hash_string("hello");
        let hash3 = hash_string("world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA256 produces 64 hex chars
    }

    #[test]
    fn test_hash_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Create test file
        std::fs::write(&file_path, "test content").unwrap();

        let hash1 = hash_file(&file_path).unwrap();
        let hash2 = hash_file(&file_path).unwrap();

        // Same content produces same hash
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);

        // Different content produces different hash
        std::fs::write(&file_path, "different content").unwrap();
        let hash3 = hash_file(&file_path).unwrap();
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_manifest_output_mapping() {
        let mut manifest = BuildManifest::new();

        manifest.add_output_mapping(
            "views/orders.view.yml",
            vec![".semantics/model/orders.yml".to_string()],
        );
        manifest.add_output_mapping(
            "views/customers.view.yml",
            vec![".semantics/model/customers.yml".to_string()],
        );

        assert_eq!(manifest.output_mapping.len(), 2);
        assert_eq!(
            manifest.output_mapping.get("views/orders.view.yml"),
            Some(&vec![".semantics/model/orders.yml".to_string()])
        );
    }

    #[test]
    fn test_manifest_dependency_graph() {
        use std::collections::BTreeMap;

        let mut manifest = BuildManifest::new();

        let mut dep_graph = BTreeMap::new();
        dep_graph.insert("orders".to_string(), vec!["customers".to_string()]);
        dep_graph.insert("shipments".to_string(), vec!["orders".to_string()]);

        manifest.set_dependency_graph(dep_graph.clone());

        assert_eq!(manifest.dependency_graph, dep_graph);
        assert_eq!(
            manifest.dependency_graph.get("orders"),
            Some(&vec!["customers".to_string()])
        );
    }

    #[test]
    fn test_manifest_version_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        // Create manifest with wrong version
        let wrong_version = r#"{
            "version": "999.0",
            "file_hashes": {},
            "output_mapping": {},
            "dependency_graph": {},
            "last_build": 0,
            "config_hash": "",
            "globals_hash": ""
        }"#;

        std::fs::write(&manifest_path, wrong_version).unwrap();

        // Should return None for version mismatch
        let loaded = BuildManifest::load(&manifest_path).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_manifest_atomic_write() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        let mut manifest = BuildManifest::new();
        manifest.add_file_hash("test.yml", "abc123".to_string());

        // Save should be atomic (no .tmp file left behind)
        manifest.save(&manifest_path).unwrap();

        assert!(manifest_path.exists());
        assert!(!temp_dir.path().join("manifest.tmp").exists());
    }
}
