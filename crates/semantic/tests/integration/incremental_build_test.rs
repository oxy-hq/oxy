use oxy_semantic::{BuildManifest, ChangeDetector};
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

#[test]
fn test_incremental_build_no_changes() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();
    let semantic_dir = workspace_root.join("semantics");
    let target_dir = workspace_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial view
    create_view_file(
        workspace_root,
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
        "semantics/views/orders.view.yml",
        oxy_semantic::hash_file(&workspace_root.join("semantics/views/orders.view.yml")).unwrap(),
    );
    manifest.set_config_hash("config_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Run change detection
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), false)
        .unwrap();

    // No changes should be detected
    assert!(!result.requires_full_rebuild);
    assert!(result.is_empty());
    assert_eq!(result.views_to_rebuild.len(), 0);
}

#[test]
fn test_incremental_build_view_modified() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();
    let semantic_dir = workspace_root.join("semantics");
    let target_dir = workspace_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial view
    create_view_file(
        workspace_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Create initial manifest with old hash
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash("semantics/views/orders.view.yml", "old_hash".to_string());
    manifest.set_config_hash("config_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Modify the view
    create_view_file(
        workspace_root,
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
        .detect_changes("config_hash".to_string(), false)
        .unwrap();

    // Should trigger full rebuild (semantic layer always rebuilds on any change)
    assert!(result.requires_full_rebuild);
    assert_eq!(
        result.full_rebuild_reason,
        Some("Semantic layer files changed".to_string())
    );
}

#[test]
fn test_incremental_build_view_added() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();
    let semantic_dir = workspace_root.join("semantics");
    let target_dir = workspace_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial view
    create_view_file(
        workspace_root,
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
        "semantics/views/orders.view.yml",
        oxy_semantic::hash_file(&workspace_root.join("semantics/views/orders.view.yml")).unwrap(),
    );
    manifest.set_config_hash("config_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Add new view
    create_view_file(
        workspace_root,
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
        .detect_changes("config_hash".to_string(), false)
        .unwrap();

    // Should trigger full rebuild (semantic layer always rebuilds on any change)
    assert!(result.requires_full_rebuild);
    assert_eq!(
        result.full_rebuild_reason,
        Some("Semantic layer files changed".to_string())
    );
}

#[test]
fn test_incremental_build_view_deleted() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();
    let semantic_dir = workspace_root.join("semantics");
    let target_dir = workspace_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial manifest with two views
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash("semantics/views/orders.view.yml", "hash1".to_string());
    manifest.add_file_hash("semantics/views/customers.view.yml", "hash2".to_string());
    manifest.add_output_mapping(
        "semantics/views/customers.view.yml",
        vec![".semantics/model/customers.yml".to_string()],
    );
    manifest.set_config_hash("config_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Create only orders view (customers deleted)
    create_view_file(
        workspace_root,
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
        .detect_changes("config_hash".to_string(), false)
        .unwrap();

    // Should trigger full rebuild (semantic layer always rebuilds on any change)
    assert!(result.requires_full_rebuild);
    assert_eq!(
        result.full_rebuild_reason,
        Some("Semantic layer files changed".to_string())
    );
}

#[test]
fn test_incremental_build_with_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();
    let semantic_dir = workspace_root.join("semantics");
    let target_dir = workspace_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create views
    create_view_file(
        workspace_root,
        "customers",
        r#"
name: customers
datasource: local
table: customers.csv
"#,
    );

    create_view_file(
        workspace_root,
        "orders",
        r#"
name: orders
datasource: local
table: orders.csv
"#,
    );

    // Create manifest with dependency: orders depends on customers
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash("semantics/views/customers.view.yml", "old_hash".to_string());
    manifest.add_file_hash(
        "semantics/views/orders.view.yml",
        oxy_semantic::hash_file(&workspace_root.join("semantics/views/orders.view.yml")).unwrap(),
    );

    let mut dep_graph = BTreeMap::new();
    dep_graph.insert("orders".to_string(), vec!["customers".to_string()]);
    manifest.set_dependency_graph(dep_graph);

    manifest.set_config_hash("config_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Modify customers view
    create_view_file(
        workspace_root,
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
        .detect_changes("config_hash".to_string(), false)
        .unwrap();

    // Should trigger full rebuild (semantic layer always rebuilds on any change)
    assert!(result.requires_full_rebuild);
    assert_eq!(
        result.full_rebuild_reason,
        Some("Semantic layer files changed".to_string())
    );
}

#[test]
fn test_full_rebuild_on_config_change() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();
    let semantic_dir = workspace_root.join("semantics");
    let target_dir = workspace_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create view
    create_view_file(
        workspace_root,
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
        "semantics/views/orders.view.yml",
        oxy_semantic::hash_file(&workspace_root.join("semantics/views/orders.view.yml")).unwrap(),
    );
    manifest.set_config_hash("old_config".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Run change detection with new config
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("new_config".to_string(), false)
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
    let workspace_root = temp_dir.path();
    let semantic_dir = workspace_root.join("semantics");
    let target_dir = workspace_root.join(".semantics");

    fs::create_dir_all(&semantic_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    // Create initial topic
    create_topic_file(
        workspace_root,
        "sales",
        r#"
name: sales
base_view: orders
"#,
    );

    // Create manifest with old hash
    let mut manifest = BuildManifest::new();
    manifest.add_file_hash("semantics/topics/sales.topic.yml", "old_hash".to_string());
    manifest.set_config_hash("config_hash".to_string());
    manifest
        .save(&target_dir.join(".build_manifest.json"))
        .unwrap();

    // Modify topic
    create_topic_file(
        workspace_root,
        "sales",
        r#"
name: sales
base_view: orders_v2  # Modified
"#,
    );

    // Run change detection
    let detector = ChangeDetector::new(&semantic_dir, &target_dir);
    let result = detector
        .detect_changes("config_hash".to_string(), false)
        .unwrap();

    // Should trigger full rebuild (semantic layer always rebuilds on any change)
    assert!(result.requires_full_rebuild);
    assert_eq!(
        result.full_rebuild_reason,
        Some("Semantic layer files changed".to_string())
    );
}
