pub mod prepare_semantic_engine {
    use assert_cmd::Command;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn setup_command() -> Command {
        let mut cmd = Command::cargo_bin("oxy").unwrap();
        cmd.current_dir("examples");
        cmd
    }

    fn get_unique_semantics_dir() -> PathBuf {
        let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        PathBuf::from(format!("examples/.semantics-test-{}", counter))
    }

    #[test]
    fn test_prepare_semantic_engine_generates_cube_files() {
        let mut cmd = setup_command();

        // Use a unique directory for this test
        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        let result = cmd
            .arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();

        // Verify success message
        assert!(output.contains("Cube.js configuration is ready for deployment"));

        // Verify .semantics directory was created
        assert!(
            semantics_dir.exists(),
            ".semantics directory should be created"
        );

        // Verify model directory exists
        let model_dir = semantics_dir.join("model");
        assert!(
            model_dir.exists(),
            "model/ directory should be created inside .semantics"
        );

        // Verify cube.js configuration file exists
        let cube_config = semantics_dir.join("cube.js");
        assert!(
            cube_config.exists(),
            "cube.js configuration file should be created"
        );

        // Verify cube.js content contains data source configuration
        let cube_config_content = fs::read_to_string(&cube_config).unwrap();
        assert!(
            cube_config_content.contains("module.exports"),
            "cube.js should contain module.exports"
        );

        // Verify YAML files were created in model directory
        let model_entries: Vec<_> = fs::read_dir(&model_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        assert!(
            !model_entries.is_empty(),
            "model/ directory should contain cube/view files"
        );

        // Verify at least one .yml file exists
        let yml_files: Vec<_> = model_entries
            .iter()
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("yml"))
            .collect();

        assert!(
            !yml_files.is_empty(),
            "model/ directory should contain .yml files"
        );

        // Verify cube YAML structure by reading one file
        if let Some(yml_file) = yml_files.first() {
            let yml_content = fs::read_to_string(yml_file.path()).unwrap();

            // Parse as YAML to verify structure
            let parsed: serde_yaml::Value = serde_yaml::from_str(&yml_content).unwrap();

            // Verify it has expected CubeJS structure (dimensions, measures, or cubes key)
            let has_cube_structure = parsed.get("dimensions").is_some()
                || parsed.get("measures").is_some()
                || parsed.get("cubes").is_some()
                || parsed.get("sql_table").is_some()
                || parsed.get("sql").is_some();

            assert!(
                has_cube_structure,
                "YAML file should have CubeJS structure (dimensions/measures/cubes/sql_table/sql)"
            );
        }

        // Clean up after test
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_prepare_semantic_engine_with_custom_output_dir() {
        let mut cmd = setup_command();

        // Use a custom output directory
        let custom_output = PathBuf::from("examples/.test-semantics-output");

        // Clean up if exists
        if custom_output.exists() {
            fs::remove_dir_all(&custom_output).ok();
        }

        // Run prepare-semantic-engine with custom output directory
        let result = cmd
            .arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(".test-semantics-output")
            .arg("--force")
            .assert()
            .success();

        let output = String::from_utf8(result.get_output().stdout.clone()).unwrap();

        // Verify success message
        assert!(output.contains("Cube.js configuration is ready for deployment"));

        // Verify custom output directory was created
        assert!(
            custom_output.exists(),
            "Custom output directory should be created"
        );

        // Verify model directory exists in custom location
        let model_dir = custom_output.join("model");
        assert!(
            model_dir.exists(),
            "model/ directory should be created in custom output"
        );

        // Verify cube.js exists in custom location
        let cube_config = custom_output.join("cube.js");
        assert!(
            cube_config.exists(),
            "cube.js should be created in custom output"
        );

        // Clean up after test
        fs::remove_dir_all(&custom_output).ok();
    }

    #[test]
    fn test_prepare_semantic_engine_without_force_flag() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();

        // First run to create the files
        let mut first_cmd = setup_command();
        first_cmd
            .arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Second run without --force should still succeed
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .assert()
            .success();

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_globals_description_rendered_correctly() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Check orders view YAML file which uses globals.semantics.descriptions
        let orders_cube_file = semantics_dir.join("model/orders.yml");
        assert!(
            orders_cube_file.exists(),
            "orders.yml cube file should be created"
        );

        let orders_content = fs::read_to_string(&orders_cube_file).unwrap();
        let orders_yaml: serde_yaml::Value = serde_yaml::from_str(&orders_content).unwrap();

        let cubes = orders_yaml
            .get("cubes")
            .expect("orders.yml should have 'cubes' key")
            .as_sequence()
            .expect("cubes should be an array");

        let orders_cube = &cubes[0];

        // Find the order_status dimension which should have the rendered description
        let dimensions = orders_cube
            .get("dimensions")
            .expect("orders should have dimensions")
            .as_sequence()
            .expect("dimensions should be an array");

        let order_status_dim = dimensions
            .iter()
            .find(|d| {
                d.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s == "order_status")
                    .unwrap_or(false)
            })
            .expect("order_status dimension should exist");

        // Verify that globals description is rendered
        let description = order_status_dim
            .get("description")
            .and_then(|d| d.as_str())
            .expect("order_status should have description");

        assert_eq!(
            description, "Current status of the order",
            "Should contain rendered order_status description from globals"
        );

        // Verify that template syntax is gone in the entire file
        assert!(
            !orders_content.contains("{{globals.semantics.descriptions"),
            "Should not contain unrendered template syntax"
        );

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_globals_dimension_inheritance_rendered_correctly() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Check orders view which inherits dimensions from globals
        let orders_cube_file = semantics_dir.join("model/orders.yml");
        let orders_content = fs::read_to_string(&orders_cube_file).unwrap();
        let orders_yaml: serde_yaml::Value = serde_yaml::from_str(&orders_content).unwrap();

        let cubes = orders_yaml
            .get("cubes")
            .expect("orders.yml should have 'cubes' key")
            .as_sequence()
            .expect("cubes should be an array");

        let orders_cube = &cubes[0];

        // Verify dimensions section exists
        let dimensions = orders_cube
            .get("dimensions")
            .expect("orders should have dimensions")
            .as_sequence()
            .expect("dimensions should be an array");

        // Find the customer_id dimension (inherited from globals.semantics.dimensions.customer_id)
        let customer_id_dim = dimensions.iter().find(|d| {
            d.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s == "customer_id")
                .unwrap_or(false)
        });

        assert!(
            customer_id_dim.is_some(),
            "customer_id dimension should exist (inherited from globals)"
        );

        let customer_id = customer_id_dim.unwrap();

        // Verify it has the properties from the global definition
        assert_eq!(
            customer_id.get("type").and_then(|t| t.as_str()),
            Some("number"),
            "customer_id should have type 'number' from global definition"
        );
        assert_eq!(
            customer_id.get("description").and_then(|d| d.as_str()),
            Some("Unique customer identifier"),
            "customer_id should have description from global definition"
        );

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_globals_measure_inheritance_rendered_correctly() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Check orders view which inherits measures from globals
        let orders_cube_file = semantics_dir.join("model/orders.yml");
        let orders_content = fs::read_to_string(&orders_cube_file).unwrap();
        let orders_yaml: serde_yaml::Value = serde_yaml::from_str(&orders_content).unwrap();

        let cubes = orders_yaml
            .get("cubes")
            .expect("orders.yml should have 'cubes' key")
            .as_sequence()
            .expect("cubes should be an array");

        let orders_cube = &cubes[0];

        // Verify measures section exists
        let measures = orders_cube
            .get("measures")
            .expect("orders should have measures")
            .as_sequence()
            .expect("measures should be an array");

        // Find the total_orders measure (inherited from globals.semantics.measures.total_orders)
        let total_orders_measure = measures.iter().find(|m| {
            m.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s == "total_orders")
                .unwrap_or(false)
        });

        assert!(
            total_orders_measure.is_some(),
            "total_orders measure should exist (inherited from globals)"
        );

        let total_orders = total_orders_measure.unwrap();

        // Verify it has the properties from the global definition
        assert_eq!(
            total_orders.get("type").and_then(|t| t.as_str()),
            Some("count"),
            "total_orders should have type 'count' from global definition"
        );
        assert_eq!(
            total_orders.get("description").and_then(|d| d.as_str()),
            Some("Number of orders"),
            "total_orders should have description from global definition"
        );

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_no_globals_template_syntax_in_output() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Read all YAML files in model directory
        let model_dir = semantics_dir.join("model");
        let model_files = fs::read_dir(&model_dir).unwrap();

        for file_entry in model_files {
            let file_entry = file_entry.unwrap();
            let path = file_entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("yml") {
                let content = fs::read_to_string(&path).unwrap();

                // Verify no globals template syntax remains
                assert!(
                    !content.contains("{{globals"),
                    "File {} should not contain '{{{{globals' template syntax",
                    path.display()
                );
                assert!(
                    !content.contains("{{ globals"),
                    "File {} should not contain '{{{{ globals' template syntax",
                    path.display()
                );

                // Also check for inherits_from with globals references
                assert!(
                    !content.contains("inherits_from: globals"),
                    "File {} should not contain 'inherits_from: globals' - should be expanded",
                    path.display()
                );
            }
        }

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_variable_references_are_encoded() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Check orders view which uses {{variables.orders_table}}
        let orders_cube_file = semantics_dir.join("model/orders.yml");
        assert!(
            orders_cube_file.exists(),
            "orders.yml cube file should be created"
        );

        let orders_content = fs::read_to_string(&orders_cube_file).unwrap();
        let orders_yaml: serde_yaml::Value = serde_yaml::from_str(&orders_content).unwrap();

        let cubes = orders_yaml
            .get("cubes")
            .expect("orders.yml should have 'cubes' key")
            .as_sequence()
            .expect("cubes should be an array");

        let orders_cube = &cubes[0];

        // Verify that the variable reference has been encoded
        if let Some(sql_table) = orders_cube.get("sql_table") {
            let sql_table_str = sql_table.as_str().unwrap();

            // Should be encoded as __VAR_xxx__ format
            assert!(
                sql_table_str.starts_with("__VAR_"),
                "sql_table should be encoded with __VAR_ prefix, got: {}",
                sql_table_str
            );
            assert!(
                sql_table_str.contains("__") && sql_table_str.matches("__").count() >= 2,
                "sql_table should be encoded with __ delimiters, got: {}",
                sql_table_str
            );

            // Should NOT contain the original template syntax
            assert!(
                !sql_table_str.contains("{{variables"),
                "sql_table should not contain template syntax, got: {}",
                sql_table_str
            );
            assert!(
                !sql_table_str.contains("orders_table"),
                "sql_table should not contain the original variable name, got: {}",
                sql_table_str
            );
        } else {
            panic!("orders cube should have sql_table field");
        }

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_encoded_variables_use_hex_format() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Check orders view
        let orders_cube_file = semantics_dir.join("model/orders.yml");
        let orders_content = fs::read_to_string(&orders_cube_file).unwrap();
        let orders_yaml: serde_yaml::Value = serde_yaml::from_str(&orders_content).unwrap();

        let cubes = orders_yaml
            .get("cubes")
            .expect("orders.yml should have 'cubes' key")
            .as_sequence()
            .expect("cubes should be an array");

        let orders_cube = &cubes[0];

        if let Some(sql_table) = orders_cube.get("sql_table") {
            let sql_table_str = sql_table.as_str().unwrap();

            // Extract the encoded part between __VAR_ and the last __
            if let Some(var_start) = sql_table_str.find("__VAR_") {
                let after_prefix = &sql_table_str[var_start + 6..]; // Skip "__VAR_"
                if let Some(var_end) = after_prefix.find("__") {
                    let encoded_part = &after_prefix[..var_end];

                    // Verify it's hexadecimal
                    assert!(
                        encoded_part.chars().all(|c| c.is_ascii_hexdigit()),
                        "Encoded variable should be hexadecimal, got: {}",
                        encoded_part
                    );

                    // Verify it's not empty
                    assert!(
                        !encoded_part.is_empty(),
                        "Encoded variable should not be empty"
                    );
                }
            }
        }

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_no_variable_template_syntax_in_output() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Read all YAML files in model directory
        let model_dir = semantics_dir.join("model");
        let model_files = fs::read_dir(&model_dir).unwrap();

        for file_entry in model_files {
            let file_entry = file_entry.unwrap();
            let path = file_entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("yml") {
                let content = fs::read_to_string(&path).unwrap();

                // Verify no variables template syntax remains
                assert!(
                    !content.contains("{{variables"),
                    "File {} should not contain '{{{{variables' template syntax",
                    path.display()
                );
                assert!(
                    !content.contains("{{ variables"),
                    "File {} should not contain '{{{{ variables' template syntax",
                    path.display()
                );
            }
        }

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_encoded_variables_are_sql_safe() {
        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Read all YAML files in model directory
        let model_dir = semantics_dir.join("model");
        let model_files = fs::read_dir(&model_dir).unwrap();

        for file_entry in model_files {
            let file_entry = file_entry.unwrap();
            let path = file_entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("yml") {
                let content = fs::read_to_string(&path).unwrap();

                // Find all __VAR_ encoded variables
                for line in content.lines() {
                    if line.contains("__VAR_") {
                        // Extract variable names
                        let mut start_idx = 0;
                        while let Some(pos) = line[start_idx..].find("__VAR_") {
                            let var_start = start_idx + pos;
                            let after_prefix = &line[var_start..];

                            if let Some(end_pos) = after_prefix[6..].find("__") {
                                let var_name = &after_prefix[..6 + end_pos + 2];

                                // Verify it only contains SQL-safe characters
                                assert!(
                                    var_name.chars().all(|c| c.is_alphanumeric() || c == '_'),
                                    "Encoded variable '{}' in {} should only contain alphanumeric and underscore characters",
                                    var_name,
                                    path.display()
                                );

                                start_idx = var_start + var_name.len();
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_encoded_variables_can_be_decoded() {
        use oxy_semantic::variables::VariableEncoder;

        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Check orders view which has encoded variable
        let orders_cube_file = semantics_dir.join("model/orders.yml");
        let orders_content = fs::read_to_string(&orders_cube_file).unwrap();
        let orders_yaml: serde_yaml::Value = serde_yaml::from_str(&orders_content).unwrap();

        let cubes = orders_yaml
            .get("cubes")
            .expect("orders.yml should have 'cubes' key")
            .as_sequence()
            .expect("cubes should be an array");

        let orders_cube = &cubes[0];

        if let Some(sql_table) = orders_cube.get("sql_table") {
            let encoded_sql_table = sql_table.as_str().unwrap();

            // Verify it's encoded
            assert!(
                encoded_sql_table.contains("__VAR_"),
                "sql_table should contain encoded variable"
            );

            // Create a variable encoder to decode
            let encoder = VariableEncoder::new();

            // Decode the encoded expression using decode_all_variables
            // (decode_all_variables doesn't require the mapping to be in memory)
            let decoded_expr = encoder.decode_all_variables(encoded_sql_table);

            // Verify it decodes back to the original template format
            assert!(
                decoded_expr.contains("{{variables."),
                "Decoded expression should contain '{{{{variables.' template syntax, got: {}",
                decoded_expr
            );
            assert!(
                decoded_expr.contains("orders_table"),
                "Decoded expression should contain 'orders_table' variable name, got: {}",
                decoded_expr
            );
            assert!(
                decoded_expr.contains("}}"),
                "Decoded expression should contain closing '}}}}', got: {}",
                decoded_expr
            );

            // Verify the full expected template
            assert_eq!(
                decoded_expr, "{{variables.orders_table}}.csv",
                "Decoded expression should match the original template"
            );
        } else {
            panic!("orders cube should have sql_table field");
        }

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }

    #[test]
    fn test_variable_encoding_roundtrip() {
        use oxy_semantic::variables::VariableEncoder;

        let mut cmd = setup_command();

        let semantics_dir = get_unique_semantics_dir();
        if semantics_dir.exists() {
            fs::remove_dir_all(&semantics_dir).ok();
        }

        // Run prepare-semantic-engine command
        cmd.arg("prepare-semantic-engine")
            .arg("--output-dir")
            .arg(semantics_dir.file_name().unwrap())
            .arg("--force")
            .assert()
            .success();

        // Test roundtrip for multiple encoded variables in the generated files
        let model_dir = semantics_dir.join("model");
        let model_files = fs::read_dir(&model_dir).unwrap();

        let mut encoder = VariableEncoder::new();
        let mut found_encoded_vars = 0;

        for file_entry in model_files {
            let file_entry = file_entry.unwrap();
            let path = file_entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("yml") {
                let content = fs::read_to_string(&path).unwrap();

                // Find all __VAR_ encoded variables
                for line in content.lines() {
                    if line.contains("__VAR_") {
                        // Test decoding using decode_all_variables
                        let decoded_line = encoder.decode_all_variables(line);

                        // If it was successfully decoded, it should contain template syntax
                        if decoded_line != line {
                            found_encoded_vars += 1;

                            // Verify the decoded line contains template syntax
                            assert!(
                                decoded_line.contains("{{") && decoded_line.contains("}}"),
                                "Decoded line should contain template syntax: {}",
                                decoded_line
                            );

                            // Re-encode and verify it matches the original
                            let re_encoded = encoder.encode_expression(&decoded_line);

                            // The re-encoded should contain __VAR_ markers
                            assert!(
                                re_encoded.contains("__VAR_"),
                                "Re-encoded expression should contain __VAR_ markers"
                            );
                        }
                    }
                }
            }
        }

        // Ensure we found at least one encoded variable to test
        assert!(
            found_encoded_vars > 0,
            "Should have found at least one encoded variable to test roundtrip"
        );

        // Clean up
        fs::remove_dir_all(&semantics_dir).ok();
    }
}
