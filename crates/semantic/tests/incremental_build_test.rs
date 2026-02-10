use oxy_semantic::{
    BuildManifest, ChangeDetectionResult, ChangeDetector, hash_database_config,
    hash_globals_registry,
};
use std::collections::BTreeMap;
use std::fs;
use tempfile::TempDir;

/// Helper to create a simple view file
fn create_view_file(dir: &std::path::Path, name: &str, content: &str) {
    let views_dir = dir.join("semantics/views");
    fs::create_dir_all(&views_dir).unwrap();
    fs::write(views_dir.join(format!("{}.view.yml", name)), content).unwrap();
}

/// Helper to create a simple topic file
fn create_topic_file(dir: &std::path::Path, name: &str, content: &str) {
    let topics_dir = dir.join("semantics/topics");
    fs::create_dir_all(&topics_dir).unwrap();
    fs::write(topics_dir.join(format!("{}.topic.yml", name)), content).unwrap();
}

/// Helper to create a globals file
fn create_globals_file(dir: &std::path::Path, content: &str) {
    let globals_dir = dir.join(".oxy/globals");
    fs::create_dir_all(&globals_dir).unwrap();
    fs::write(globals_dir.join("semantics.yml"), content).unwrap();
}

#[test]
fn test_incremental_build_no_changes() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let semantic_dir = project_root.join("semantics");
    let target_dir = project_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial view
    create_view_file(
        project_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Create initial manifest
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash(
        "views/orders.view.yml",
        oxy_semantic::hash_file(&semantic_dir.join("views/orders.view.yml")).unwrap(),
    );
    manifest.set_config_hash("config_hash".to_string());
    manifest.set_globals_hash("globals_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Run change detection
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), "globals_hash".to_string(), false)
        .unwrap();

    // No changes should be detected
    assert!(!result.requires_full_rebuild);
    assert!(result.is_empty());
    assert_eq!(result.views_to_rebuild.len(), 0);
}

#[test]
fn test_incremental_build_view_modified() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let semantic_dir = project_root.join("semantics");
    let target_dir = project_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial view
    create_view_file(
        project_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Create initial manifest with old hash
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash("views/orders.view.yml", "old_hash".to_string());
    manifest.set_config_hash("config_hash".to_string());
    manifest.set_globals_hash("globals_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Modify the view
    create_view_file(
        project_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders_v2.csv  # Modified
"#,
    );

    // Run change detection
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), "globals_hash".to_string(), false)
        .unwrap();

    // Should detect the modified view
    assert!(!result.requires_full_rebuild);
    assert_eq!(result.views_to_rebuild.len(), 1);
    assert!(result.views_to_rebuild.contains(&"orders".to_string()));
}

#[test]
fn test_incremental_build_view_added() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let semantic_dir = project_root.join("semantics");
    let target_dir = project_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial view
    create_view_file(
        project_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Create initial manifest
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash(
        "views/orders.view.yml",
        oxy_semantic::hash_file(&semantic_dir.join("views/orders.view.yml")).unwrap(),
    );
    manifest.set_config_hash("config_hash".to_string());
    manifest.set_globals_hash("globals_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Add new view
    create_view_file(
        project_root,
        "customers",
        r#"
name: customers
datasource: local
table: customers.csv
"#,
    );

    // Run change detection
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), "globals_hash".to_string(), false)
        .unwrap();

    // Should detect the new view
    assert!(!result.requires_full_rebuild);
    assert_eq!(result.views_to_rebuild.len(), 1);
    assert!(result.views_to_rebuild.contains(&"customers".to_string()));
}

#[test]
fn test_incremental_build_view_deleted() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let semantic_dir = project_root.join("semantics");
    let target_dir = project_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial manifest with two views
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash("views/orders.view.yml", "hash1".to_string());
    manifest.add_file_hash("views/customers.view.yml", "hash2".to_string());
    manifest.add_output_mapping(
        "views/customers.view.yml",
        vec![".semantics/model/customers.yml".to_string()],
    );
    manifest.set_config_hash("config_hash".to_string());
    manifest.set_globals_hash("globals_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Create only orders view (customers deleted)
    create_view_file(
        project_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Run change detection
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), "globals_hash".to_string(), false)
        .unwrap();

    // Should detect deleted file
    assert!(!result.requires_full_rebuild);
    assert_eq!(result.files_to_delete.len(), 1);
    assert_eq!(
        result.files_to_delete[0].to_string_lossy(),
        ".semantics/model/customers.yml"
    );
}

#[test]
fn test_incremental_build_with_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let semantic_dir = project_root.join("semantics");
    let target_dir = project_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create views
    create_view_file(
        project_root,
        "customers",
        r#"
name: customers
datasource: local
table: customers.csv
"#,
    );

    create_view_file(
        project_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Create manifest with dependency: orders depends on customers
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash("views/customers.view.yml", "old_hash".to_string());
    manifest.add_file_hash(
        "views/orders.view.yml",
        oxy_semantic::hash_file(&semantic_dir.join("views/orders.view.yml")).unwrap(),
    );

    let mut dep_graph = BTreeMap::new();
    dep_graph.insert("orders".to_string(), vec!["customers".to_string()]);
    manifest.set_dependency_graph(dep_graph);

    manifest.set_config_hash("config_hash".to_string());
    manifest.set_globals_hash("globals_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Modify customers view
    create_view_file(
        project_root,
        "customers",
        r#"
name: customers
datasource: local
table: customers_v2.csv  # Modified
"#,
    );

    // Run change detection
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), "globals_hash".to_string(), false)
        .unwrap();

    // Should rebuild both customers and orders (dependency)
    assert!(!result.requires_full_rebuild);
    assert_eq!(result.views_to_rebuild.len(), 2);
    assert!(result.views_to_rebuild.contains(&"customers".to_string()));
    assert!(result.views_to_rebuild.contains(&"orders".to_string()));
}

#[test]
fn test_full_rebuild_on_globals_change() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let semantic_dir = project_root.join("semantics");
    let target_dir = project_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create view
    create_view_file(
        project_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Create initial globals
    create_globals_file(project_root, "version: 1");

    // Create manifest with old globals hash
    let old_globals_hash = hash_globals_registry(&project_root.join(".oxy/globals")).unwrap();
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash(
        "views/orders.view.yml",
        oxy_semantic::hash_file(&semantic_dir.join("views/orders.view.yml")).unwrap(),
    );
    manifest.set_config_hash("config_hash".to_string());
    manifest.set_globals_hash(old_globals_hash);
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Modify globals
    create_globals_file(project_root, "version: 2");

    // Run change detection
    let new_globals_hash = hash_globals_registry(&project_root.join(".oxy/globals")).unwrap();
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), new_globals_hash, false)
        .unwrap();

    // Should trigger full rebuild
    assert!(result.requires_full_rebuild);
    assert_eq!(
        result.full_rebuild_reason,
        Some("Globals changed".to_string())
    );
}

#[test]
fn test_full_rebuild_on_config_change() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let semantic_dir = project_root.join("semantics");
    let target_dir = project_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create view
    create_view_file(
        project_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Create manifest with old config hash
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash(
        "views/orders.view.yml",
        oxy_semantic::hash_file(&semantic_dir.join("views/orders.view.yml")).unwrap(),
    );
    manifest.set_config_hash("old_config".to_string());
    manifest.set_globals_hash("globals_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Run change detection with new config
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("new_config".to_string(), "globals_hash".to_string(), false)
        .unwrap();

    // Should trigger full rebuild
    assert!(result.requires_full_rebuild);
    assert_eq!(
        result.full_rebuild_reason,
        Some("Database configuration changed".to_string())
    );
}

#[test]
fn test_topics_incremental_build() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let semantic_dir = project_root.join("semantics");
    let target_dir = project_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial topic
    create_topic_file(
        project_root,
        "sales",
        r#"
name: sales
base_view: orders
"#,
    );

    // Create manifest with old hash
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash("topics/sales.topic.yml", "old_hash".to_string());
    manifest.set_config_hash("config_hash".to_string());
    manifest.set_globals_hash("globals_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Modify topic
    create_topic_file(
        project_root,
        "sales",
        r#"
name: sales
base_view: orders_v2  # Modified
"#,
    );

    // Run change detection
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), "globals_hash".to_string(), false)
        .unwrap();

    // Should detect modified topic
    assert!(!result.requires_full_rebuild);
    assert_eq!(result.topics_to_rebuild.len(), 1);
    assert!(result.topics_to_rebuild.contains(&"sales".to_string()));
}
