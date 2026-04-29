use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// Oxy-specific integration config for an airform project.
/// Loaded from `oxy.yml` in the project directory alongside `dbt_project.yml`.
///
/// Example:
/// ```yaml
/// mappings:
///   dev: local   # dbt target 'dev' maps to Oxy database 'local'
/// ```
///
/// Keys are dbt target names; values are database names in `config.yml`.
/// dbt already defines input and output semantics via the profile — this mapping just tells
/// Oxy which of its registered databases corresponds to each target.
///
/// `oxy.yml` with a `mappings` section is required — the service will error if it is absent
/// or if any profile output has no entry.
#[derive(Debug, Default, Deserialize)]
pub struct OxyProjectConfig {
    /// Maps dbt target names to Oxy database names (from `config.yml`).
    /// A single entry covers both source reading and output registration for that target.
    ///
    /// Key   = dbt target name (e.g. `"dev"`)
    /// Value = database name registered in Oxy's `config.yml`  (e.g. `"local"`)
    #[serde(default)]
    pub mappings: HashMap<String, String>,
}

impl OxyProjectConfig {
    /// Return `true` if `oxy.yml` exists in the project directory.
    pub fn exists(project_dir: &Path) -> bool {
        project_dir.join("oxy.yml").exists()
    }

    /// Load `oxy.yml` from the project directory.
    /// Returns a default (empty) config if the file doesn't exist or fails to parse.
    pub fn load(project_dir: &Path) -> Self {
        let path = project_dir.join("oxy.yml");
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_yaml::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!("Failed to parse oxy.yml at {}: {e}", path.display());
                Self::default()
            }),
            Err(e) => {
                tracing::warn!("Failed to read oxy.yml at {}: {e}", path.display());
                Self::default()
            }
        }
    }

    /// Return the list of target names that have no explicit entry in `mappings:`.
    /// An empty vec means all targets are covered.
    pub fn unmapped_outputs<'a>(
        &'a self,
        output_names: impl Iterator<Item = &'a str>,
    ) -> Vec<&'a str> {
        output_names
            .filter(|name| !self.mappings.contains_key(*name))
            .collect()
    }

    /// Resolve the Oxy database name for a given dbt target name.
    /// Returns `None` if the target has no entry in `mappings`.
    pub fn resolve_profile_database<'a>(&'a self, target_name: &'a str) -> Option<&'a str> {
        self.mappings.get(target_name).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_oxy_yml(dir: &TempDir, content: &str) {
        let path = dir.path().join("oxy.yml");
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    // ── exists ────────────────────────────────────────────────────────────────

    #[test]
    fn test_exists_returns_true_when_file_present() {
        let dir = TempDir::new().unwrap();
        write_oxy_yml(&dir, "mappings:\n  dev: local\n");
        assert!(OxyProjectConfig::exists(dir.path()));
    }

    #[test]
    fn test_exists_returns_false_when_file_absent() {
        let dir = TempDir::new().unwrap();
        assert!(!OxyProjectConfig::exists(dir.path()));
    }

    // ── load ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_load_parses_mappings() {
        let dir = TempDir::new().unwrap();
        write_oxy_yml(&dir, "mappings:\n  dev: local\n  prod: warehouse\n");
        let cfg = OxyProjectConfig::load(dir.path());
        assert_eq!(cfg.mappings.get("dev").map(|s| s.as_str()), Some("local"));
        assert_eq!(
            cfg.mappings.get("prod").map(|s| s.as_str()),
            Some("warehouse")
        );
        assert_eq!(cfg.mappings.len(), 2);
    }

    #[test]
    fn test_load_returns_default_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let cfg = OxyProjectConfig::load(dir.path());
        assert!(cfg.mappings.is_empty());
    }

    #[test]
    fn test_load_returns_default_on_invalid_yaml() {
        let dir = TempDir::new().unwrap();
        write_oxy_yml(&dir, ":::not valid yaml:::");
        let cfg = OxyProjectConfig::load(dir.path());
        assert!(cfg.mappings.is_empty());
    }

    #[test]
    fn test_load_empty_mappings_section() {
        let dir = TempDir::new().unwrap();
        write_oxy_yml(&dir, "mappings: {}\n");
        let cfg = OxyProjectConfig::load(dir.path());
        assert!(cfg.mappings.is_empty());
    }

    // ── resolve_profile_database ──────────────────────────────────────────────

    #[test]
    fn test_resolve_returns_mapped_database() {
        let dir = TempDir::new().unwrap();
        write_oxy_yml(&dir, "mappings:\n  dev: local\n");
        let cfg = OxyProjectConfig::load(dir.path());
        assert_eq!(cfg.resolve_profile_database("dev"), Some("local"));
    }

    #[test]
    fn test_resolve_returns_none_for_unmapped_target() {
        let dir = TempDir::new().unwrap();
        write_oxy_yml(&dir, "mappings:\n  dev: local\n");
        let cfg = OxyProjectConfig::load(dir.path());
        assert_eq!(cfg.resolve_profile_database("prod"), None);
    }

    #[test]
    fn test_resolve_returns_none_on_empty_mappings() {
        let cfg = OxyProjectConfig::default();
        assert_eq!(cfg.resolve_profile_database("dev"), None);
    }

    // ── unmapped_outputs ──────────────────────────────────────────────────────

    #[test]
    fn test_unmapped_outputs_returns_empty_when_all_covered() {
        let dir = TempDir::new().unwrap();
        write_oxy_yml(&dir, "mappings:\n  dev: local\n  prod: warehouse\n");
        let cfg = OxyProjectConfig::load(dir.path());
        let unmapped = cfg.unmapped_outputs(["dev", "prod"].iter().copied());
        assert!(unmapped.is_empty());
    }

    #[test]
    fn test_unmapped_outputs_returns_missing_targets() {
        let dir = TempDir::new().unwrap();
        write_oxy_yml(&dir, "mappings:\n  dev: local\n");
        let cfg = OxyProjectConfig::load(dir.path());
        let unmapped = cfg.unmapped_outputs(["dev", "prod", "staging"].iter().copied());
        assert_eq!(unmapped.len(), 2);
        assert!(unmapped.contains(&"prod"));
        assert!(unmapped.contains(&"staging"));
    }

    #[test]
    fn test_unmapped_outputs_all_missing_when_mappings_empty() {
        let cfg = OxyProjectConfig::default();
        let unmapped = cfg.unmapped_outputs(["dev", "prod"].iter().copied());
        assert_eq!(unmapped.len(), 2);
    }
}
