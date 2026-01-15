use oxy::config::model::DatabaseType;
use oxy::config::parse_config;
use std::path::PathBuf;

#[test]
fn test_clickhouse_filter_schema_parsing() {
    // Set required environment variable for test
    unsafe {
        std::env::set_var("OPENAI_API_KEY", "test_key");
        std::env::set_var("CLICKHOUSE_PASSWORD", "test_password");
    }

    let config_path = PathBuf::from("tests/fixtures/clickhouse_filters");
    let project_path = config_path.clone();
    let config_file = config_path.join("config.yml");
    let result = parse_config(&config_file, project_path);

    assert!(
        result.is_ok(),
        "Failed to parse config with ClickHouse filters: {:?}",
        result.err()
    );

    let config = result.unwrap();

    // Verify database exists
    assert_eq!(config.databases.len(), 1);
    let db = &config.databases[0];
    assert_eq!(db.name, "clickhouse_test");

    // Verify ClickHouse-specific fields
    if let DatabaseType::ClickHouse(ch) = &db.database_type {
        // Verify role
        assert_eq!(ch.role, Some("app_readonly".to_string()));

        // Verify settings_prefix
        assert_eq!(ch.settings_prefix, Some("SQL_".to_string()));

        // Verify filters
        assert_eq!(ch.filters.len(), 3);
        assert!(ch.filters.contains_key("account_id"));
        assert!(ch.filters.contains_key("user_id"));
        assert!(ch.filters.contains_key("opening_ids"));

        // Verify account_id filter schema
        let account_id_schema = &ch.filters["account_id"];
        if let Some(instance_type) = &account_id_schema.instance_type {
            assert!(
                format!("{:?}", instance_type).contains("Integer"),
                "account_id should be integer type"
            );
        }

        // Verify user_id filter schema
        let user_id_schema = &ch.filters["user_id"];
        if let Some(instance_type) = &user_id_schema.instance_type {
            assert!(
                format!("{:?}", instance_type).contains("Integer"),
                "user_id should be integer type"
            );
        }

        // Verify opening_ids filter schema (array type with default)
        let opening_ids_schema = &ch.filters["opening_ids"];
        if let Some(instance_type) = &opening_ids_schema.instance_type {
            assert!(
                format!("{:?}", instance_type).contains("Array"),
                "opening_ids should be array type"
            );
        }
        // Check for default value
        if let Some(metadata) = &opening_ids_schema.metadata {
            assert!(
                metadata.default.is_some(),
                "opening_ids should have a default value"
            );
        }
    } else {
        panic!("Expected ClickHouse database type");
    }
}

#[test]
fn test_clickhouse_without_filters() {
    // Test that ClickHouse config without filters still works
    unsafe {
        std::env::set_var("OPENAI_API_KEY", "test_key");
    }

    let config_yaml = r#"
        databases:
          - name: clickhouse_simple
            type: clickhouse
            host: https://test.clickhouse.cloud:8443
            user: test_user
            password: test_password
            database: test_db

        models:
          - name: test_model
            vendor: openai
            model_ref: gpt-4
            key_var: OPENAI_API_KEY
    "#;

    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("config.yml");
    std::fs::write(&config_path, config_yaml).unwrap();

    let result = parse_config(&config_path, temp_dir.path().to_path_buf());

    assert!(
        result.is_ok(),
        "Failed to parse config without filters: {:?}",
        result.err()
    );

    let config = result.unwrap();
    let db = &config.databases[0];

    if let DatabaseType::ClickHouse(ch) = &db.database_type {
        assert!(ch.filters.is_empty(), "Filters should be empty by default");
        assert!(ch.role.is_none(), "Role should be None by default");
        assert!(
            ch.settings_prefix.is_none(),
            "Settings prefix should be None by default"
        );
    } else {
        panic!("Expected ClickHouse database type");
    }
}
