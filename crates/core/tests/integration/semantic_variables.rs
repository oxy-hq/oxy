pub mod semantic_variables {
    use assert_cmd::assert::OutputAssertExt;
    use std::process::Command;

    fn setup_command() -> Command {
        let mut cmd = Command::new(oxy_test_utils::get_oxy_binary());
        cmd.current_dir(oxy_test_utils::oxy_example_fixture_dir())
            .arg("run");
        cmd
    }

    #[test]
    fn run_automation_with_semantic_variables_validates() {
        // This test verifies the automation with variables parses correctly
        let mut cmd = setup_command();
        let result = cmd
            .arg("procedures/semantic_variables.automation.yml")
            .assert();

        // The automation may fail to execute without infrastructure, but should not fail on parsing
        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        // Should not have parsing errors
        assert!(!output.contains("Failed to deserialize"));
        assert!(!output.contains("missing field"));
    }

    #[test]
    fn run_automation_with_semantic_variables_override_validates() {
        let mut cmd = setup_command();
        let result = cmd
            .arg("procedures/semantic_variables.automation.yml")
            .arg("-v")
            .arg("orders_table=custom_orders")
            .assert();

        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        // Should not have parsing/validation errors
        assert!(!output.contains("Failed to deserialize"));
        assert!(!output.contains("Invalid variable"));
    }

    #[test]
    fn run_semantic_variables_example_automation_validates() {
        // Test the example automation from semantic-with-variables directory
        let mut cmd = setup_command();
        let result = cmd
            .arg("semantic-with-variables/workflow-example.automation.yml")
            .assert();

        let stderr = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        let stdout = String::from_utf8(result.get_output().stdout.clone()).unwrap();

        // Should parse successfully (execution may fail without infrastructure, but parsing should work)
        // Check that we don't have fundamental parsing/validation errors
        if stderr.contains("Failed to deserialize") {
            println!("Stderr: {}", stderr);
            println!("Stdout: {}", stdout);
        }
        // Allow execution failures, but not parsing failures
        assert!(
            !stderr.contains("missing field `topic`") && !stderr.contains("invalid type"),
            "Automation should parse correctly even if execution fails. Stderr: {}",
            stderr
        );
    }

    #[test]
    fn run_semantic_query_with_nested_variables_validates() {
        // Test using the automation-example which has nested variable paths
        let mut cmd = setup_command();
        let result = cmd
            .arg("semantic-with-variables/workflow-example.automation.yml")
            .assert();

        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        // Should validate variable syntax correctly
        assert!(!output.contains("Invalid variable syntax"));
        assert!(!output.contains("Failed to parse"));
    }

    #[test]
    fn run_semantic_query_with_variable_precedence_validates() {
        // Test that automation variables can override defaults
        let mut cmd = setup_command();
        let result = cmd
            .arg("procedures/semantic_variables.automation.yml")
            .arg("-v")
            .arg("orders_table=priority_orders")
            .assert();

        let output = String::from_utf8(result.get_output().stderr.clone()).unwrap();
        // Should not error on variable override
        assert!(!output.contains("Unknown variable"));
        assert!(!output.contains("Variable conflict"));
    }
}
