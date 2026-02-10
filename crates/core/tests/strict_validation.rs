//! Tests for strict validation with deny_unknown_fields.
//!
//! These tests verify that:
//! 1. Unknown fields are rejected with clear error messages
//! 2. Valid configs still parse correctly
//! 3. Flatten fields work correctly with deny_unknown_fields
//! 4. Empty required collections are caught by garde validation

use serde::de::DeserializeOwned;

fn parse_yaml<T: DeserializeOwned>(yaml: &str) -> Result<T, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

/// Helper to check if parsing rejects invalid config (unknown field OR missing required field)
/// When a struct has both a required field and deny_unknown_fields, using a typo
/// may result in either "unknown field" or "missing field" error depending on order.
fn assert_unknown_field_error_with_result<T>(result: Result<T, serde_yaml::Error>, field: &str) {
    match result {
        Ok(_) => panic!("Should reject unknown field '{field}', but parsing succeeded"),
        Err(err) => {
            let err_str = err.to_string();
            // Accept both "unknown field" and "missing field" errors
            // When using typo like "steps" instead of "tasks", serde may report:
            // - "unknown field `steps`" OR
            // - "missing field `tasks`" (if tasks is required)
            // Both indicate the config is invalid due to the typo
            assert!(
                err_str.contains("unknown field")
                    || err_str.contains("unknown variant")
                    || err_str.contains("missing field"),
                "Error should mention unknown or missing field for '{field}', got: {err_str}"
            );
        }
    }
}

// =============================================================================
// AppConfig Tests
// =============================================================================

mod app_config_tests {
    use super::*;
    use oxy::config::model::AppConfig;

    #[test]
    fn test_valid_app_config() {
        let yaml = r#"
tasks:
  - name: query
    type: execute_sql
    database: test_db
    sql_query: "SELECT 1"
display:
  - type: table
    data: query
"#;
        let result: Result<AppConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid app config should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_app_config_rejects_unknown_fields() {
        let yaml = r#"
tasks:
  - name: query
    type: execute_sql
    database: test_db
    sql_query: "SELECT 1"
display:
  - type: table
    data: query
unknown_field: "should fail"
"#;
        let result: Result<AppConfig, _> = parse_yaml(yaml);
        assert_unknown_field_error_with_result(result, "unknown_field");
    }

    #[test]
    fn test_app_config_rejects_steps_typo() {
        let yaml = r#"
steps:
  - name: query
    type: execute_sql
    database: test_db
    sql_query: "SELECT 1"
display:
  - type: table
    data: query
"#;
        let result: Result<AppConfig, _> = parse_yaml(yaml);
        assert_unknown_field_error_with_result(result, "steps");
    }

    #[test]
    fn test_app_config_with_name_field() {
        // Test that the `name` field is accepted in YAML (for backwards compatibility).
        // The name is typically derived from the filename at runtime.
        let yaml = r#"
name: my_app
tasks:
  - name: query
    type: execute_sql
    database: test_db
    sql_query: "SELECT 1"
display:
  - type: table
    data: query
"#;
        let result: Result<AppConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "App config with name field should parse: {:?}",
            result.err()
        );

        let app = result.unwrap();
        assert_eq!(app.name, "my_app");
    }
}

// =============================================================================
// Semantics Tests
// =============================================================================

mod semantics_tests {
    use super::*;
    use oxy::config::model::Semantics;

    #[test]
    fn test_valid_semantics() {
        let yaml = r#"
dimensions:
  - name: test_dimension
    targets:
      - target1
"#;
        let result: Result<Semantics, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid semantics should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_semantics_rejects_unknown_fields() {
        let yaml = r#"
dimensions:
  - name: test_dimension
    targets:
      - target1
unknown_field: "should fail"
"#;
        let result: Result<Semantics, _> = parse_yaml(yaml);
        assert_unknown_field_error_with_result(result, "unknown_field");
    }

    #[test]
    fn test_empty_dimensions() {
        let yaml = r#"
dimensions: []
"#;
        let result: Result<Semantics, _> = parse_yaml(yaml);
        // Empty dimensions should still parse (no min length requirement on Semantics)
        assert!(
            result.is_ok(),
            "Empty dimensions should parse: {:?}",
            result.err()
        );
    }
}

// =============================================================================
// Config Tests
// =============================================================================

mod config_tests {
    use super::*;
    use oxy::config::model::Config;

    #[test]
    fn test_valid_config() {
        let yaml = r#"
models:
  - name: test_model
    vendor: openai
    model_ref: gpt-4
    key_var: OPENAI_API_KEY
databases: []
"#;
        let result: Result<Config, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid config should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_config_rejects_unknown_root_fields() {
        let yaml = r#"
models:
  - name: test_model
    vendor: openai
    model_ref: gpt-4
    key_var: OPENAI_API_KEY
databases: []
unknown_setting: true
"#;
        let result: Result<Config, _> = parse_yaml(yaml);
        assert_unknown_field_error_with_result(result, "unknown_setting");
    }

    #[test]
    fn test_config_rejects_typo_model_field() {
        let yaml = r#"
model:
  - name: test_model
    vendor: openai
    model_ref: gpt-4
    key_var: OPENAI_API_KEY
databases: []
"#;
        let result: Result<Config, _> = parse_yaml(yaml);
        // "model" instead of "models" should be caught
        assert_unknown_field_error_with_result(result, "model");
    }
}

// =============================================================================
// Workflow Tests
// =============================================================================

mod workflow_tests {
    use super::*;
    use oxy::config::model::Workflow;

    #[test]
    fn test_valid_workflow() {
        let yaml = r#"
tasks:
  - name: test_task
    type: agent
    agent_ref: test.agent.yml
    prompt: "test prompt"
"#;
        let result: Result<Workflow, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid workflow should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_workflow_with_variables() {
        // Test that flatten for variables still works with deny_unknown_fields
        let yaml = r#"
tasks:
  - name: test_task
    type: agent
    agent_ref: test.agent.yml
    prompt: "test {{ my_var }}"
variables:
  my_var:
    type: string
    default: "hello"
"#;
        let result: Result<Workflow, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Workflow with variables should parse: {:?}",
            result.err()
        );

        let workflow = result.unwrap();
        assert!(workflow.variables.is_some(), "Variables should be parsed");
    }

    #[test]
    fn test_workflow_rejects_steps_typo() {
        // This is the main use case - catching the common "steps" vs "tasks" typo
        let yaml = r#"
steps:
  - name: test_task
    type: agent
    agent_ref: test.agent.yml
    prompt: "test prompt"
"#;
        let result: Result<Workflow, _> = parse_yaml(yaml);
        assert_unknown_field_error_with_result(result, "steps");
    }

    #[test]
    fn test_workflow_rejects_unknown_fields() {
        let yaml = r#"
tasks:
  - name: test_task
    type: agent
    agent_ref: test.agent.yml
    prompt: "test prompt"
unknown_field: "should fail"
"#;
        let result: Result<Workflow, _> = parse_yaml(yaml);
        assert_unknown_field_error_with_result(result, "unknown_field");
    }

    #[test]
    fn test_workflow_with_all_valid_fields() {
        // Test all valid Workflow fields to ensure they still work
        let yaml = r#"
tasks:
  - name: test_task
    type: agent
    agent_ref: test.agent.yml
    prompt: "test prompt"
description: "A test workflow"
tests: []
consistency_prompt: "Check consistency"
"#;
        let result: Result<Workflow, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Workflow with all valid fields should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_workflow_with_name_field() {
        // Test that the `name` field is accepted in YAML (for backwards compatibility).
        // The name is ignored during parsing and always derived from the filename at runtime.
        let yaml = r#"
name: my_workflow_name
tasks:
  - name: test_task
    type: agent
    agent_ref: test.agent.yml
    prompt: "test prompt"
"#;
        let result: Result<Workflow, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Workflow with name field should parse: {:?}",
            result.err()
        );

        // The name field should be parsed (defaults to empty if not set, but here it's set)
        let workflow = result.unwrap();
        assert_eq!(workflow.name, "my_workflow_name");
    }

    #[test]
    fn test_empty_tasks_parses_but_should_fail_validation() {
        // Empty tasks should parse at serde level, but fail garde validation
        let yaml = r#"
tasks: []
"#;
        let result: Result<Workflow, _> = parse_yaml(yaml);
        // Serde parsing should succeed
        assert!(result.is_ok(), "Empty tasks should parse at serde level");

        // But garde validation should fail (tested after adding #[garde(length(min = 1))])
        // This test documents expected behavior after the garde changes
    }
}

// =============================================================================
// AgentConfig Tests
// =============================================================================

mod agent_config_tests {
    use super::*;
    use oxy::config::model::{AgentConfig, AgentType, Config, ToolType};

    /// Create a minimal valid Config for validation context
    fn create_test_config() -> Config {
        let yaml = r#"
models:
  - name: test_model
    vendor: openai
    model_ref: gpt-4
    key_var: OPENAI_API_KEY
databases:
  - name: test_db
    type: bigquery
    project: test_project
    credentials_path: /tmp/creds.json
"#;
        parse_yaml(yaml).expect("Test config should parse")
    }

    #[test]
    fn test_valid_default_agent() {
        let yaml = r#"
model: gpt-4
system_instructions: "You are a helpful assistant"
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid default agent should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_valid_routing_agent() {
        let yaml = r#"
model: gpt-4
type: routing
system_instructions: "Route queries appropriately"
routes:
  - agent1.agent.yml
  - agent2.agent.yml
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid routing agent should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_agent_with_tools() {
        // Test that flatten for AgentToolsConfig works
        let yaml = r#"
model: gpt-4
system_instructions: "You are a helpful assistant"
tools:
  - type: execute_sql
    name: sql
    database: test_db
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Agent with tools should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_agent_tool_without_name_gets_default_name() {
        // Test that when a tool doesn't have a 'name' field, it gets a default name based on tool type
        let yaml = r#"
model: gpt-4
system_instructions: "You are a helpful assistant"
tools:
  - type: execute_sql
    database: test_db
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Tool without name should parse successfully: {:?}",
            result.err()
        );

        let agent = result.unwrap();
        let tools = match &agent.r#type {
            AgentType::Default(default_agent) => &default_agent.tools_config.tools,
            AgentType::Routing(_) => panic!("Expected Default agent, not Routing"),
        };
        assert_eq!(tools.len(), 1, "Should have one tool");

        // Check that the tool got a default name based on its type
        match &tools[0] {
            ToolType::ExecuteSQL(tool) => {
                assert_eq!(
                    tool.name, "execute_sql",
                    "Tool should get default name 'execute_sql'"
                );
            }
            _ => panic!("Expected ExecuteSQL tool"),
        }
    }

    #[test]
    fn test_agent_tools_duplicate_names_rejected() {
        use garde::Validate;
        use oxy::config::validate::AgentValidationContext;

        // Test that duplicate tool names are rejected during validation
        let yaml = r#"
model: gpt-4
system_instructions: "You are a helpful assistant"
tools:
  - type: execute_sql
    name: my_tool
    database: test_db
  - type: validate_sql
    name: my_tool
    database: test_db
"#;
        let agent: AgentConfig = parse_yaml(yaml).expect("Should parse at serde level");

        // Validation should fail due to duplicate tool names
        let config = create_test_config();
        let context = AgentValidationContext {
            agent_config: agent.clone(),
            config,
        };
        let result = agent.validate_with(&context);
        assert!(
            result.is_err(),
            "Should fail validation with duplicate tool names"
        );

        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("duplicate") || err_str.contains("my_tool"),
            "Error should mention duplicate tool name, got: {err_str}"
        );
    }

    #[test]
    fn test_agent_tools_default_names_can_duplicate_if_different_types() {
        // When tools don't have explicit names, they get default names based on type.
        // Two tools of the same type without names should get the same default name,
        // which should be rejected as duplicate.
        let yaml = r#"
model: gpt-4
system_instructions: "You are a helpful assistant"
tools:
  - type: execute_sql
    database: db1
  - type: execute_sql
    database: db2
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Multiple tools of same type should parse: {:?}",
            result.err()
        );

        use garde::Validate;
        use oxy::config::validate::AgentValidationContext;

        let agent = result.unwrap();
        let config = create_test_config();
        let context = AgentValidationContext {
            agent_config: agent.clone(),
            config,
        };
        let result = agent.validate_with(&context);

        // Should fail because both tools have the same default name "execute_sql"
        assert!(
            result.is_err(),
            "Should fail validation when multiple tools have same default name"
        );
    }

    #[test]
    fn test_agent_with_variables() {
        // Test that flatten for Variables works
        let yaml = r#"
model: gpt-4
system_instructions: "You are a helpful assistant"
variables:
  query:
    type: string
    default: "test"
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Agent with variables should parse: {:?}",
            result.err()
        );

        let agent = result.unwrap();
        assert!(agent.variables.is_some(), "Variables should be parsed");
    }

    // NOTE: AgentConfig cannot use deny_unknown_fields due to flatten on untagged enum (AgentType).
    // The untagged enum deserialization is incompatible with strict field checking.
    // This test documents the limitation - unknown fields are NOT rejected for AgentConfig.
    #[test]
    fn test_agent_unknown_fields_not_rejected_due_to_flatten_limitation() {
        let yaml = r#"
model: gpt-4
system_instructions: "You are a helpful assistant"
unknown_field: "this will be silently ignored"
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        // Due to flatten + untagged enum, deny_unknown_fields cannot be added
        // This test documents that unknown fields are accepted (not ideal, but a known limitation)
        assert!(
            result.is_ok(),
            "AgentConfig accepts unknown fields due to flatten limitation: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_agent_with_all_flatten_fields() {
        // Test that both flatten fields (r#type and variables) work together
        let yaml = r#"
model: gpt-4
system_instructions: "You are a helpful assistant"
tools:
  - type: execute_sql
    name: sql
    database: test_db
variables:
  filter:
    type: string
    default: "all"
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Agent with all flatten fields should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_routing_agent_with_variables() {
        // Test routing agent type with variables
        let yaml = r#"
model: gpt-4
type: routing
system_instructions: "Route queries appropriately"
routes:
  - agent1.agent.yml
variables:
  filter:
    type: string
    default: "all"
"#;
        let result: Result<AgentConfig, _> = parse_yaml(yaml);
        assert!(
            result.is_ok(),
            "Routing agent with variables should parse: {:?}",
            result.err()
        );
    }
}

// =============================================================================
// Integration Tests for Validation Context
// =============================================================================

mod garde_validation_tests {
    use super::*;
    use garde::Validate;
    use oxy::config::model::Config;

    /// Create a minimal valid Config for validation context
    fn create_test_config() -> Config {
        let yaml = r#"
models:
  - name: test_model
    vendor: openai
    model_ref: gpt-4
    key_var: OPENAI_API_KEY
databases:
  - name: test_db
    type: bigquery
    project: test_project
    credentials_path: /tmp/creds.json
"#;
        parse_yaml(yaml).expect("Test config should parse")
    }

    // Verifies that empty tasks fail garde validation
    #[test]
    fn test_workflow_empty_tasks_fails_garde_validation() {
        use oxy::config::model::Workflow;

        let yaml = r#"
tasks: []
"#;
        let workflow: Workflow = parse_yaml(yaml).expect("Should parse at serde level");

        let config = create_test_config();
        let context = oxy::config::validate::ValidationContext {
            config,
            metadata: None,
        };

        let result = workflow.validate_with(&context);
        assert!(result.is_err(), "Empty tasks should fail garde validation");

        // Verify the error mentions length/min requirement
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("length") || err_str.contains("tasks"),
            "Error should mention tasks length validation, got: {err_str}"
        );
    }

    #[test]
    fn test_app_empty_tasks_fails_garde_validation() {
        use oxy::config::model::AppConfig;
        use oxy::config::validate::{DataAppValidationContext, ValidationContextMetadata};

        let yaml = r#"
tasks: []
display:
  - type: table
    data: query
"#;
        let app: AppConfig = parse_yaml(yaml).expect("Should parse at serde level");

        let config = create_test_config();
        let context = oxy::config::validate::ValidationContext {
            config,
            metadata: Some(ValidationContextMetadata::DataApp(
                DataAppValidationContext {
                    app_config: app.clone(),
                },
            )),
        };

        let result = app.validate_with(&context);
        assert!(result.is_err(), "Empty tasks should fail garde validation");

        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("length") || err_str.contains("tasks"),
            "Error should mention tasks length validation, got: {err_str}"
        );
    }

    #[test]
    fn test_app_empty_display_fails_garde_validation() {
        use oxy::config::model::AppConfig;
        use oxy::config::validate::{DataAppValidationContext, ValidationContextMetadata};

        let yaml = r#"
tasks:
  - name: query
    type: execute_sql
    database: test_db
    sql_query: "SELECT 1"
display: []
"#;
        let app: AppConfig = parse_yaml(yaml).expect("Should parse at serde level");

        let config = create_test_config();
        let context = oxy::config::validate::ValidationContext {
            config,
            metadata: Some(ValidationContextMetadata::DataApp(
                DataAppValidationContext {
                    app_config: app.clone(),
                },
            )),
        };

        let result = app.validate_with(&context);
        assert!(
            result.is_err(),
            "Empty display should fail garde validation"
        );

        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("length") || err_str.contains("display"),
            "Error should mention display length validation, got: {err_str}"
        );
    }

    #[test]
    fn test_app_invalid_display_data_reference_fails_validation() {
        use oxy::config::model::AppConfig;
        use oxy::config::validate::{DataAppValidationContext, ValidationContextMetadata};

        let yaml = r#"
tasks:
  - name: sql_query
    type: execute_sql
    database: test_db
    sql_query: "SELECT 1"
display:
  - type: table
    data: wrong_task_name
"#;
        let app: AppConfig = parse_yaml(yaml).expect("Should parse at serde level");

        let config = create_test_config();
        let context = oxy::config::validate::ValidationContext {
            config,
            metadata: Some(ValidationContextMetadata::DataApp(
                DataAppValidationContext {
                    app_config: app.clone(),
                },
            )),
        };

        let result = app.validate_with(&context);
        assert!(
            result.is_err(),
            "Invalid display data reference should fail validation"
        );

        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("wrong_task_name") || err_str.contains("does not exist"),
            "Error should mention the invalid task reference, got: {err_str}"
        );
    }

    #[test]
    fn test_app_valid_config_passes_validation() {
        use oxy::config::model::AppConfig;
        use oxy::config::validate::{DataAppValidationContext, ValidationContextMetadata};

        let yaml = r#"
tasks:
  - name: sql_query
    type: execute_sql
    database: test_db
    sql_query: "SELECT 1"
display:
  - type: table
    data: sql_query
"#;
        let app: AppConfig = parse_yaml(yaml).expect("Should parse at serde level");

        let config = create_test_config();
        let context = oxy::config::validate::ValidationContext {
            config,
            metadata: Some(ValidationContextMetadata::DataApp(
                DataAppValidationContext {
                    app_config: app.clone(),
                },
            )),
        };

        let result = app.validate_with(&context);
        assert!(
            result.is_ok(),
            "Valid app config should pass validation: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_app_dot_notation_reference_with_valid_workflow_passes() {
        use oxy::config::model::AppConfig;
        use oxy::config::validate::{DataAppValidationContext, ValidationContextMetadata};

        // Dot notation like "my_workflow.task_output" should pass if "my_workflow" exists as a task
        let yaml = r#"
tasks:
  - name: my_workflow
    type: workflow
    src: workflows/test.workflow.yml
display:
  - type: table
    data: my_workflow.nested_task
"#;
        let app: AppConfig = parse_yaml(yaml).expect("Should parse at serde level");

        let config = create_test_config();
        let context = oxy::config::validate::ValidationContext {
            config,
            metadata: Some(ValidationContextMetadata::DataApp(
                DataAppValidationContext {
                    app_config: app.clone(),
                },
            )),
        };

        let result = app.validate_with(&context);
        assert!(
            result.is_ok(),
            "Dot notation with valid workflow name should pass: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_app_dot_notation_reference_with_invalid_workflow_fails() {
        use oxy::config::model::AppConfig;
        use oxy::config::validate::{DataAppValidationContext, ValidationContextMetadata};

        // Dot notation like "typo_workflow.task_output" should fail if "typo_workflow" doesn't exist
        let yaml = r#"
tasks:
  - name: my_workflow
    type: workflow
    src: workflows/test.workflow.yml
display:
  - type: table
    data: typo_workflow.nested_task
"#;
        let app: AppConfig = parse_yaml(yaml).expect("Should parse at serde level");

        let config = create_test_config();
        let context = oxy::config::validate::ValidationContext {
            config,
            metadata: Some(ValidationContextMetadata::DataApp(
                DataAppValidationContext {
                    app_config: app.clone(),
                },
            )),
        };

        let result = app.validate_with(&context);
        assert!(
            result.is_err(),
            "Dot notation with invalid workflow name should fail"
        );

        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("typo_workflow") || err_str.contains("does not exist"),
            "Error should mention the invalid workflow reference, got: {err_str}"
        );
    }
}
