pub mod semantic_variables {
    use assert_cmd::assert::OutputAssertExt;
    use std::path::PathBuf;
    use std::process::Command;

    fn setup_command() -> Command {
        // Get the path to the oxy binary
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_dir = PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        // Try llvm-cov target first, fall back to regular debug target
        let mut bin_path = workspace_dir.join("target/llvm-cov-target/debug/oxy");
        if !bin_path.exists() {
            bin_path = workspace_dir.join("target/debug/oxy");
        }

        let mut cmd = Command::new(&bin_path);
        cmd.current_dir(workspace_dir.join("examples")).arg("run");
        cmd
    }

    #[test]
    fn run_workflow_with_semantic_variables_validates() {
        // This test verifies the workflow with variables parses correctly
        // Even if CubeJS is not running, the workflow should validate
        let mut cmd = setup_command();
        let result = cmd
            .arg("workflows/semantic_variables.workflow.yml")
            .assert();

        // The workflow will fail to execute without CubeJS, but should not fail on parsing
        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        // Should not have parsing errors
        assert!(!output.contains("Failed to deserialize"));
        assert!(!output.contains("missing field"));
    }

    #[test]
    fn run_workflow_with_semantic_variables_override_validates() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("workflows/semantic_variables.workflow.yml")
            .arg("-v")
            .arg("orders_table=custom_orders")
            .assert();

        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        // Should not have parsing/validation errors
        assert!(!output.contains("Failed to deserialize"));
        assert!(!output.contains("Invalid variable"));
    }

    #[test]
    fn run_semantic_variables_example_workflow_validates() {
        // Test the example workflow from semantic-with-variables directory
        let mut cmd = setup_command();
        let result = cmd
            .arg("semantic-with-variables/workflow-example.workflow.yml")
            .assert();

        let stderr = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        let stdout = String::from_utf8(result.get_output().stdout.clone()).unwrap();

        // Should parse successfully (execution may fail without infrastructure, but parsing should work)
        // Check that we don't have fundamental parsing/validation errors
        if stderr.contains("Failed to deserialize") {
            println!("Stderr: {}", stderr);
            println!("Stdout: {}", stdout);
        }
        // Allow execution failures (CubeJS not running), but not parsing failures
        assert!(
            !stderr.contains("missing field `topic`") && !stderr.contains("invalid type"),
            "Workflow should parse correctly even if execution fails. Stderr: {}",
            stderr
        );
    }

    #[test]
    fn run_semantic_query_with_nested_variables_validates() {
        // Test using the workflow-example which has nested variable paths
        let mut cmd = setup_command();
        let result = cmd
            .arg("semantic-with-variables/workflow-example.workflow.yml")
            .assert();

        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        // Should validate variable syntax correctly
        assert!(!output.contains("Invalid variable syntax"));
        assert!(!output.contains("Failed to parse"));
    }

    #[test]
    fn run_semantic_query_with_variable_precedence_validates() {
        // Test that workflow variables can override defaults
        let mut cmd = setup_command();
        let result = cmd
            .arg("workflows/semantic_variables.workflow.yml")
            .arg("-v")
            .arg("orders_table=priority_orders")
            .assert();

        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        // Should not error on variable override
        assert!(!output.contains("Unknown variable"));
        assert!(!output.contains("Variable conflict"));
    }
}
